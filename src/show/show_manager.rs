use crate::{
    prelude::{pack::HeaderPack, Audio, MessageSendPack},
    InternalMessage, MessageKind,
};
use packed_struct::debug_fmt;
use rillrate::prime::{Click, ClickOpts};
use rust_embed::RustEmbed;
use std::{
    cmp::max,
    collections::{HashMap, VecDeque},
    sync::Arc,
    thread,
    time::Duration,
};
use tokio::{
    sync::{mpsc, Mutex},
    time::{sleep, Instant},
};

use super::{
    prelude::{LoadedShow, UnloadedShow},
    show::Show,
    ShowAsset,
};

pub type SongName = String;
pub type ShowMap = HashMap<SongName, UnloadedShow>;


pub struct ShowManager {
    pub current_show: LoadedShow,
    pub next_show: Option<LoadedShow>,
    /// This stores what is going to happen next
    ///
    /// TODO: Add a way to add transitions between songs for the audio to
    /// overlap
    pub show_queue: Vec<ShowElement>,
    pub start_time: Option<Instant>,
    pub shows: ShowMap,
    pub message_queue: Option<mpsc::Sender<MessageKind>>,
}

/// There are several states to be in:
/// - There is a show playing
/// - A show just ended and another is starting right away
/// - A show just ended and there is a break before the next one
pub enum ShowState {
    
}

#[derive(Debug, Clone)]
pub enum ShowElement {
    Home, // Wait 15 seconds after, then assume the show can start
    Show { show_id: usize },
    // Disable {duration: Duration},
    NullOut, // Send header with 0 frames, then 50 frames of 00000000, wait 3 seconds before homing again
    // BoundaryCheck,
    Idle { time: u64 },
    Transition { show_id: usize },
}

const HOME_SLEEP_TIME: u64 = 15;

impl ShowManager {
    pub fn new(shows: ShowMap, sender: Option<mpsc::Sender<MessageKind>>) -> Self {
        Self {
            current_show: None,
            next_show: None,
            start_time: None,
            message_queue: sender,
            shows,
            show_queue: Vec::new(),
        }
    }

    pub fn load_show(show_file_contents: String, message_queue: mpsc::Sender<MessageKind>) -> Self {
        let message_queue_clone = message_queue.clone();

        ShowManager {
            current_show: UnloadedShow::load_show_file(show_file_contents),
            next_show: None,
            message_queue: Some(message_queue),
            shows: ShowManager::load_shows(message_queue_clone),
            start_time: None,
            show_queue: Vec::new(),
        }
    }

    pub fn save_show(&self, show: UnloadedShow) -> String {
        let mut file_json = json::JsonValue::new_object();

        for frame in show.frames {
            let timestamp = frame.timestamp.to_string();
            file_json[&timestamp] = json::JsonValue::new_object();

            for (i, light) in frame.lights.iter().enumerate() {
                let light_name = format!("light-{}", i);
                if let Some(light) = light {
                    file_json[&timestamp][&light_name] = match light {
                        true => 1.0.into(),
                        false => 0.0.into(),
                    };
                }
            }

            for (i, laser) in frame.lasers.iter().enumerate() {
                let laser_name = format!("laser-{}", i);
                let laser_config_name = format!("laser-{}-config", i);
                if let Some(laser) = laser {
                    file_json[&timestamp][&laser_config_name] = json::JsonValue::new_object();
                    file_json[&timestamp][&laser_config_name]["home"] =
                        json::JsonValue::Boolean(laser.home);
                    file_json[&timestamp][&laser_config_name]["speed-profile"] =
                        json::JsonValue::Number(laser.speed_profile.into());

                    file_json[&timestamp][&laser_name] = json::JsonValue::new_array();

                    for laser_frame in &laser.data_frame {
                        file_json[&timestamp][&laser_name]
                            .push(json::JsonValue::new_array())
                            .unwrap();
                        let last_index = file_json[&timestamp][&laser_name].len() - 1;
                        file_json[&timestamp][&laser_name][last_index]
                            .push(laser_frame.x_pos)
                            .unwrap();
                        file_json[&timestamp][&laser_name][last_index]
                            .push(laser_frame.y_pos)
                            .unwrap();
                        file_json[&timestamp][&laser_name][last_index]
                            .push(laser_frame.r)
                            .unwrap();
                        file_json[&timestamp][&laser_name][last_index]
                            .push(laser_frame.g)
                            .unwrap();
                        file_json[&timestamp][&laser_name][last_index]
                            .push(laser_frame.b)
                            .unwrap();
                    }
                }
            }
        }

        file_json.pretty(4)
    }

