use anyhow::Error;
use log::info;
use rillrate::prime::{Switch, SwitchOpts};
use rppal::gpio::{Gpio, OutputPin};
use tokio::sync::mpsc;

use crate::{
    config::{Config, Pin},
    proto_schema::schema::{pico_message::Payload, Light, PicoMessage},
};

#[allow(dead_code)]
pub struct LightController {
    pins: Vec<OutputPin>,
    switches: Vec<Switch>,
}

impl LightController {
    pub async fn init(
        config: &Config,
        message_queue: mpsc::Sender<PicoMessage>,
    ) -> Result<Self, Error> {
        let mut pins = Vec::new();
        let mut switches = Vec::new();

        for (i, light) in config.lights.iter().enumerate() {
            // Turn this pin into a physical pin
            let pin = match light.pin {
                Pin::Physical(pin) => pin,
                Pin::Gpio(pin) => pin.into(),
                Pin::WiringPi(pin) => pin.into(),
            };

            info!("Light {}: initializing on pin {}", i, pin.0);

            // Only initialize GPIO if the Pi feature is enabled
            if cfg!(feature = "pi") {
                let mut pin = Gpio::new()?.get(pin.0).unwrap().into_output();

                // Turn the light off
                // Note; light values are inverted since the physical lights are inverted
                pin.set_high();

                // Add the pin to the list
                pins.push(pin);
            }

            // Set up a dashboard button to enable this light
            let switch = Switch::new(
                format!("app.dashboard.Lights.Light-{} (pin {})", i + 1, pin.0),
                SwitchOpts::default().label("Click Me!"),
            );
            let this = switch.clone();

            let message_queue_clone = message_queue.clone();
            switch.sync_callback(move |envelope| {
                if let Some(action) = envelope.action {
                    let mut light_message = PicoMessage::new();
                    light_message.payload = Some(Payload::Light(Light {
                        light_id: i as i32,
                        // Note; light values are inverted since the physical
                        // lights are inverted
                        enable: !action,
                        ..Default::default()
                    }));

                    message_queue_clone.blocking_send(light_message)?;

                    this.apply(action);
                }
                Ok(())
            });

            switches.push(switch);
        }

        Ok(Self { pins, switches })
    }

    #[allow(dead_code)]
    pub fn set_pin(&mut self, pin: u8, value: bool) {
        // Note; light values are inverted since the physical lights are inverted
        match value {
            true => self.pins[pin as usize].set_low(),
            false => self.pins[pin as usize].set_high(),
        }
    }
}
