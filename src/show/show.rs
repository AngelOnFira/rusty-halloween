use kira::sound::static_sound::StaticSoundData;

use crate::audio::Audio;

use super::{LaserDataFrame, MAX_LIGHTS, MAX_PROJECTORS};

/// A show contains a song and a list of frames. The song won't be loaded in
/// until it is the next one up. This is to save memory. A show should be
/// clonable from the show dictionary with ease.
#[derive(Clone, Debug)]
pub enum Show {}

#[derive(Clone, Debug)]
pub struct LoadedShow {
    pub song: Song,
    pub frames: Vec<Frame>,
}

/// Turn an unloaded show into a loaded show. This will be async because it
/// needs to load the song from disk.
impl UnloadedShow {
    pub async fn load_show(self) -> LoadedShow {
        // Load the song
        let song = Song {
            name: "test".to_string(),
            stream: None,
        };
                let song = Audio::get_sound(&song_file_name).unwrap();


        LoadedShow {
            song,
            frames: self.frames,
        }
    }
}

#[derive(Clone, Debug)]
pub struct UnloadedShow {
    pub frames: Vec<Frame>,
}

// #[derive(Clone, Debug)]
// pub struct Show {
//     pub song: Option<Song>,
//     pub frames: Vec<Frame>,
// }

#[derive(Clone, Debug)]
pub struct Song {
    pub name: String,
    pub stream: StaticSoundData,
}

/// A frame consists of a timestamp since the beginning of this show, a list of
/// commands for the lights, and a list of commands for the lasers.
#[derive(Clone, Debug)]
pub struct Frame {
    pub timestamp: u64,
    pub lights: Vec<Option<bool>>,
    pub lasers: Vec<Option<Laser>>,
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
    pub fn load_show_file(show_file_contents: String) -> Self {
        // Load as json
        let file_json = json::parse(&show_file_contents).unwrap();

        let mut frames = Vec::new();

        // Get every frame
        for (timestamp, frame) in file_json.entries() {
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

                    if frame[&laser_name].is_null() {
                        None
                    } else {
                        let laser = frame[&laser_name].to_owned();

                        // If the laser is set to zero, reset it
                        if laser.is_number() {
                            return Some(Laser {
                                home: false,
                                speed_profile: 0,
                                enable: true,
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

                        let laser_config = &laser["config"];

                        // Laser config data
                        let home = laser_config["home"].as_bool().unwrap_or(false);
                        let speed_profile = laser_config["speed-profile"].as_u8().unwrap_or(0);

                        // Laser data
                        let laser_frames: Vec<LaserDataFrame> = laser["points"]
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
                            enable: true,
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

        UnloadedShow { frames }
    }

    // Show frame patterns
    pub fn row_flashing() -> Vec<Frame> {
        (0..1_000)
            .into_iter()
            .map(|i| {
                let frame = Frame {
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
                    lasers: (0..MAX_PROJECTORS).into_iter().map(|_| None).collect(),
                };
                frame
            })
            .collect::<Vec<Frame>>()
    }
}