    pub fn load_show_file(
        show_file_path: String,
        message_queue: mpsc::Sender<MessageKind>,
    ) -> Self {
        // let show_file_contents = std::fs::read_to_string(show_file_path).unwrap();
        let show_file_contents: String =
            std::str::from_utf8(&ShowAsset::get(&show_file_path).unwrap().data)
                .unwrap()
                .to_string();

        ShowManager::load_show(show_file_contents, message_queue)
    }

    /// This function starts a thread that will manage the show. It will keep a
    /// list of upcoming shows to play, and it will send messages to the other
    /// worker threads for the projector, lights, and audio.
    pub async fn start_show_worker(mut self, mut receiver: mpsc::Receiver<Vec<ShowElement>>) {
        let show_job_queue: Arc<Mutex<VecDeque<ShowElement>>> =
            Arc::new(Mutex::new(VecDeque::new()));

        // Start a thread to add jobs to the queue
        let show_job_queue_clone = show_job_queue.clone();
        let queue_handle = tokio::spawn(async move {
            while let Some(show_job_list) = receiver.recv().await {
                let mut show_job_queue = show_job_queue_clone.lock().await;
                show_job_queue.extend(show_job_list);
            }
        });

        // Start the show worker thread
        let show_job_queue_clone = show_job_queue.clone();
        let worker_handle = tokio::spawn(async move {
            loop {
                // Get the next element in the queue
                let mut show_job_queue = show_job_queue_clone.lock().await;
                let next_show_element = show_job_queue.pop_front().to_owned();

                // If there isn't a next element, wait for one
                if next_show_element.is_none() {
                    drop(show_job_queue);
                    sleep(Duration::from_millis(100)).await;
                    continue;
                }

                match next_show_element.unwrap() {
                    ShowElement::Home => {
                        // Send a home command
                        self.message_queue
                            .as_mut()
                            .unwrap()
                            .send(MessageKind::InternalMessage(InternalMessage::Projector(
                                MessageSendPack {
                                    header: HeaderPack {
                                        projector_id: 15.into(),
                                        point_count: 0.into(),
                                        home: true,
                                        enable: true,
                                        configuration_mode: false,
                                        draw_boundary: false,
                                        oneshot: false,
                                        speed_profile: 0.into(),
                                        ..Default::default()
                                    },
                                    draw_instructions: Vec::new(),
                                }
                                .into(),
                            )))
                            .await
                            .unwrap();

                        // Sleep for 15 seconds
                        sleep(Duration::from_secs(HOME_SLEEP_TIME)).await;
                    }
                    ShowElement::Show { show_id } => {
                        // Set the show from the id
                        self.current_show = self.shows[show_id].clone();

                        // Set the timer
                        self.start_time = Some(Instant::now());

                        // Start the song
                        let song = &self.current_show.song;
                        if let Some(message_queue) = self.message_queue.as_ref() {
                            message_queue
                                .try_send(MessageKind::InternalMessage(InternalMessage::Audio {
                                    audio_file_contents: Arc::new(song.clone()),
                                }))
                                .unwrap();
                        }

                        loop {
                            // Get the next frame
                            let curr_frame = self.current_show.frames.remove(0);

                            // Sleep until the current frame is ready
                            let curr_time = self.start_time.unwrap().elapsed().as_millis() as i64;
                            let sleep_time = max(curr_frame.timestamp as i64 - curr_time, 0);
                            sleep(Duration::from_millis(sleep_time as u64)).await;

                            // Execute the current frame

                            // Send all the lights data
                            for (i, light) in curr_frame.lights.iter().enumerate() {
                                if let Some(light) = light {
                                    self.message_queue
                                        .as_mut()
                                        .unwrap()
                                        .send(MessageKind::InternalMessage(
                                            InternalMessage::Light {
                                                light_id: i as u8 + 1,
                                                enable: *light,
                                            },
                                        ))
                                        .await
                                        .unwrap();
                                }
                            }

                            // Send all the lasers data
                            for (i, laser) in curr_frame.lasers.iter().enumerate() {
                                if let Some(laser) = laser {
                                    self.message_queue
                                        .as_mut()
                                        .unwrap()
                                        .send(MessageKind::InternalMessage(
                                            InternalMessage::Projector(
                                                MessageSendPack {
                                                    header: HeaderPack {
                                                        projector_id: (i as u8).into(),
                                                        point_count: (laser.data_frame.len() as u8)
                                                            .into(),
                                                        home: false,
                                                        enable: true,
                                                        configuration_mode: false,
                                                        draw_boundary: false,
                                                        oneshot: false,
                                                        speed_profile: laser.speed_profile.into(),
                                                        ..Default::default()
                                                    },
                                                    draw_instructions: Vec::new(),
                                                }
                                                .into(),
                                            ),
                                        ))
                                        .await
                                        .unwrap();
                                }
                            }

                            if self.current_show.frames.len() == 0 {
                                break;
                            }
                        }
                    }
                    ShowElement::NullOut => {
                        // Send a null out command
                        self.message_queue
                            .as_mut()
                            .unwrap()
                            .send(MessageKind::InternalMessage(InternalMessage::Projector(
                                MessageSendPack {
                                    header: HeaderPack {
                                        projector_id: 16.into(),
                                        point_count: 0.into(),
                                        home: false,
                                        enable: true,
                                        configuration_mode: false,
                                        draw_boundary: false,
                                        oneshot: false,
                                        speed_profile: 0.into(),
                                        ..Default::default()
                                    },
                                    draw_instructions: Vec::new(),
                                }
                                .into(),
                            )))
                            .await
                            .unwrap();

                        // Sleep for 3 seconds
                        sleep(Duration::from_secs(3)).await;
                    }
                    ShowElement::Idle { time } => {
                        // Sleep for the given time
                        sleep(Duration::from_secs(time)).await;
                    }
                    ShowElement::Transition { show_id } => todo!(),
                }
            }
        });
    }

