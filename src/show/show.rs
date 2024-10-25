use std::path::Path;

use log::info;

use crate::{
    audio::Audio,
    prelude::{LoadedSong, LoadingSong},
};

use super::{LaserDataFrame, MAX_LIGHTS, MAX_PROJECTORS};

/// A show contains a song and a list of frames. The song won't be loaded in
/// until it is the next one up. This is to save memory. A show should be
/// clonable from the show dictionary with ease.
#[derive(Clone, Debug)]
pub enum Show {}

#[derive(Clone, Debug)]
pub struct UnloadedShow {
    pub name: String,
    pub frames: Vec<Frame>,
}

/// Turn an unloaded show into a loaded show. This will be async because it
/// needs to load the song from disk.
impl UnloadedShow {
    pub async fn load_show(self) -> LoadingShow {
        // Load the song
        info!("Name is {}", self.name);
        let song = match Audio::get_sound(&self.name) {
            Ok(song) => song,
            Err(e) => panic!("Error loading song: {}", e),
        };

        LoadingShow {
            song,
            name: self.name,
            frames: self.frames,
        }
    }
}

/// A partly-loaded show is a show that might have a song loaded, but it might
/// not be ready to play yet.
#[derive(Clone, Debug)]
pub struct LoadingShow {
    pub song: LoadingSong,
    pub name: String,
    pub frames: Vec<Frame>,
}

impl LoadingShow {
    pub fn is_ready(&self) -> bool {
        self.song.stream.lock().unwrap().is_some()
    }

    pub fn get_loaded_show(self) -> Result<LoadedShow, ()> {
        // Verify that the song is loaded
        match self.song.stream.lock().unwrap().clone() {
            Some(stream) => Ok(LoadedShow {
                song: LoadedSong {
                    name: self.song.name,
                    stream,
                },
                name: self.name,
                frames: self.frames,
            }),
            None => Err(()),
        }
    }
}

// This is clone because the song is behind an Arc
#[derive(Debug)]
pub struct LoadedShow {
    pub song: LoadedSong,
    pub name: String,
    pub frames: Vec<Frame>,
}

/// A frame consists of a timestamp since the beginning of this show, a list of
/// commands for the lights, and a list of commands for the lasers.
#[derive(Clone, Debug)]
pub struct Frame {
    pub timestamp: u64,
    pub lights: Vec<Option<bool>>,
    pub lasers: Vec<Option<Laser>>,
    pub dmx_states: Vec<DmxState>, // New field for DMX devices
}

