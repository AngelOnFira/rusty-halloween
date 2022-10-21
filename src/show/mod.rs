use std::{cmp::max, time::Duration};

use tokio::{
    sync::mpsc,
    time::{sleep, Instant},
};

use crate::{
    MessageKind,
};

pub struct Show {
    pub song: String,
    pub frames: Vec<Frame>,
    pub start_time: Option<Instant>,
    pub message_queue: mpsc::Sender<MessageKind>,
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

const MAX_LIGHTS: usize = 7;
const MAX_PROJECTORS: usize = 5;

impl Show {
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

        Show {
            song,
            frames,
            message_queue,
            start_time: None,
        }
    }

    pub fn load_show_file(
        show_file_path: String,
        message_queue: mpsc::Sender<MessageKind>,
    ) -> Self {
        let show_file_contents = std::fs::read_to_string(show_file_path).unwrap();
        Show::load_show(show_file_contents, message_queue)
    }

    pub async fn start_show(mut self) {
        // Set the timer
        self.start_time = Some(Instant::now());

        // Start the song
        self.message_queue
            .try_send(MessageKind::InternalMessage(
                crate::InternalMessage::Audio {
                    audio_file_contents: self.song.clone(),
                },
            ))
            .unwrap();

        // Start the show thread
        let handle = tokio::spawn(async move {
            // Get the first frame
            let curr_frame = self.frames.remove(0);

            while self.frames.len() > 0 {
                // Sleep until the current frame is ready
                let curr_time = self.start_time.unwrap().elapsed().as_millis() as u64;
                let sleep_time = max(curr_frame.timestamp - curr_time, 0);

                sleep(Duration::from_millis(sleep_time));

                // Execute the current frame

                // Send all the lights data
                for (i, light) in curr_frame.lights.iter().enumerate() {
                    if let Some(light) = light {
                        self.message_queue
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
            }

            // Sleep until the next instruction
        });

        handle.await.unwrap();
    }
}