    pub fn load_shows(message_queue: mpsc::Sender<MessageKind>) -> ShowMap {
        // Find all folders in the shows folder
        let shows = std::fs::read_dir("shows").unwrap();

        let names = shows
            .into_iter()
            .map(|show| {
                show.unwrap()
                    .path()
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_string()
            })
            .collect::<Vec<String>>();

        println!("Found shows: {:?}", names);

        // For each one, load the show and song
        let shows = names
            .iter()
            .map(|name| {
                // Get the name of all the files in the folder
                let files = std::fs::read_dir(format!("shows/{}", name)).unwrap();

                // Get all that begin with `instructions-`
                let instructions_files = files
                    .into_iter()
                    .filter(|file| {
                        file.as_ref()
                            .unwrap()
                            .path()
                            .file_name()
                            .unwrap()
                            .to_str()
                            .unwrap()
                            .starts_with("instructions-")
                    })
                    .collect::<Vec<_>>();

                // Get the name of all the files in the folder
                let files = std::fs::read_dir(format!("shows/{}", name)).unwrap();

                // Get all that that have a file format .mp3, keep only the path
                // of the first one
                let song_file_name = files
                    .into_iter()
                    .filter(|file| {
                        file.as_ref()
                            .unwrap()
                            .path()
                            .extension()
                            .unwrap()
                            .to_str()
                            .unwrap()
                            == "mp3"
                    })
                    .map(|file| {
                        file.as_ref()
                            .unwrap()
                            .path()
                            .file_stem()
                            .unwrap()
                            .to_str()
                            .unwrap()
                            .to_string()
                    })
                    .next()
                    .unwrap()
                    .clone();

                // Create a show for each one of these files
                let shows = instructions_files
                    .into_iter()
                    .filter_map(|file| {
                        if let Ok(file) = file {
                            // Load the frames
                            let mut show = UnloadedShow::load_show_file(&file.path());

                            // Set up the buttons on the dashboard
                            let click = Click::new(
                                format!("app.dashboard.Shows.{}", name),
                                ClickOpts::default().label("Start"),
                            );
                            let this = click.clone();

                            let _message_queue_clone = message_queue.clone();
                            click.sync_callback(move |_envelope| {
                                // Start loading that song
                                this.apply();
                                Ok(())
                            });

                            return Some((file.file_name().to_str().unwrap().to_string(), show));
                        }

                        None
                    })
                    .collect::<ShowMap>();

                shows
            })
            .flatten()
            .collect::<ShowMap>();

        shows
    }
}
