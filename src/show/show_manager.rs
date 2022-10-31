use crate::{
    prelude::{pack::HeaderPack, Audio, FrameSendPack, MessageSendPack},
    proto_schema::schema::Header,
    InternalMessage, MessageKind,
};
use rillrate::prime::{Click, ClickOpts};
use rust_embed::RustEmbed;
use std::{cmp::max, sync::Arc, time::Duration};
use tokio::{
    sync::mpsc,
    time::{sleep, Instant},
};

use super::{prelude::Laser, show::Show, ShowAsset};

pub struct ShowManager {
    pub current_show: Option<Show>,
    pub start_time: Option<Instant>,
    pub shows: Vec<Show>,
    pub message_queue: Option<mpsc::Sender<MessageKind>>,
}

impl ShowManager {
    pub fn new(shows: Vec<Show>, sender: Option<mpsc::Sender<MessageKind>>) -> Self {
        Self {
            current_show: None,
            start_time: None,
            message_queue: sender,
            shows: shows,
        }
    }

    pub fn load_show(show_file_contents: String, message_queue: mpsc::Sender<MessageKind>) -> Self {
        let message_queue_clone = message_queue.clone();

        ShowManager {
            current_show: Some(Show::load_show(show_file_contents)),
            message_queue: Some(message_queue),
            shows: ShowManager::load_shows(message_queue_clone),
            start_time: None,
        }
    }

    pub fn save_show(&self, show: Show) -> String {
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

    pub async fn start_show(mut self, id: u64) {
        // Set the show from the id
        self.current_show = Some(self.shows[id as usize].clone());

        // Set the timer
        self.start_time = Some(Instant::now());

        // Start the song
        if let Some(song) = &self.current_show.as_ref().unwrap().song {
            if let Some(message_queue) = self.message_queue.as_ref() {
                message_queue
                    .try_send(MessageKind::InternalMessage(
                        crate::InternalMessage::Audio {
                            audio_file_contents: Arc::new(song.clone()),
                        },
                    ))
                    .unwrap();
            }
        }

        // Start the show thread
        tokio::spawn(async move {
            loop {
                // Get the next frame
                let curr_frame = self.current_show.as_mut().unwrap().frames.remove(0);

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
                            .try_send(MessageKind::InternalMessage(
                                crate::InternalMessage::Light {
                                    light_id: i as u8 + 1,
                                    enable: *light,
                                },
                            ))
                            .unwrap();
                    }
                }

                // Send all the lasers data
                curr_frame.lasers.iter().enumerate().for_each(|(i, laser)| {
                    if let Some(laser) = laser {
                        self.message_queue
                            .as_mut()
                            .unwrap()
                            .try_send(MessageKind::InternalMessage(InternalMessage::Projector(
                                MessageSendPack {
                                    header: HeaderPack {
                                        projector_id: (i as u8).into(),
                                        point_count: (laser.data_frame.len() as u8).into(),
                                        home: false,
                                        enable: false,
                                        configuration_mode: false,
                                        draw_boundary: false,
                                        oneshot: false,
                                        speed_profile: laser.speed_profile.into(),
                                        ..Default::default()
                                    },
                                    draw_instructions: Vec::new(),
                                }
                                .into(),
                            )))
                            .unwrap();
                    }
                });

                if self.current_show.as_ref().unwrap().frames.len() == 0 {
                    break;
                }
            }
        });
    }

    pub fn load_shows(message_queue: mpsc::Sender<MessageKind>) -> Vec<Show> {
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

                let song = Audio::get_sound(&song_file_name).unwrap();

                // Create a show for each one of these files
                let shows = instructions_files
                    .into_iter()
                    .filter_map(|file| {
                        if let Ok(file) = file {
                            // Load the show file
                            let file_contents = std::fs::read_to_string(file.path()).unwrap();

                            // Load the frames
                            let mut show = Show::load_show(file_contents);

                            show.song = Some(song.clone());

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

                            return Some(show);
                        }

                        None
                    })
                    .collect::<Vec<_>>();

                shows
            })
            .flatten()
            .collect::<Vec<Show>>();

        shows
    }
}
