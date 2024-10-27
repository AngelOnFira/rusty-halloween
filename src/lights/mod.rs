use anyhow::Error;
use log::{error, info};
use tokio::sync::mpsc;

#[cfg(feature = "pi")]
use rppal::gpio::{Gpio, OutputPin};

use crate::{
    config::{Config, Pin},
    MessageKind,
};

#[allow(dead_code)]
#[cfg(feature = "pi")]
pub struct LightController {
    pins: Vec<OutputPin>,
}

#[cfg(not(feature = "pi"))]
#[allow(dead_code)]
pub struct LightController {
    pins: Vec<()>,
}

impl LightController {
    pub async fn init(
        config: &Config,
        _message_queue: mpsc::Sender<MessageKind>,
    ) -> Result<Self, Error> {
        #[allow(unused_mut)]
        let mut pins = Vec::new();

        for (i, light) in config.lights.iter().enumerate() {
            // Turn this pin into a physical pin
            let pin = match light.pin {
                Pin::Physical(pin) => pin.into(),
                Pin::Gpio(pin) => pin,
                Pin::WiringPi(pin) => pin.into(),
            };

            info!("Light {}: initializing on pin {}", i, pin.0);

            // Only initialize GPIO if the Pi feature is enabled
            #[cfg(feature = "pi")]
            {
                let mut pin = Gpio::new()?.get(pin.0).unwrap().into_output();

                // Turn the light off
                // Note; light values are inverted since the physical lights are inverted
                pin.set_high();

                // Add the pin to the list
                pins.push(pin);
            }
        }

        Ok(Self { pins })
    }

    #[allow(dead_code, unused_variables)]
    pub fn set_pin(&mut self, pin: u8, value: bool) {
        let pin = pin - 1;
        info!(
            "Light {}: setting to {}. Len of pins: {}",
            pin,
            value,
            self.pins.len()
        );
        // Note; light values are inverted since the physical lights are
        // inverted

        // Make sure the pin input is not outside of the range of pins
        if pin as usize > self.pins.len() {
            error!("Light {}: pin {} is out of range", pin, pin);
            return;
        }

        #[cfg(feature = "pi")]
        match value {
            true => self.pins[pin as usize].set_low(),
            false => self.pins[pin as usize].set_high(),
        }
    }
}
