use anyhow::Error;
use pi_pinout::{GpioPin, PhysicalPin, WiringPiPin};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Config {
    pub lights: Vec<Light>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Light {
    pub pin: Pin,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
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
        ),
        Light(
            pin: Physical(PhysicalPin(10)),
        ),
        Light(
            pin: Physical(PhysicalPin(16)),
        ),
        Light(
            pin: Physical(PhysicalPin(18)),
        ),
        Light(
            pin: Physical(PhysicalPin(22)),
        ),
        Light(
            pin: Physical(PhysicalPin(24)),
        ),
        Light(
            pin: Physical(PhysicalPin(26)),
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
                        pin: Pin::Physical(pi_pinout::PhysicalPin(8))
                    },
                    Light {
                        pin: Pin::Physical(pi_pinout::PhysicalPin(10))
                    },
                    Light {
                        pin: Pin::Physical(pi_pinout::PhysicalPin(16))
                    },
                    Light {
                        pin: Pin::Physical(pi_pinout::PhysicalPin(18))
                    },
                    Light {
                        pin: Pin::Physical(pi_pinout::PhysicalPin(22))
                    },
                    Light {
                        pin: Pin::Physical(pi_pinout::PhysicalPin(24))
                    },
                    Light {
                        pin: Pin::Physical(pi_pinout::PhysicalPin(26))
                    },
                ]
            }
        );
    }
}