#[derive(Clone, Debug)]
pub struct DmxState {
    pub device_name: String,
    pub channel_id: u64,
    pub values: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct Laser {
    // Laser conf
    pub home: bool,
    pub speed_profile: u8,
    pub enable: bool,
    // Laser
    pub data_frame: Vec<LaserDataFrame>,
}

impl UnloadedShow {
    pub fn load_show_file(show_file_path: &Path) -> Self {
        // The show name is in shows/<show_name>/instructions.json, extract it
        let show_name = show_file_path
            .parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap();

        // Load the hardware configuration
        let hardware_config = std::fs::read_to_string("src/show/assets/2024/hardware.json")
            .expect("Failed to read hardware config");
        let hardware: serde_json::Value = serde_json::from_str(&hardware_config)
            .expect("Failed to parse hardware config");

        // Load the show file
        let show_file = std::fs::read_to_string(show_file_path).unwrap();
        let show_json: serde_json::Value = serde_json::from_str(&show_file).unwrap();

        let mut frames = Vec::new();

        // Process each timestamp frame
        for (timestamp, frame) in show_json.as_object().unwrap() {
            if timestamp == "song" { continue; }
            
            let timestamp = timestamp.parse().unwrap();
            let frame = frame.as_object().unwrap();

            let mut lights = vec![None; MAX_LIGHTS];
            let mut lasers = vec![None; MAX_PROJECTORS];
            let mut dmx_states = Vec::new();

            // Process each device in the frame
            for (device_name, device_state) in frame {
                let device_config = &hardware[device_name];
                let protocol = device_config["protocol"].as_str().unwrap();

                match protocol {
                    "GPIO" => {
                        if let Some(light_num) = device_name.strip_prefix("light-") {
                            if let Ok(index) = light_num.parse::<usize>() {
                                if index <= MAX_LIGHTS {
                                    let value = device_state.as_f64().unwrap_or(0.0) > 0.0;
                                    lights[index - 1] = Some(value);
                                }
                            }
                        }
                    },
                    "SERIAL" => {
                        if let Some(laser_num) = device_name.strip_prefix("laser-") {
                            if let Ok(index) = laser_num.parse::<usize>() {
                                if index <= MAX_PROJECTORS {
                                    let laser = if device_state.is_number() {
                                        // Reset command
                                        Some(Laser {
                                            home: false,
                                            speed_profile: 0,
                                            enable: true,
                                            data_frame: vec![LaserDataFrame::default()],
                                        })
                                    } else {
                                        // Full laser configuration
                                        let config = device_state.get("config");
                                        let points = device_state.get("points");

                                        match (config, points) {
                                            (Some(config), Some(points)) => {
                                                let home = config.get("home").and_then(|v| v.as_bool()).unwrap_or(false);
                                                let speed_profile = config.get("speed-profile").and_then(|v| v.as_u64()).unwrap_or(0) as u8;
                                                
                                                let laser_frames = points.as_array().unwrap().iter()
                                                    .map(|point| {
                                                        let coords = point.as_array().unwrap();
                                                        LaserDataFrame {
                                                            x_pos: coords[0].as_u64().unwrap() as u16,
                                                            y_pos: coords[1].as_u64().unwrap() as u16,
                                                            r: coords[2].as_u64().unwrap() as u8,
                                                            g: coords[3].as_u64().unwrap() as u8,
                                                            b: coords[4].as_u64().unwrap() as u8,
                                                        }
                                                    })
                                                    .collect();

                                                Some(Laser {
                                                    home,
                                                    speed_profile,
                                                    enable: true,
                                                    data_frame: laser_frames,
                                                })
                                            },
                                            _ => None,
                                        }
                                    };
                                    lasers[index - 1] = laser;
                                }
                            }
                        }
                    },
                    "DMX" => {
                        let id = device_config["id"].as_u64().unwrap();
                        let format = device_config["format"].as_array().unwrap();
                        
                        // Convert the device state into DMX values based on the format
                        let mut values = vec![0u8; format.len()];
                        
                        for (i, channel_type) in format.iter().enumerate() {
                            if let Some(channel_name) = channel_type.as_str() {
                                if !channel_name.is_empty() {
                                    if let Some(value) = device_state.get(channel_name) {
                                        values[i] = value.as_u64().unwrap_or(0) as u8;
                                    }
                                }
                            }
                        }

                        dmx_states.push(DmxState {
                            device_name: device_name.to_string(),
                            channel_id: id,
                            values,
                        });
                    },
                    _ => {
                        info!("Unknown protocol {} for device {}", protocol, device_name);
                    }
                }
            }

            frames.push(Frame {
                timestamp,
                lights,
                lasers,
                dmx_states,
            });
        }

        // Sort frames by timestamp
        frames.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

        UnloadedShow {
            name: show_name.to_string(),
            frames,
        }
    }

    // Update row_flashing to include empty DMX states
    pub fn row_flashing() -> Vec<Frame> {
        (0..1_000)
            .map(|i| Frame {
                timestamp: i * (60.0 / 166.0 * 1000.0) as u64,
                lights: (0..MAX_LIGHTS)
                    .map(|light| {
                        if i as usize % MAX_LIGHTS == light {
                            Some(true)
                        } else {
                            Some(false)
                        }
                    })
                    .collect(),
                lasers: (0..MAX_PROJECTORS).map(|_| None).collect(),
                dmx_states: Vec::new(), // Empty DMX states for test pattern
            })
            .collect::<Vec<Frame>>()
    }
}

// Add a default implementation for LaserDataFrame
impl Default for LaserDataFrame {
    fn default() -> Self {
        Self {
            x_pos: 0,
            y_pos: 0,
            r: 0,
            g: 0,
            b: 0,
        }
    }
}
