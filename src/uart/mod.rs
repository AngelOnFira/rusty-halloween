use anyhow::Error;
use log::error;
use tokio::sync::mpsc;

#[cfg(feature = "pi")]
use rppal::uart::{Parity, Uart};

pub enum UartMessage {
    Laser(Vec<u8>),
    DMX(Vec<u8>),
}

pub struct UartController {
    #[cfg(feature = "pi")]
    uart: Uart,
}

impl UartController {
    pub async fn init() -> Result<Self, Error> {
        #[cfg(feature = "pi")]
        let uart = Uart::with_path("/dev/serial0", 57_600, Parity::None, 8, 1)?;

        Ok(UartController {
            #[cfg(feature = "pi")]
            uart,
        })
    }

    pub fn send_data(&mut self, data: Vec<u8>) -> Result<(), Error> {
        #[cfg(feature = "pi")]
        {
            // Send data in chunks of 8 bytes
            for chunk in data.chunks(8) {
                self.uart.write(chunk)?;
            }

            // Block until the data is sent
            self.uart.drain()?;
        }

        Ok(())
    }

    pub async fn start(mut self, mut rx: mpsc::Receiver<UartMessage>) {
        while let Some(message) = rx.recv().await {
            match message {
                UartMessage::Laser(data) => {
                    if let Err(e) = self.send_data(data) {
                        error!("Failed to send projector data: {}", e);
                    }
                }
                UartMessage::DMX(data) => {
                    if let Err(e) = self.send_data(data) {
                        error!("Failed to send DMX data: {}", e);
                    }
                }
            }
        }
    }
}
