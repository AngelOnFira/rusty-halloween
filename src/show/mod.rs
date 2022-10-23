use std::{cmp::max, time::Duration};

use tokio::{
    sync::mpsc,
    time::{sleep, Instant},
};

use crate::MessageKind;

pub struct ShowManager {
    pub current_show: Option<Show>,
    pub start_time: Option<Instant>,
    pub message_queue: Option<mpsc::Sender<MessageKind>>,
}

pub struct Show {
    pub song: String,
    pub frames: Vec<Frame>,
}

pub struct Frame {
    pub timestamp: u64,
    pub lights: Vec<Option<bool>>,
    pub lasers: Vec<Option<Laser>>,
}

pub struct Laser {
    // Laser conf
    pub home: bool,
    pub speed_profile: bool,
    // Laser
    pub data_frame: Vec<LaserDataFrame>,
}

#[derive(Debug)]
pub struct LaserDataFrame {
    pub x_pos: u16,
    pub y_pos: u16,
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

pub const MAX_LIGHTS: usize = 7;
pub const MAX_PROJECTORS: usize = 5;

impl ShowManager {
    pub fn new() -> Self {
        Self {
            current_show: None,
            start_time: None,
            message_queue: None,
        }
    }

    pub fn load_show(show_file_contents: String, message_queue: mpsc::Sender<MessageKind>) -> Self {
        // Load as json
        let file_json = json::parse(&show_file_contents).unwrap();

        // Get the song name
        let song = file_json["song"].as_str().unwrap().to_string();

        let mut frames = Vec::new();

        // Get every frame
        for (timestamp, frame) in file_json.entries() {
            // Debug the frame
            println!("{:?}", frame);

            // Skip the song name
            if timestamp == "song" {
                continue;
            }

            let timestamp = timestamp.parse().unwrap();

            // Get all of the lights of this frame
            let lights: Vec<Option<bool>> = (0..MAX_LIGHTS)
                .into_iter()
                .map(|i| {
                    let light_name = format!("light-{}", i);
                    if frame[&light_name].is_null() {
                        None
                    } else {
                        Some(frame[&light_name].as_f32().unwrap() > 0.0)
                    }
                })
                .collect();

            // Get all the lasers of this frame
            let lasers: Vec<Option<Laser>> = (0..MAX_PROJECTORS)
                .into_iter()
                .map(|i| {
                    let laser_name = format!("laser-{}", i);
                    let laser_config_name = format!("laser-{}-config", i);
                    if frame[&laser_name].is_null() {
                        None
                    } else {
                        let laser = frame[&laser_name].to_owned();

                        // If the laser is set to zero, reset it
                        if laser.is_number() {
                            return Some(Laser {
                                home: true,
                                speed_profile: false,
                                // TODO: This shouldn't be just a single frame
                                data_frame: vec![LaserDataFrame {
                                    x_pos: 0,
                                    y_pos: 0,
                                    r: 0,
                                    g: 0,
                                    b: 0,
                                }],
                            });
                        }

                        let laser_config = &frame[&laser_config_name];

                        // Laser config data
                        let home = laser_config["home"].as_bool().unwrap();
                        let speed_profile = laser_config["speed-profile"].as_bool().unwrap();

                        // Laser data
                        let laser_frames: Vec<LaserDataFrame> = laser
                            .members()
                            .map(|frame| {
                                let frame = frame.to_owned();
                                let arr = frame
                                    .members()
                                    .map(|x| x.as_u16().unwrap())
                                    .collect::<Vec<u16>>();

                                let x_pos = arr[0];
                                let y_pos = arr[1];
                                let r = arr[2] as u8;
                                let g = arr[3] as u8;
                                let b = arr[4] as u8;

                                LaserDataFrame {
                                    x_pos,
                                    y_pos,
                                    r,
                                    g,
                                    b,
                                }
                            })
                            .collect();

                        Some(Laser {
                            home,
                            speed_profile,
                            data_frame: laser_frames,
                        })
                    }
                })
                .collect();

            frames.push(Frame {
                timestamp,
                lights,
                lasers,
            });
        }

        // Sort frames by timestamp
        frames.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

        ShowManager {
            current_show: Some(Show { song, frames }),
            message_queue: Some(message_queue),
            start_time: None,
        }
    }

    pub fn save_show(&self, show: Show)-> String {
        let mut file_json = json::JsonValue::new_object();

        file_json["song"] = show.song.into();

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
                        json::JsonValue::Boolean(laser.speed_profile);

                    file_json[&timestamp][&laser_name] = json::JsonValue::new_array();

                    for laser_frame in &laser.data_frame {
                        file_json[&timestamp][&laser_name].push(json::JsonValue::new_array());
                        let last_index = file_json[&timestamp][&laser_name].len() - 1;
                        file_json[&timestamp][&laser_name][last_index].push(laser_frame.x_pos);
                        file_json[&timestamp][&laser_name][last_index].push(laser_frame.y_pos);
                        file_json[&timestamp][&laser_name][last_index].push(laser_frame.r);
                        file_json[&timestamp][&laser_name][last_index].push(laser_frame.g);
                        file_json[&timestamp][&laser_name][last_index].push(laser_frame.b);
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
        let show_file_contents = std::fs::read_to_string(show_file_path).unwrap();
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
        let handle = tokio::spawn(async move {
            loop {
                // Get the next frame
                let curr_frame = self.current_show.as_mut().unwrap().frames.remove(0);

                // Sleep until the current frame is ready
                let curr_time = self.start_time.unwrap().elapsed().as_millis() as u64;
                let sleep_time = max(curr_frame.timestamp - curr_time, 0);
                sleep(Duration::from_millis(sleep_time)).await;

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
                // ...

                if self.current_show.as_ref().unwrap().frames.len() == 0 {
                    break;
                }
            }
        });

        handle.await.unwrap();
    }
}
