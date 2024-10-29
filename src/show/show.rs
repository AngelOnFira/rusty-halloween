use std::path::Path;

use log::info;

use crate::{
    audio::Audio,
    config::Config,
    prelude::{LoadedSong, LoadingSong},
};

use super::{LaserDataFrame, MAX_LASERS, MAX_LIGHTS, MAX_PROJECTORS, MAX_TURRETS};

pub type DmxStateData = u8;
pub type DmxStateIndex = u8;
pub type DmxStateVarPosition = (DmxStateIndex, DmxStateData);

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
    pub projectors: Vec<Option<Projector>>,
    pub turrets: Vec<Option<Turret>>,
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
    pub point_count: u8,
    pub speed_profile: u8,
    pub enable: bool,
    // Laser
    pub hex: [u8; 3],
    pub value: u8,
}

#[derive(Clone, Debug)]
pub struct Projector {
    pub state: DmxStateVarPosition,
    pub gallery: DmxStateVarPosition,
    pub pattern: DmxStateVarPosition,
    pub colour: DmxStateVarPosition,
}

#[derive(Clone, Debug)]
pub struct Turret {
    pub state: DmxStateVarPosition,
    pub pan: DmxStateVarPosition,
    pub tilt: DmxStateVarPosition,
}

impl UnloadedShow {
    pub fn load_show_file(show_file_path: &Path, config: &Config) -> Self {
        // The show name is in shows/<show_name>/instructions.json, extract it
        let show_name = show_file_path
            .parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap();

        // // Load the hardware configuration
        // let hardware_config = std::fs::read_to_string("src/show/assets/2024/hardware.json")
        //     .expect("Failed to read hardware config");
        // let hardware: serde_json::Value =
        //     serde_json::from_str(&hardware_config).expect("Failed to parse hardware config");

        // Load the show file
        let show_file = std::fs::read_to_string(show_file_path).unwrap();
        let show_json: serde_json::Value = serde_json::from_str(&show_file).unwrap();

        let mut frames = Vec::new();

        // Process each timestamp frame
        for (timestamp, frame) in show_json.as_object().unwrap() {
            if timestamp == "song" {
                continue;
            }

            let timestamp: u64 = timestamp.parse().unwrap();
            let frame = frame.as_object().unwrap();

            let mut lights = vec![None; MAX_LIGHTS];
            let mut lasers = vec![None; MAX_LASERS];
            let mut projectors = vec![None; MAX_PROJECTORS];
            let mut turrets = vec![None; MAX_TURRETS];
            // let mut dmx_states = Vec::new();

            // Process each device in the frame
            for (device_name, device_state) in frame {
                dbg!(&device_name);
                dbg!(&device_state);
                if let Some(light_num) = device_name.strip_prefix("light-") {
                    if let Ok(index) = light_num.parse::<usize>() {
                        if index <= MAX_LIGHTS {
                            let value = device_state.as_f64().unwrap_or(0.0) > 0.0;
                            lights[index - 1] = Some(value);
                        }
                    }
                } else if let Some(laser_num) = device_name.strip_prefix("laser-") {
                    if let Ok(index) = laser_num.parse::<usize>() {
                        if index <= MAX_LASERS {
                            let laser = if device_state.is_number() {
                                // Reset command
                                Some(Laser {
                                    home: false,
                                    point_count: 0,
                                    speed_profile: 0,
                                    enable: true,
                                    hex: [0, 0, 0],
                                    value: 0,
                                })
                            } else {
                                // Full laser configuration
                                let config = device_state.get("config");
                                let points = device_state.get("points");

                                match config {
                                    Some(config) => {
                                        let home = config
                                            .get("home")
                                            .and_then(|v| v.as_bool())
                                            .unwrap_or(false);

                                        let speed_profile = config
                                            .get("speed-profile")
                                            .and_then(|v| v.as_u64())
                                            .unwrap_or(0)
                                            as u8;

                                        let point_count =
                                            points.unwrap().as_array().unwrap().len() as u8;

                                        dbg!(device_state.get("hex"));

                                        let hex = device_state
                                            .get("hex")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("000")
                                            .chars()
                                            .map(|c| {
                                                u8::from_str_radix(&c.to_string(), 16).unwrap()
                                            })
                                            .collect::<Vec<u8>>()
                                            .try_into()
                                            .unwrap();

                                        let value_lookup = [
                                            "bat",
                                            "bow",
                                            "bow_slow",
                                            "candy",
                                            "circle",
                                            "circle_slow",
                                            "clockwise_spiral_slow",
                                            "counterclockwise_spiral_slow",
                                            "crescent",
                                            "ghost",
                                            "gravestone_cross",
                                            "hexagon",
                                            "hexagon_slow",
                                            "horizontal_lines_left_to_right_slow",
                                            "horizontal_lines_right_to_left_slow",
                                            "lightning_bolt",
                                            "octagon",
                                            "octagon_slow",
                                            "parallelogram",
                                            "parallelogram_slow",
                                            "pentagon",
                                            "pentagon_slow",
                                            "pentagram",
                                            "pentagram_slow",
                                            "pumpkin",
                                            "septagon_slow",
                                            "square_large",
                                            "square_large_slow",
                                            "square_small",
                                            "square_small_slow",
                                            "star",
                                            "star_slow",
                                            "triangle_large",
                                            "triangle_large_slow",
                                            "triangle_small",
                                            "triangle_small_slow",
                                            "vertical_lines_bottom_to_top_slow",
                                            "vertical_lines_top_to_bottom_slow",
                                        ];

                                        let value_config = device_state.get("value").unwrap();
                                        let value = value_lookup
                                            .iter()
                                            .position(|&v| v == value_config.as_str().unwrap())
                                            .unwrap_or(0)
                                            as u8;

                                        Some(Laser {
                                            home,
                                            point_count,
                                            speed_profile,
                                            enable: true,
                                            hex,
                                            value,
                                        })
                                    }
                                    _ => None,
                                }
                            };
                            lasers[index - 1] = laser;
                        }
                    }
                } else if let Some(projector_num) = device_name.strip_prefix("projector-") {
                    if let Ok(index) = projector_num.parse::<usize>() {
                        if index <= MAX_PROJECTORS {
                            let projector = Projector {
                                state: (
                                    config.get_dmx_state_var_position(device_name, "state"),
                                    device_state["state"].as_u64().unwrap_or(0) as u8,
                                ),
                                gallery: (
                                    config.get_dmx_state_var_position(device_name, "gallery"),
                                    device_state["gallery"].as_u64().unwrap_or(0) as u8,
                                ),
                                pattern: (
                                    config.get_dmx_state_var_position(device_name, "pattern"),
                                    device_state["pattern"].as_u64().unwrap_or(0) as u8,
                                ),
                                colour: (
                                    config.get_dmx_state_var_position(device_name, "colour"),
                                    device_state["colour"].as_u64().unwrap_or(0) as u8,
                                ),
                            };
                            projectors[index - 1] = Some(projector);
                        }
                    }
                } else if let Some(turret_num) = device_name.strip_prefix("turret-") {
                    if let Ok(index) = turret_num.parse::<usize>() {
                        if index <= MAX_TURRETS {
                            let turret = Turret {
                                state: (
                                    config.get_dmx_state_var_position(device_name, "state"),
                                    device_state["state"].as_u64().unwrap_or(0) as u8,
                                ),
                                pan: (
                                    config.get_dmx_state_var_position(device_name, "pan"),
                                    device_state["pan"].as_u64().unwrap_or(0) as u8,
                                ),
                                tilt: (
                                    config.get_dmx_state_var_position(device_name, "tilt"),
                                    device_state["tilt"].as_u64().unwrap_or(0) as u8,
                                ),
                            };
                            turrets[index - 1] = Some(turret);
                        }
                    }
                } else {
                    // // Assume any other device is DMX
                    // let device_config = &hardware[device_name];
                    // let id = device_config["id"].as_u64().unwrap();
                    // let format = device_config["format"].as_array().unwrap();

                    // let mut values = vec![0u8; format.len()];

                    // for (i, channel_type) in format.iter().enumerate() {
                    //     if let Some(channel_name) = channel_type.as_str() {
                    //         if !channel_name.is_empty() {
                    //             if let Some(value) = device_state.get(channel_name) {
                    //                 values[i] = value.as_u64().unwrap_or(0) as u8;
                    //             }
                    //         }
                    //     }
                    // }

                    // dmx_states.push(DmxState {
                    //     device_name: device_name.to_string(),
                    //     channel_id: id,
                    //     values,
                    // });
                    panic!("Unknown device: {}", device_name);
                }
            }

            frames.push(Frame {
                timestamp,
                lights,
                lasers,
                projectors,
                turrets,
            });
        }

        // Sort frames by timestamp
        frames.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

        dbg!(&frames);

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
                lasers: (0..MAX_LASERS).map(|_| None).collect(),
                projectors: (0..MAX_PROJECTORS).map(|_| None).collect(),
                turrets: (0..MAX_TURRETS).map(|_| None).collect(),
            })
            .collect::<Vec<Frame>>()
    }
}

// Add a default implementation for LaserDataFrame
impl Default for LaserDataFrame {
    fn default() -> Self {
        Self {
            pattern_id: 0,
            r: 0,
            g: 0,
            b: 0,
        }
    }
}
