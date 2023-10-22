use crate::{
    prelude::{pack::HeaderPack, MessageSendPack},
    InternalMessage, MessageKind,
};
use log::{error, info};

use rand::seq::IteratorRandom;
// use rillrate::prime::{Click, ClickOpts};
use std::{
    cmp::max,
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::Duration,
};
use tokio::{
    sync::{mpsc, Mutex},
    time::{sleep, Instant},
};

use super::prelude::{LoadedShow, LoadingShow, UnloadedShow};

pub type ShowName = String;
pub type ShowMap = HashMap<ShowName, UnloadedShow>;

pub struct ShowManager {
    pub current_show: Option<LoadedShow>,
    pub next_show: Option<LoadingShow>,
    /// This stores what is going to happen next
    ///
    /// TODO: Add a way to add transitions between songs for the audio to
    /// overlap
    pub show_queue: Vec<ShowElement>,
    pub start_time: Option<Instant>,
    pub shows: ShowMap,
    pub message_queue: mpsc::Sender<MessageKind>,
}

/// There are several states to be in:
/// - There is a show playing
/// - A show just ended and another is starting right away
/// - A show just ended and there is a break before the next one
///
/// Regardless, there is a chance that there is not a "song" that is currently
/// loaded in the current_show
pub enum ShowState {}

#[derive(Debug, Clone)]
pub enum ShowElement {
    /// Wait 15 seconds after, then assume the show can start
    Home,
    /// Start loading a show into the next show slot. This will take place on a
    /// separate thread
    PrepareShow(ShowChoice),
    /// Pull the next show into the current show and start it. This will wait
    /// until the new song is loaded
    NextShow,
    // Disable {duration: Duration},
    // Send header with 0 frames, then 50 frames of 00000000, wait 3 seconds
    // before homing again
    NullOut,
    // BoundaryCheck,
    Idle {
        time: u64,
    },
    Transition {
        show_id: usize,
    },
}

#[derive(Debug, Clone)]
pub enum ShowChoice {
    Name(ShowName),
    Random,
}

const HOME_SLEEP_TIME: u64 = 15;

impl ShowManager {
    pub fn new(shows: ShowMap, sender: mpsc::Sender<MessageKind>) -> Self {
        Self {
            current_show: None,
            next_show: None,
            start_time: None,
            message_queue: sender,
            shows,
            show_queue: Vec::new(),
        }
    }

    // pub fn load_show(show_file_contents: String, message_queue: mpsc::Sender<MessageKind>) -> Self {
    //     let message_queue_clone = message_queue.clone();

    //     ShowManager {
    //         current_show: UnloadedShow::load_show_file(show_file_contents),
    //         next_show: None,
    //         message_queue: Some(message_queue),
    //         shows: ShowManager::load_shows(message_queue_clone),
    //         start_time: None,
    //         show_queue: Vec::new(),
    //     }
    // }

