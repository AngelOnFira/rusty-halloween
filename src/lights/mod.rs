use anyhow::Error;
use log::{info, error};
// use rillrate::prime::{Switch, SwitchOpts};
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
    // switches: Vec<Switch>,
}

#[cfg(not(feature = "pi"))]
#[allow(dead_code)]
pub struct LightController {
    pins: Vec<()>,
    // switches: Vec<Switch>,
}

impl LightController {
    pub async fn init(
        config: &Config,
        _message_queue: mpsc::Sender<MessageKind>,
    ) -> Result<Self, Error> {
        #[allow(unused_mut)]
        let mut pins = Vec::new();
        // let mut switches = Vec::new();

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

            // // Set up a dashboard button to enable this light
            // let switch = Switch::new(
            //     format!("app.dashboard.Lights.Light-{} (pin {})", i, pin.0),
            //     SwitchOpts::default().label("Click Me!"),
            // );
            // let this = switch.clone();

            // let message_queue_clone = message_queue.clone();
            // switch.sync_callback(move |envelope| {
            //     if let Some(action) = envelope.action {
            //         let light_message = InternalMessage::Light {
            //             light_id: i as u8,
            //             enable: action,
            //         };

            //         message_queue_clone
            //             .blocking_send(MessageKind::InternalMessage(light_message))
            //             .unwrap();

            //         this.apply(action);
            //     }
            //     Ok(())
            // });

            // switches.push(switch);
        }

        Ok(Self { pins })
    }

    #[allow(dead_code, unused_variables)]
    pub fn set_pin(&mut self, pin: u8, value: bool) {
        let pin = pin - 1;
        info!("Light {}: setting to {}. Len of pins: {}", pin, value, self.pins.len());
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

        // // Change the switch on the dashboard
        // self.switches[pin as usize - 1].apply(value);
    }
}
