use anyhow::Error;
use pi_pinout::{GpioPin, PhysicalPin, WiringPiPin};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct Config {
    pub lights: Vec<Light>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct Light {
    pub pin: Pin,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
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
            pin: Physical(PhysicalPin(2)),
        ),
        Light(
            pin: Gpio(GpioPin(5)),
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
                        pin: Pin::Physical(pi_pinout::PhysicalPin(2))
                    },
                    Light {
                        pin: Pin::Gpio(pi_pinout::GpioPin(5))
                    },
                ]
            }
        );
    }
}
