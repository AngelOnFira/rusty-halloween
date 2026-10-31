use std::collections::BTreeMap;

use anyhow::Error;
use log::warn;
use pi_pinout::{GpioPin, PhysicalPin, WiringPiPin};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::show::prelude::DmxStateIndex;

/// Highest addressable DMX channel. Channels are numbered 1-512
pub const MAX_DMX_CHANNEL: usize = 512;

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, Clone)]
pub struct Config {
    pub lights: Vec<Light>,
    pub lasers: Vec<Laser>,
    /// Every DMX device, keyed by its name in hardware.json (e.g. "lp-1")
    pub dmx_devices: BTreeMap<String, DmxDevice>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, Clone)]
pub struct Light {
    pub pin: Pin,
    pub id: u8,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, Clone)]
pub struct Laser {
    pub id: u8,
}

/// A DMX device. `id` is the device's 1-based start address, and each entry
/// in `format` is the keyword for the channel at `id + index`. Empty strings
/// are unused channels.
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, Clone)]
pub struct DmxDevice {
    pub id: DmxStateIndex,
    pub format: Vec<String>,
}

impl DmxDevice {
    /// Get the DMX channel for a keyword on this device
    pub fn channel(&self, var_name: &str) -> Option<DmxStateIndex> {
        // Empty strings mark unused channels, so they can't be looked up
        if var_name.is_empty() {
            return None;
        }

        // Find the index of the var_name in the format, and offset it from
        // the DMX address for this hardware device
        let var_name_index = self.format.iter().position(|v| v == var_name)?;
        Some(self.id + var_name_index as DmxStateIndex)
    }

    /// The last DMX channel this device occupies
    pub fn last_channel(&self) -> DmxStateIndex {
        self.id + self.format.len() as DmxStateIndex - 1
    }

    /// All the named (non-empty) channels of this device
    pub fn named_channels(&self) -> impl Iterator<Item = (&str, DmxStateIndex)> {
        self.format
            .iter()
            .enumerate()
            .filter(|(_, name)| !name.is_empty())
            .map(|(index, name)| (name.as_str(), self.id + index as DmxStateIndex))
    }
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, Clone)]
pub enum Pin {
    Physical(PhysicalPin),
    Gpio(GpioPin),
    WiringPi(WiringPiPin),
}

impl Config {
    pub fn load_from_json(path: &str) -> Result<Config, Error> {
        let json_str = std::fs::read_to_string(path)?;
        let json: Value = serde_json::from_str(&json_str)?;

        let mut lights = Vec::new();
        let mut lasers = Vec::new();
        let mut dmx_devices = BTreeMap::new();

        // Process all entries in the JSON
        for (key, value) in json.as_object().ok_or_else(|| Error::msg("Invalid JSON"))? {
            match value["protocol"].as_str() {
                Some("GPIO") => {
                    if key.starts_with("light-") {
                        lights.push(Light {
                            pin: Pin::Physical(PhysicalPin(
                                value["pin"].as_u64().unwrap_or(0) as u8
                            )),
                            id: value["id"].as_u64().unwrap_or(0) as u8,
                        });
                    }
                }
                Some("SERIAL") => {
                    if key.starts_with("laser-") {
                        lasers.push(Laser {
                            id: value["id"].as_u64().unwrap_or(0) as u8,
                        });
                    }
                }
                Some("DMX") => {
                    let format: Vec<String> = value["format"]
                        .as_array()
                        .unwrap_or(&Vec::new())
                        .iter()
                        .map(|v| v.as_str().unwrap_or("").to_string())
                        .collect();

                    // Make sure the device fits inside the universe
                    let id = value["id"].as_u64().unwrap_or(0) as usize;
                    if id < 1 || format.is_empty() || id + format.len() - 1 > MAX_DMX_CHANNEL {
                        warn!(
                            "DMX device {}: start address {} with {} channels does not fit in 1-{}, skipping",
                            key,
                            id,
                            format.len(),
                            MAX_DMX_CHANNEL
                        );
                        continue;
                    }

                    dmx_devices.insert(
                        key.clone(),
                        DmxDevice {
                            id: id as DmxStateIndex,
                            format,
                        },
                    );
                }
                _ => continue,
            }
        }

        // Sort all vectors by ID for consistency
        lights.sort_by_key(|l| l.id);
        lasers.sort_by_key(|l| l.id);

        Ok(Config {
            lights,
            lasers,
            dmx_devices,
        })
    }

