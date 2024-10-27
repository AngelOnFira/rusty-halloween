use anyhow::Error;
use pi_pinout::{GpioPin, PhysicalPin, WiringPiPin};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::show::prelude::{DmxStateIndex, DmxStateVarPosition};

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, Clone)]
pub struct Config {
    pub lights: Vec<Light>,
    pub lasers: Vec<Laser>,
    pub projectors: Vec<Projector>,
    pub turrets: Vec<Turret>,
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

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, Clone)]
pub struct Projector {
    pub id: u8,
    pub format: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, Clone)]
pub struct Turret {
    pub id: u8,
    pub format: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, Clone)]
pub enum Pin {
    Physical(PhysicalPin),
    Gpio(GpioPin),
    WiringPi(WiringPiPin),
}

impl Config {
    pub fn load() -> Result<Config, Error> {
        let config = std::fs::read_to_string("config.ron")?;
        let config: Config = ron::from_str(&config)?;
        Ok(config)
    }

    pub fn load_from_json(path: &str) -> Result<Config, Error> {
        let json_str = std::fs::read_to_string(path)?;
        let json: Value = serde_json::from_str(&json_str)?;

        let mut lights = Vec::new();
        let mut lasers = Vec::new();
        let mut projectors = Vec::new();
        let mut turrets = Vec::new();

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
                    let format = value["format"]
                        .as_array()
                        .unwrap_or(&Vec::new())
                        .iter()
                        .map(|v| v.as_str().unwrap_or("").to_string())
                        .collect();

                    if key.starts_with("projector-") {
                        projectors.push(Projector {
                            id: value["id"].as_u64().unwrap_or(0) as u8,
                            format,
                        });
                    } else if key.starts_with("turret-") {
                        turrets.push(Turret {
                            id: value["id"].as_u64().unwrap_or(0) as u8,
                            format,
                        });
                    }
                }
                _ => continue,
            }
        }

        // Sort all vectors by ID for consistency
        lights.sort_by_key(|l| l.id);
        lasers.sort_by_key(|l| l.id);
        projectors.sort_by_key(|p| p.id);
        turrets.sort_by_key(|t| t.id);

        Ok(Config {
            lights,
            lasers,
            projectors,
            turrets,
        })
    }

    pub fn get_dmx_state_var_position(&self, device_name: &str, var_name: &str) -> DmxStateIndex {
        // Look through either projectors or turrets
        if let Some(project_num) = device_name.strip_prefix("projector-") {
            let id = project_num.parse::<u8>().unwrap();

            // Find the index of the var_name in the format
            let index = self.projectors[id as usize]
                .format
                .iter()
                .position(|v| v == var_name)
                .unwrap() as u8;
            return id + index;
        } else if let Some(turret_num) = device_name.strip_prefix("turret-") {
            let id = turret_num.parse::<u8>().unwrap();

            // Find the index of the var_name in the format
            let index = self.turrets[id as usize]
                .format
                .iter()
                .position(|v| v == var_name)
                .unwrap() as u8;
            return id + index;
        } else {
            // If it wasn't a projector or turret then throw an error
            panic!("Invalid device name: {}", device_name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load() {
        // Write an example config file
        std::fs::write(
            "config.ron",
            r#"(
    lights: [
        Light(
            pin: Physical(PhysicalPin(8)),
            id: 1,
        ),
        Light(
            pin: Physical(PhysicalPin(10)),
            id: 2,
        ),
        Light(
            pin: Physical(PhysicalPin(16)),
            id: 3,
        ),
        Light(
            pin: Physical(PhysicalPin(18)),
            id: 4,
        ),
        Light(
            pin: Physical(PhysicalPin(22)),
            id: 5,
        ),
        Light(
            pin: Physical(PhysicalPin(24)),
            id: 6,
        ),
        Light(
            pin: Physical(PhysicalPin(26)),
            id: 7,
        ),
    ],
    lasers: [
        Laser(
            id: 1,
        ),
        Laser(
            id: 2,
        ),
    ],
    projectors: [
        Projector(
            id: 1,
            format: ["state", "", "gallery"],
        ),
        Projector(
            id: 2,
            format: ["pan", "tilt", "state"],
        ),
    ],
    turrets: [
        Turret(
            id: 1,
            format: ["state", "", "gallery"],
        ),
        Turret(
            id: 2,
            format: ["pan", "tilt", "state"],
        ),
    ],
)"#,
        )
        .unwrap();

        let config = Config::load().unwrap();
        assert_eq!(
            config,
            Config {
                lights: vec![
                    Light {
                        pin: Pin::Physical(pi_pinout::PhysicalPin(8)),
                        id: 1,
                    },
                    Light {
                        pin: Pin::Physical(pi_pinout::PhysicalPin(10)),
                        id: 2,
                    },
                    Light {
                        pin: Pin::Physical(pi_pinout::PhysicalPin(16)),
                        id: 3,
                    },
                    Light {
                        pin: Pin::Physical(pi_pinout::PhysicalPin(18)),
                        id: 4,
                    },
                    Light {
                        pin: Pin::Physical(pi_pinout::PhysicalPin(22)),
                        id: 5,
                    },
                    Light {
                        pin: Pin::Physical(pi_pinout::PhysicalPin(24)),
                        id: 6,
                    },
                    Light {
                        pin: Pin::Physical(pi_pinout::PhysicalPin(26)),
                        id: 7,
                    },
                ],
                lasers: vec![Laser { id: 1 }, Laser { id: 2 },],
                projectors: vec![
                    Projector {
                        id: 1,
                        format: vec!["state".to_string(), "".to_string(), "gallery".to_string()],
                    },
                    Projector {
                        id: 2,
                        format: vec!["pan".to_string(), "tilt".to_string(), "state".to_string(),],
                    },
                ],
                turrets: vec![
                    Turret {
                        id: 1,
                        format: vec!["state".to_string(), "".to_string(), "gallery".to_string(),],
                    },
                    Turret {
                        id: 2,
                        format: vec!["pan".to_string(), "tilt".to_string(), "state".to_string(),],
                    },
                ],
            }
        );
    }

    #[test]
    fn test_load_from_json() {
        let config = Config::load_from_json("src/show/assets/2024/hardware.json").unwrap();

        dbg!(&config);

        assert_eq!(config.lights.len(), 7);
        assert_eq!(config.lasers.len(), 5);
        assert_eq!(config.projectors.len(), 1);
        assert_eq!(config.turrets.len(), 4);

        // Check a few specific items
        assert_eq!(config.lights[0].pin, Pin::Physical(PhysicalPin(28)));
        assert_eq!(config.lights[0].id, 1);
        assert_eq!(config.lasers[0].id, 1);
        assert_eq!(config.projectors[0].id, 1);
        assert_eq!(
            config.projectors[0].format,
            vec![
                "state", "", "gallery", "pattern", "", "", "", "", "", "", "", "colour", "", "",
                "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", ""
            ]
        );
        assert_eq!(config.turrets[0].id, 41);
        assert_eq!(config.turrets[0].format, vec!["pan", "tilt", "state"]);
    }
}
