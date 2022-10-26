use crate::MessageKind;
use rillrate::prime::{Click, ClickOpts};
use rust_embed::RustEmbed;
use std::{cmp::max, time::Duration};
use tokio::{
    sync::mpsc,
    time::{sleep, Instant},
};

use super::{
    show::{Frame, Laser, Show},
    LaserDataFrame, ShowAsset, MAX_LIGHTS, MAX_PROJECTORS,
};

pub struct ShowManager {
    pub current_show: Option<Show>,
    pub start_time: Option<Instant>,
    pub show_buttons: Option<Vec<Click>>,
    pub message_queue: Option<mpsc::Sender<MessageKind>>,
}

impl ShowManager {
    pub fn new() -> Self {
        Self {
            current_show: None,
            start_time: None,
            message_queue: None,
            show_buttons: None,
        }
    }

    pub fn load_show(show_file_contents: String, message_queue: mpsc::Sender<MessageKind>) -> Self {
        let message_queue_clone = message_queue.clone();

        ShowManager {
            current_show: Some(Show::load_show(show_file_contents)),
            message_queue: Some(message_queue),
            show_buttons: Some(ShowManager::load_shows(message_queue_clone)),
            start_time: None,
        }
    }

    pub fn save_show(&self, show: Show) -> String {
        let mut file_json = json::JsonValue::new_object();

        file_json["song"] = show.song.into();

        for frame in show.frames {
            let timestamp = frame.timestamp.to_string();
            file_json["timestamps"][&timestamp] = json::JsonValue::new_object();

            for (i, light) in frame.lights.iter().enumerate() {
                let light_name = format!("light-{}", i);
                if let Some(light) = light {
                    file_json["timestamps"][&timestamp][&light_name] = match light {
                        true => 1.0.into(),
                        false => 0.0.into(),
                    };
                }
            }

            for (i, laser) in frame.lasers.iter().enumerate() {
                let laser_name = format!("laser-{}", i);
                let laser_config_name = format!("laser-{}-config", i);
                if let Some(laser) = laser {
                    file_json["timestamps"][&timestamp][&laser_config_name] =
                        json::JsonValue::new_object();
                    file_json["timestamps"][&timestamp][&laser_config_name]["home"] =
                        json::JsonValue::Boolean(laser.home);
                    file_json["timestamps"][&timestamp][&laser_config_name]["speed-profile"] =
                        json::JsonValue::Boolean(laser.speed_profile);

                    file_json["timestamps"][&timestamp][&laser_name] = json::JsonValue::new_array();

                    for laser_frame in &laser.data_frame {
                        file_json["timestamps"][&timestamp][&laser_name]
                            .push(json::JsonValue::new_array());
                        let last_index = file_json["timestamps"][&timestamp][&laser_name].len() - 1;
                        file_json["timestamps"][&timestamp][&laser_name][last_index]
                            .push(laser_frame.x_pos);
                        file_json["timestamps"][&timestamp][&laser_name][last_index]
                            .push(laser_frame.y_pos);
                        file_json["timestamps"][&timestamp][&laser_name][last_index]
                            .push(laser_frame.r);
                        file_json["timestamps"][&timestamp][&laser_name][last_index]
                            .push(laser_frame.g);
                        file_json["timestamps"][&timestamp][&laser_name][last_index]
                            .push(laser_frame.b);
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

    pub async fn start_show(mut self) {
        // Set the timer
        self.start_time = Some(Instant::now());

        // Start the song
        self.message_queue
            .as_ref()
            .unwrap()
            .try_send(MessageKind::InternalMessage(
                crate::InternalMessage::Audio {
                    audio_file_contents: self.current_show.as_ref().unwrap().song.clone(),
                },
            ))
            .unwrap();

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

                // Print the message queue size
                println!(
                    "Message queue size: {}",
                    self.message_queue.as_ref().unwrap().capacity()
                );

                // Send all the lasers data
                // ...

                if self.current_show.as_ref().unwrap().frames.len() == 0 {
                    break;
                }
            }
        });
    }

    pub fn load_shows(message_queue: mpsc::Sender<MessageKind>) -> Vec<Click> {
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
        let clicks = names
            .iter()
            .map(|name| {
                // Load the show file
                let show_file =
                    std::fs::read_to_string(format!("shows/{}/instructions.json", name)).unwrap();

                // Set up the buttons on the dashboard
                let click = Click::new(
                    format!("app.dashboard.Shows.{}", name),
                    ClickOpts::default().label("Start"),
                );
                let this = click.clone();

                let message_queue_clone = message_queue.clone();
                click.sync_callback(move |envelope| {
                    // Start loading that song
                    this.apply();
                    Ok(())
                });

                click
            })
            .collect::<Vec<Click>>();

        clicks
    }
}
