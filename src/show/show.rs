use std::path::Path;

use log::{info, warn};

use crate::{
    audio::Audio,
    config::{Config, DmxDevice},
    prelude::{LoadedSong, LoadingSong},
};

use super::{LaserDataFrame, MAX_LASERS, MAX_LIGHTS};

// Re-export types from common
pub use common::{
    DmxStateData, DmxStateIndex, DmxStateVarPosition, Frame, Laser, SerializableShow,
};

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

    /// Convert to a serializable show that can be sent over the network
    pub fn to_serializable(&self) -> SerializableShow {
        SerializableShow {
            name: self.name.clone(),
            frames: self.frames.clone(),
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

    /// Convert to a serializable show that can be sent over the network
    pub fn to_serializable(&self) -> SerializableShow {
        SerializableShow {
            name: self.name.clone(),
            frames: self.frames.clone(),
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

impl LoadedShow {
    /// Convert to a serializable show that can be sent over the network
    pub fn to_serializable(&self) -> SerializableShow {
        SerializableShow {
            name: self.name.clone(),
            frames: self.frames.clone(),
        }
    }
}

// Frame and Laser types are now imported from common

#[derive(Clone, Debug)]
pub struct DmxState {
    pub device_name: String,
    pub channel_id: u64,
    pub values: Vec<u8>,
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
            let mut dmx = Vec::new();

            // Process each device in the frame
            for (device_name, device_state) in frame {
                if let Some(light_num) = device_name.strip_prefix("light-") {
                    let value = device_state.as_f64().unwrap_or(0.0) > 0.0;

                    // Lights patched over DMX are switched fully on or off,
                    // otherwise they're GPIO lights
                    if let Some(channel) = config.get_dmx_state_var_position(device_name, "state")
                    {
                        dmx.push((channel, if value { DmxStateData::MAX } else { 0 }));
                    } else if let Ok(index) = light_num.parse::<usize>() {
                        if (1..=MAX_LIGHTS).contains(&index) {
                            lights[index - 1] = Some(value);
                        }
                    }
                } else if let Some(laser_num) = device_name.strip_prefix("laser-") {
                    if let Some(device) = config.dmx_devices.get(device_name.as_str()) {
                        // Lasers patched over DMX (the pumpkins) light up in
                        // the laser's colour
                        dmx.extend(parse_laser_as_rgb(device_name, device, device_state));
                    } else if let Ok(index) = laser_num.parse::<usize>() {
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

                                        let hex_str = device_state
                                            .get("hex")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("000");

                                        let hex: [u8; 3] = {
                                            let values = hex_str
                                                .chars()
                                                .map(|c| match c {
                                                    'F' | 'f' => 7,
                                                    '0' => 0,
                                                    _ => panic!("Invalid hex value: {}", c),
                                                })
                                                .collect::<Vec<u8>>();

                                            // Check that exactly one value is 7
                                            if values.iter().filter(|&&x| x == 7).count() != 1 {
                                                panic!("Exactly one color channel must be set to F in hex value: {}", hex_str);
                                            }

                                            values.try_into().unwrap()
                                        };

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
                                            .position(|&v| {
                                                v == value_config
                                                    .as_str()
                                                    .unwrap()
                                                    .replace("-", "_")
                                            })
                                            .unwrap()
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
                } else if let Some(device) = config.dmx_devices.get(device_name.as_str()) {
                    dmx.extend(parse_dmx_device(device_name, device, device_state));
                } else {
                    warn!("Unknown device {} at {}ms, skipping", device_name, timestamp);
                }
            }

            let frame = Frame {
                timestamp,
                lights,
                lasers,
                dmx,
            };

            // dbg!("Frame: {:?}", &frame);

            frames.push(frame);
        }

        // Sort frames by timestamp
        frames.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

        // Drive the fog machines from the existing light and laser cues
        super::fog::add_fog_cues(&mut frames, config);

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
                dmx: Vec::new(),
            })
            .collect::<Vec<Frame>>()
    }
}

/// Turn a DMX device's state in a show frame into channel writes. The state is
/// either an object of format keywords to values, or a single number that is
/// written to every named channel of the device (e.g. `"lp-1": 0` for off).
///
/// A keyword's value is a number for one channel, or an array of numbers that
/// fills consecutive channels starting at that keyword (e.g. an RGB segment).
/// Only the keywords in the frame are written, the rest keep their last value.
fn parse_dmx_device(
    device_name: &str,
    device: &DmxDevice,
    device_state: &serde_json::Value,
) -> Vec<DmxStateVarPosition> {
    let mut positions = Vec::new();

    if device_state.is_number() {
        if let Some(value) = to_dmx_value(device_name, "", device_state) {
            for (_, channel) in device.named_channels() {
                positions.push((channel, value));
            }
        }

        return positions;
    }

    let Some(state) = device_state.as_object() else {
        warn!("DMX device {}: unsupported state {}, skipping", device_name, device_state);
        return positions;
    };

    for (var_name, value) in state {
        let Some(channel) = device.channel(var_name) else {
            warn!("DMX device {}: unknown keyword {}, skipping", device_name, var_name);
            continue;
        };

        match value.as_array() {
            Some(values) => {
                for (offset, value) in values.iter().enumerate() {
                    let channel = channel + offset as DmxStateIndex;
                    if channel > device.last_channel() {
                        warn!(
                            "DMX device {}: {} writes past the device's last channel {}, dropping the rest",
                            device_name,
                            var_name,
                            device.last_channel()
                        );
                        break;
                    }

                    if let Some(value) = to_dmx_value(device_name, var_name, value) {
                        positions.push((channel, value));
                    }
                }
            }
            None => {
                if let Some(value) = to_dmx_value(device_name, var_name, value) {
                    positions.push((channel, value));
                }
            }
        }
    }

    positions
}

/// Convert a show value to a DMX value, clamping it to 0-255 instead of
/// wrapping
fn to_dmx_value(
    device_name: &str,
    var_name: &str,
    value: &serde_json::Value,
) -> Option<DmxStateData> {
    let Some(number) = value.as_f64() else {
        warn!("DMX device {}: {} is not a number ({}), skipping", device_name, var_name, value);
        return None;
    };

    let max = DmxStateData::MAX as f64;
    if !(0.0..=max).contains(&number) {
        warn!("DMX device {}: {} is out of range ({}), clamping", device_name, var_name, number);
    }

    Some(number.round().clamp(0.0, max) as DmxStateData)
}

/// Halloween colours for the RGB devices (the pumpkins) that stand in for the
/// serial lasers. The old laser cues only use pure red, green, or blue.
const LASER_PALETTE: [(&str, [DmxStateData; 3]); 3] = [
    // Red becomes orange
    ("f00", [255, 70, 0]),
    // Green becomes toxic green
    ("0f0", [90, 255, 0]),
    // Blue becomes purple
    ("00f", [150, 0, 255]),
];

/// Turn a laser cue into a colour for the RGB device patched in place of that
/// laser. A number (the laser's off command) turns it off, otherwise the
/// laser's `hex` colour is swapped for its palette colour.
fn parse_laser_as_rgb(
    device_name: &str,
    device: &DmxDevice,
    device_state: &serde_json::Value,
) -> Vec<DmxStateVarPosition> {
    let colour = if device_state.is_number() {
        [0, 0, 0]
    } else {
        let hex = device_state.get("hex").and_then(|v| v.as_str()).unwrap_or("");
        match LASER_PALETTE
            .iter()
            .find(|(laser_hex, _)| laser_hex.eq_ignore_ascii_case(hex))
        {
            Some((_, colour)) => *colour,
            None => {
                warn!("Laser {}: no palette colour for hex {:?}, skipping", device_name, hex);
                return Vec::new();
            }
        }
    };

    ["red", "green", "blue"]
        .into_iter()
        .zip(colour)
        .filter_map(|(var_name, value)| device.channel(var_name).map(|channel| (channel, value)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Write a show file to a temporary directory and load it with the 2026
    /// hardware config
    fn load_test_show(test_name: &str, show_json: &str) -> UnloadedShow {
        let dir = std::env::temp_dir()
            .join(format!("rusty-halloween-{}-{}", test_name, std::process::id()))
            .join("test-show");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("instructions-exported.json");
        std::fs::write(&path, show_json).unwrap();

        let config = Config::load_from_json("config/hardware.json").unwrap();
        let show = UnloadedShow::load_show_file(&path, &config);

        std::fs::remove_dir_all(dir.parent().unwrap()).unwrap();
        show
    }

    /// Channels of the fog controller (fog-1) in the 2026 hardware config
    const FOG_CHANNELS: std::ops::RangeInclusive<DmxStateIndex> = 81..=87;

    fn sorted(mut positions: Vec<DmxStateVarPosition>) -> Vec<DmxStateVarPosition> {
        positions.sort();
        positions
    }

    /// A frame's DMX writes from the show itself, leaving out the generated
    /// fog cues
    fn show_dmx(frame: &Frame) -> Vec<DmxStateVarPosition> {
        sorted(
            frame
                .dmx
                .iter()
                .copied()
                .filter(|(channel, _)| !FOG_CHANNELS.contains(channel))
                .collect(),
        )
    }

    /// The generated fog cues, as (timestamp, writes) for each frame that has any
    fn fog_cues(show: &UnloadedShow) -> Vec<(u64, Vec<DmxStateVarPosition>)> {
        show.frames
            .iter()
            .map(|frame| {
                let fog = frame
                    .dmx
                    .iter()
                    .copied()
                    .filter(|(channel, _)| FOG_CHANNELS.contains(channel))
                    .collect::<Vec<_>>();
                (frame.timestamp, sorted(fog))
            })
            .filter(|(_, fog)| !fog.is_empty())
            .collect()
    }

    #[test]
    fn test_legacy_keys_translate_to_dmx() {
        let show = load_test_show(
            "legacy",
            r#"{
                "song": "test.mp3",
                "0": {
                    "light-2": 1,
                    "turret-1": {"state": 1, "pan": 120, "tilt": 45},
                    "lp-1": {"state": 1, "gallery": 255, "pattern": 4, "colour": 10},
                    "laser-5": 0,
                    "projector-9": {"state": 1}
                },
                "1000": {
                    "light-2": 0,
                    "lp-1": 0
                }
            }"#,
        );

        assert_eq!(show.name, "test-show");
        assert_eq!(show.frames.len(), 2);

        let frame = &show.frames[0];
        assert_eq!(frame.timestamp, 0);

        // Lights are on DMX now, so nothing is left for GPIO
        assert!(frame.lights.iter().all(|light| light.is_none()));

        // Lasers that aren't patched over DMX are parsed the same as before
        assert!(frame.lasers[4].is_some());

        // The laser projector gets last year's values unchanged
        assert_eq!(
            show_dmx(frame),
            vec![
                (2, 255),
                (97, 120),
                (98, 45),
                (99, 1),
                (256, 1),
                (258, 255),
                (259, 4),
                (267, 10),
            ]
        );

        // A bare number turns off every named channel on the laser
        let frame = &show.frames[1];
        assert_eq!(frame.timestamp, 1000);
        let mut expected = vec![(2, 0)];
        expected.extend((256..=287).map(|channel| (channel, 0)));
        assert_eq!(show_dmx(frame), expected);
    }

    #[test]
    fn test_lasers_become_pumpkins() {
        let show = load_test_show(
            "pumpkins",
            r#"{
                "0": {
                    "laser-1": {"config": {}, "points": [[0, 0, 7, 0, 0]], "hex": "f00", "value": "bat"},
                    "laser-2": {"config": {}, "points": [[0, 0, 0, 7, 0]], "hex": "0f0", "value": "bat"},
                    "laser-3": {"config": {}, "points": [[0, 0, 0, 0, 7]], "hex": "00f", "value": "bat"},
                    "laser-4": {"config": {}, "points": [[0, 0, 7, 0, 0]], "hex": "F00", "value": "bat"},
                    "laser-5": {"config": {}, "points": [[0, 0, 0, 7, 0]], "hex": "0f0", "value": "bat"}
                },
                "1000": {
                    "laser-1": 0,
                    "laser-2": {"config": {}, "points": [], "hex": "ff0", "value": "bat"}
                }
            }"#,
        );

        // Lasers 1-4 are pumpkins on RGB-1's odd segments (1, 3, 5, 7), in the
        // Halloween palette. Laser 5 is still a serial laser
        let frame = &show.frames[0];
        assert_eq!(
            show_dmx(frame),
            vec![
                (17, 255),
                (18, 70),
                (19, 0),
                (23, 90),
                (24, 255),
                (25, 0),
                (29, 150),
                (30, 0),
                (31, 255),
                (35, 255),
                (36, 70),
                (37, 0),
            ]
        );
        assert!(frame.lasers[..4].iter().all(|laser| laser.is_none()));
        assert!(frame.lasers[4].is_some());

        // Off turns the pumpkin off, and colours outside the palette are
        // skipped so the pumpkin keeps its last colour
        assert_eq!(
            show_dmx(&show.frames[1]),
            vec![(17, 0), (18, 0), (19, 0)]
        );
    }

    #[test]
    fn test_fog_follows_light_and_laser() {
        let show = load_test_show(
            "fog",
            r#"{
                "0": {"light-4": 1, "lp-1": {"state": 1}},
                "2000": {"light-4": 0, "lp-1": {"state": 0}},
                "3000": {"lp-1": {"state": 1}},
                "6000": {"lp-1": 0},
                "7000": {"lp-1": {"state": 1}},
                "8000": {"lp-1": 0},
                "13000": {"lp-1": {"state": 1}},
                "14000": {"lp-1": 0},
                "21000": {"lp-1": {"state": 1}},
                "22000": {"light-4": 1},
                "24000": {"lp-1": {"state": 255}}
            }"#,
        );

        assert_eq!(
            fog_cues(&show),
            vec![
                // The fog controller is enabled, fog 3 turns on with light-4,
                // and fog 1 fires when the laser turns on
                (0, vec![(81, 255), (83, 255), (87, 255)]),
                // 3000: the laser turns on during fog 1's burst, so it's
                // skipped. Fog 3 is held on for 5s, even though light-4 turned
                // off at 2000
                (5000, vec![(81, 0), (83, 0)]),
                // Fog 2 takes the next turn
                (7000, vec![(82, 255)]),
                (12000, vec![(82, 0)]),
                // 13000: fog 1 is still resting until 20000, so it's skipped
                (21000, vec![(81, 255)]),
                (22000, vec![(83, 255)]),
                // The burst is cut short at the end of the show. 255 turns the
                // laser off, so it doesn't trigger anything
                (24000, vec![(81, 0)]),
            ]
        );
    }

    #[test]
    fn test_fog_holds_on_after_light_flickers() {
        let show = load_test_show(
            "fog-hold",
            r#"{
                "0": {"light-4": 1},
                "500": {"light-4": 0},
                "1000": {"light-4": 1},
                "1500": {"light-4": 0},
                "10000": {"light-4": 1},
                "17000": {"light-4": 0},
                "20000": {"light-4": 1},
                "21000": {"light-4": 0},
                "25000": {"light-4": 1},
                "26000": {"light-4": 0},
                "40000": {"light-4": 1},
                "41000": {"light-4": 0}
            }"#,
        );

        assert_eq!(
            fog_cues(&show),
            vec![
                (0, vec![(83, 255), (87, 255)]),
                // Flickering keeps fog 3 on until 5s after the last time the
                // light turned on
                (6000, vec![(83, 0)]),
                // The light stays on longer than 5s, so fog 3 follows it
                (10000, vec![(83, 255)]),
                (17000, vec![(83, 0)]),
                // The light comes back on right as the hold runs out, so fog 3
                // stays on and holds for another 5s
                (20000, vec![(83, 255)]),
                (30000, vec![(83, 0)]),
                // The hold is cut short at the end of the show
                (40000, vec![(83, 255)]),
                (41000, vec![(83, 0)]),
            ]
        );
    }

    #[test]
    fn test_fog_bursts_never_overlap() {
        // Turn the laser on and off every second for two minutes
        let cues = (0..120)
            .map(|second| format!(r#""{}": {{"lp-1": {{"state": {}}}}}"#, second * 1000, (second + 1) % 2))
            .collect::<Vec<_>>()
            .join(",");
        let show = load_test_show("fog-overlap", &format!("{{{}}}", cues));

        let mut on = [false; 2];
        let mut last_on = [None; 2];
        for (timestamp, writes) in fog_cues(&show) {
            for (channel, value) in writes {
                if let Some(machine) = [81, 82].iter().position(|&c| c == channel) {
                    if value > 0 {
                        // Each machine rests 15s after a 5s burst
                        if let Some(last) = last_on[machine] {
                            assert!(timestamp - last >= 20_000, "fog {} fired too soon", machine + 1);
                        }
                        last_on[machine] = Some(timestamp);
                    }
                    on[machine] = value > 0;
                }
            }
            assert!(!(on[0] && on[1]), "fog 1 and 2 are on together at {}ms", timestamp);
        }
        assert!(last_on.iter().all(|last| last.is_some()));
    }

    #[test]
    fn test_new_fixture_keys() {
        let show = load_test_show(
            "fixtures",
            r#"{
                "0": {
                    "rgb-1": {"seg-1-r": [255, 80, 0]},
                    "rgb-2": {"seg-8-g": [10, 20, 30]},
                    "fog-1": {"enable": 300, "machine-1": -5, "not-a-keyword": 1},
                    "bird-1": {"volume": 127.6}
                }
            }"#,
        );

        assert_eq!(
            sorted(show.frames[0].dmx.clone()),
            vec![
                (17, 255),
                (18, 80),
                (19, 0),
                // The array is cut off at the end of rgb-2's footprint
                (71, 10),
                (72, 20),
                // Clamped instead of wrapping
                (81, 0),
                (87, 255),
                (125, 128),
            ]
        );
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