    /// Get the DMX channel for a keyword on a device. Returns None if the
    /// device or keyword is not in the hardware config
    pub fn get_dmx_state_var_position(
        &self,
        device_name: &str,
        var_name: &str,
    ) -> Option<DmxStateIndex> {
        self.dmx_devices.get(device_name)?.channel(var_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_from_json() {
        let config = Config::load_from_json("src/show/assets/2024/hardware.json").unwrap();

        dbg!(&config);

        assert_eq!(config.lights.len(), 7);
        assert_eq!(config.lasers.len(), 5);
        assert_eq!(config.dmx_devices.len(), 5);

        // Check a few specific items
        assert_eq!(config.lights[0].pin, Pin::Physical(PhysicalPin(28)));
        assert_eq!(config.lights[0].id, 1);
        assert_eq!(config.lasers[0].id, 1);
        assert_eq!(config.dmx_devices["lp-1"].id, 1);
        assert_eq!(
            config.dmx_devices["lp-1"].format,
            vec![
                "state", "", "gallery", "pattern", "", "", "", "", "", "", "", "colour", "", "",
                "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", ""
            ]
        );
        assert_eq!(config.dmx_devices["turret-1"].id, 41);
        assert_eq!(
            config.dmx_devices["turret-1"].format,
            vec!["pan", "tilt", "state"]
        );
    }

    #[test]
    fn test_load_2026_dmx_patch() {
        let config = Config::load_from_json("config/hardware.json").unwrap();

        // The GPIO lights moved to DMX this year
        assert!(config.lights.is_empty());

        let channel = |device: &str, var: &str| config.get_dmx_state_var_position(device, var);

        // Legacy aliases land on the new fixtures
        assert_eq!(channel("light-1", "state"), Some(1));
        assert_eq!(channel("light-7", "state"), Some(7));
        assert_eq!(channel("turret-1", "pan"), Some(97));
        assert_eq!(channel("turret-3", "pan"), Some(103));
        assert_eq!(channel("turret-4", "state"), Some(108));
        assert_eq!(channel("lp-1", "state"), Some(256));
        assert_eq!(channel("lp-1", "size"), Some(257));
        assert_eq!(channel("lp-1", "gallery"), Some(258));
        assert_eq!(channel("lp-1", "pattern"), Some(259));
        assert_eq!(channel("lp-1", "colour"), Some(267));

        // Lasers 1-4 are the pumpkins on RGB-1's odd segments (1, 3, 5, 7).
        // Laser 5 is still a serial laser
        assert_eq!(channel("laser-1", "red"), Some(17));
        assert_eq!(channel("laser-2", "red"), Some(23));
        assert_eq!(channel("laser-3", "red"), Some(29));
        assert_eq!(channel("laser-4", "blue"), Some(37));
        assert_eq!(channel("laser-5", "red"), None);
        assert_eq!(config.lasers.len(), 1);

        // New fixtures, checked at the start and end of their footprints
        assert_eq!(channel("pwm-1", "out-1"), Some(1));
        assert_eq!(channel("pwm-1", "out-8"), Some(8));
        assert_eq!(channel("rgb-1", "seg-1-r"), Some(17));
        assert_eq!(channel("rgb-1", "seg-8-b"), Some(40));
        assert_eq!(channel("rgb-2", "seg-1-r"), Some(49));
        assert_eq!(channel("rgb-2", "seg-8-b"), Some(72));
        assert_eq!(channel("fog-1", "machine-1"), Some(81));
        assert_eq!(channel("fog-1", "enable"), Some(87));
        assert_eq!(channel("bird-1", "mode"), Some(113));
        assert_eq!(channel("bird-1", "volume"), Some(125));
        assert_eq!(channel("spot-1", "pan"), Some(129));
        assert_eq!(channel("spot-1", "control"), Some(146));
        assert_eq!(channel("spot-4", "pan"), Some(183));
        assert_eq!(channel("spot-4", "control"), Some(200));
        assert_eq!(config.dmx_devices["lp-1"].last_channel(), 287);

        // Unknown devices and keywords are not found
        assert_eq!(channel("fog-9", "enable"), None);
        assert_eq!(channel("lp-1", "not-a-keyword"), None);
        assert_eq!(channel("lp-1", ""), None);
    }
}
