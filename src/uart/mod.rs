use tokio::sync::mpsc;
use anyhow::Error;

#[cfg(feature = "pi")]
use rppal::uart::{Parity, Uart};

pub enum UartMessage {
    Projector(Vec<u8>),
    DMX(Vec<u8>),
}

pub struct UartController {
    #[cfg(feature = "pi")]
    uart: Uart,
}

impl UartController {
    pub async fn init(message_queue: mpsc::Sender<MessageKind>) -> Result<Self, Error> {
        #[cfg(feature = "pi")]
        let uart = Uart::with_path("/dev/serial0", 57_600, Parity::None, 8, 1)?;

        Ok(UartController {
            #[cfg(feature = "pi")]
            uart,
        })
    }

    pub fn send_data(&mut self, data: Vec<u8>) -> Result<(), Error> {
        #[cfg(feature = "pi")]
        self.uart.write(&data)?;

        Ok(())
    }

    pub async fn start(mut self, mut rx: mpsc::Receiver<UartMessage>) {
        while let Some(message) = rx.recv().await {
            match message {
                UartMessage::Projector(data) => {
                    if let Err(e) = self.send_data(data) {
                        error!("Failed to send projector data: {}", e);
                    }
                    // Sleep for the calculated delay (as in your original code)
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                },
                UartMessage::DMX(data) => {
                    if let Err(e) = self.send_data(data) {
                        error!("Failed to send DMX data: {}", e);
                    }
                }
            }
        }
    }
}