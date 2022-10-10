use anyhow::Error;
use log::info;
use rppal::gpio::{Gpio, OutputPin};

use crate::config::{Config, Pin};

pub struct Lights {
    pins: Vec<OutputPin>,
}

impl Lights {
    pub fn init(config: &Config) -> Result<Self, Error> {
        let mut pins = Vec::new();

        for (i, light) in config.lights.iter().enumerate() {
            // Turn this pin into a physical pin
            let pin = match light.pin {
                Pin::Physical(pin) => pin,
                Pin::Gpio(pin) => pin.into(),
                Pin::WiringPi(pin) => pin.into(),
            };

            info!("Light {}: initializing on pin {}", i, pin.0);

            let pin = Gpio::new()?.get(pin.0).unwrap().into_output();
            pins.push(pin);
        }

        Ok(Self { pins })
    }

    pub fn set_pin(&mut self, pin: u8, value: bool) {
        match value {
            true => self.pins[pin as usize].set_high(),
            false => self.pins[pin as usize].set_low(),
        }
    }
}