    pub fn save_show(show: UnloadedShow) -> String {
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

    // pub fn load_show_file(
    //     show_file_path: String,
    //     message_queue: mpsc::Sender<MessageKind>,
    // ) -> Self {
    //     // let show_file_contents = std::fs::read_to_string(show_file_path).unwrap();
    //     let show_file_contents: String =
    //         std::str::from_utf8(&ShowAsset::get(&show_file_path).unwrap().data)
    //             .unwrap()
    //             .to_string();

    //     ShowManager::load_show(show_file_contents, message_queue)
    // }

    /// This function starts a thread that will manage the show. It will keep a
    /// list of upcoming shows to play, and it will send messages to the other
    /// worker threads for the projector, lights, and audio.
    pub async fn start_show_worker(self, mut receiver: mpsc::Receiver<Vec<ShowElement>>) {
        let show_job_queue: Arc<Mutex<VecDeque<ShowElement>>> =
            Arc::new(Mutex::new(VecDeque::new()));

        // Start a thread to add jobs to the queue
        let show_job_queue_clone = show_job_queue.clone();
        let _queue_handle = tokio::spawn(async move {
            while let Some(show_job_list) = receiver.recv().await {
                let mut show_job_queue = show_job_queue_clone.lock().await;
                show_job_queue.extend(show_job_list);
            }
        });

        // Start the show worker thread
        let show_job_queue_clone = show_job_queue.clone();
        let _worker_handle =
            tokio::spawn(async move { show_task_loop(self, show_job_queue_clone).await });
    }

    pub fn load_shows(_message_queue: mpsc::Sender<MessageKind>) -> ShowMap {
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

        info!("Found shows: {:?}", names);

        // For each one, load the show and song
        let shows = names
            .iter()
            .flat_map(|name| {
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
                let _song_file_name = files
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

                instructions_files
                    .into_iter()
                    .filter_map(|file| {
                        if let Ok(file) = file {
                            // Load the frames
                            let show = UnloadedShow::load_show_file(&file.path());

                            // // Set up the buttons on the dashboard
                            // let click = Click::new(
                            //     format!("app.dashboard.Shows.{}", name),
                            //     ClickOpts::default().label("Start"),
                            // );
                            // let this = click.clone();

                            // let _message_queue_clone = message_queue.clone();
                            // click.sync_callback(move |_envelope| {
                            //     // Start loading that song
                            //     this.apply();
                            //     Ok(())
                            // });

                            return Some((file.file_name().to_str().unwrap().to_string(), show));
                        }

                        None
                    })
                    .collect::<ShowMap>()
            })
            .collect::<ShowMap>();

        shows
    }
}

async fn show_task_loop(
    mut show_manager: ShowManager,
    show_job_queue_clone: Arc<Mutex<VecDeque<ShowElement>>>,
) {
    let mut now: Option<Instant> = None;
    loop {
        // Get the next element in the queue
        let mut show_job_queue = show_job_queue_clone.lock().await;
        let next_show_element = show_job_queue.pop_front().to_owned();

        // If nothing is playing, then we should move on to the next song if
        // there is one in next_show
        if show_manager.current_show.is_none() {
            if show_manager.next_show.is_some() {
                show_job_queue.push_back(ShowElement::NextShow);
            }
        }

        // If there isn't a next element, wait for one. If we've slept for more
        // than 5 seconds, add an instruction to prepare a random show.
        if next_show_element.is_none() {
            drop(show_job_queue);

            // If we don't have a next song loaded, then we should see if we
            // should queue a random one up
            if show_manager.next_show.is_none() {
                // If 5 seconds has elapsed, add a random show to the queue
                if now.is_none() {
                    now = Some(Instant::now());
                } else if now.unwrap().elapsed().as_secs() > 5 {
                    info!("Adding a random show to the queue");

                    show_job_queue_clone
                        .lock()
                        .await
                        .push_back(ShowElement::PrepareShow(ShowChoice::Random));

                    info!("{:?}", &show_manager.current_show);

                    // // If there isn't a current show, then we should start the
                    // // next show right away
                    // if show_manager.current_show.is_none() {
                    //     show_job_queue_clone
                    //         .lock()
                    //         .await
                    //         .push_back(ShowElement::NextShow);
                    // }
                }

                // Either way, it's fine to sleep for a bit
                sleep(Duration::from_millis(100)).await;
                continue;
            }
        } else {
            // Print the details about the current show manager
            info!(
                "Current show: {:?}, Next show: {:?}, Queue {:?}",
                match show_manager.current_show {
                    Some(ref show) => show.name.clone(),
                    None => "None".to_string(),
                },
                match show_manager.next_show {
                    Some(ref show) => show.name.clone(),
                    None => "None".to_string(),
                },
                show_job_queue
            );

            // Reset the timer
            now = None;

            match next_show_element.unwrap() {
                ShowElement::Home => {
                    // Send a home command
                    info!("Homing the projector");
                    show_manager
                        .message_queue
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

                    // Add a 15 second idle command to the beginning of the
                    // queue
                    show_job_queue.push_front(ShowElement::Idle {
                        time: HOME_SLEEP_TIME,
                    });
                }
                ShowElement::PrepareShow(choice) => {
                    info!("Preparing a show");
                    match choice {
                        ShowChoice::Name(show_name) => {
                            // Set the show from the name
                            let unloaded_show = match show_manager.shows.get(&show_name) {
                                Some(show) => show.clone(),
                                None => {
                                    error!("Show {} not found", show_name);
                                    continue;
                                }
                            };

                            // Turn it into a loading show
                            let loading_show = unloaded_show.load_show().await;

                            // Set the next show
                            show_manager.next_show = Some(loading_show);
                        }
                        ShowChoice::Random => {
                            // Pick a random show from the show manager list
                            let unloaded_show =
                                match show_manager.shows.values().choose(&mut rand::thread_rng()) {
                                    Some(show) => show.clone(),
                                    None => {
                                        error!("There are no shows to play");
                                        continue;
                                    }
                                };

                            // Turn it into a loading show
                            let loading_show = unloaded_show.load_show().await;

                            // Set the next show
                            show_manager.next_show = Some(loading_show);
                        }
                    }
                }
                ShowElement::NextShow => {
                    info!("Starting the next show");

                    // Set the timer
                    show_manager.start_time = Some(Instant::now());

                    const TIMEOUT: u64 = 10;

                    // If there isn't a next_show, then log that there isn't one and
                    // continue to the next instruction
                    if show_manager.next_show.is_none() {
                        error!("There is no next show to play");
                        continue;
                    }

                    let next_show = show_manager.next_show.take().unwrap();

                    // Load the song. If there is no song loaded, wait on it
                    // appearing for TIMEOUT seconds. If it doesn't appear, then
                    // log that the song wasn't loaded in time, and continue to the
                    // next instruction.
                    let timer = Instant::now();
                    loop {
                        if next_show.is_ready() {
                            break;
                        }

                        if timer.elapsed().as_secs() > TIMEOUT {
                            error!("There was no next show loaded in time");
                            break;
                        }

                        sleep(Duration::from_millis(100)).await;
                    }

                    // Turn this into a loaded show
                    let loaded_show = match next_show.get_loaded_show() {
                        Ok(show) => show,
                        Err(_) => {
                            error!("The next show is not ready to play");
                            continue;
                        }
                    };

                    // Set the current show
                    show_manager.current_show = Some(loaded_show);

                    // Get the show
                    let current_show = show_manager.current_show.as_ref().unwrap();

                    // Start the song
                    let song = current_show.song.clone();
                    show_manager
                        .message_queue
                        .try_send(MessageKind::InternalMessage(InternalMessage::Audio {
                            audio_file_contents: song,
                        }))
                        .unwrap();

                    let mut frames_iter = current_show.frames.iter();

                    loop {
                        // Get the next frame
                        let curr_frame = match frames_iter.next() {
                            Some(frame) => frame,
                            None => break,
                        };

                        // Sleep until the current frame is ready
                        let curr_time =
                            show_manager.start_time.unwrap().elapsed().as_millis() as i64;
                        let sleep_time = max(curr_frame.timestamp as i64 - curr_time, 0);
                        sleep(Duration::from_millis(sleep_time as u64)).await;

                        // Execute the current frame

                        // Send all the lights data
                        for (i, light) in curr_frame.lights.iter().enumerate() {
                            if let Some(light) = light {
                                show_manager
                                    .message_queue
                                    .send(MessageKind::InternalMessage(InternalMessage::Light {
                                        light_id: i as u8 + 1,
                                        enable: *light,
                                    }))
                                    .await
                                    .unwrap();
                            }
                        }

                        // Send all the lasers data
                        for (i, laser) in curr_frame.lasers.iter().enumerate() {
                            if let Some(laser) = laser {
                                show_manager
                                    .message_queue
                                    .send(MessageKind::InternalMessage(InternalMessage::Projector(
                                        MessageSendPack::new(
                                            HeaderPack {
                                                projector_id: (i as u8).into(),
                                                point_count: (laser.data_frame.len() as u8).into(),
                                                home: false,
                                                enable: true,
                                                configuration_mode: false,
                                                draw_boundary: false,
                                                oneshot: false,
                                                speed_profile: laser.speed_profile.into(),
                                                ..Default::default()
                                            },
                                            laser.data_frame.clone(),
                                        )
                                        .into(),
                                    )))
                                    .await
                                    .unwrap();
                            }
                        }
                    }
                }
                ShowElement::NullOut => {
                    info!("Nulling out the projector");

                    // Send a null out command
                    show_manager
                        .message_queue
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
                    // Sleep for the given time. Print once a second that we're
                    // still sleeping.
                    info!("Idling for {} seconds", time);

                    let mut time_remaining = time;

                    while time_remaining > 0 {
                        // If we're on the pi, wait a full second, otherwise
                        // wait 100ms
                        if !cfg!(feature = "pi") {
                            sleep(Duration::from_millis(100)).await;
                        } else {
                            sleep(Duration::from_secs(1)).await;
                        }
                        time_remaining -= 1;
                        info!("{} seconds remaining", time_remaining);
                    }
                }
                ShowElement::Transition { show_id: _ } => todo!(),
            }
        }
    }
}
