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
    laser_uart: Uart,

    #[cfg(feature = "pi")]
    dmx_now_uart: Uart,
}

impl UartController {
    pub fn init() -> Result<Self, Error> {
        #[cfg(feature = "pi")]
        let laser_uart = Uart::with_path("/dev/serial0", 57_600, Parity::None, 8, 1)?;

        #[cfg(feature = "pi")]
        let dmx_now_uart = Uart::with_path("/tmp/wireless-dmx-raw", 250_000, Parity::None, 8, 1)?;

        Ok(UartController {
            #[cfg(feature = "pi")]
            uart,
        })
    }

    pub fn send_data(&mut self, data: UartMessage) -> Result<(), Error> {
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

    async fn start(mut self, mut rx: mpsc::Receiver<UartMessage>) {
        while let Some(message) = rx.recv().await {
            match message {
                UartMessage::Laser(data) => {
                    // Print out the array of bytes that were sent in binary format
                    for byte in &data {
                        print!("{:08b} ", byte);
                    }
                    println!();

                    #[cfg(feature = "pi")]
                    {
                        // Send data in chunks of 8 bytes
                        for chunk in data.chunks(8) {
                            self.laser_uart.write(chunk)?;
                        }
            
                        self.uart.drain()?;
                    }
                }
                UartMessage::DMX(data) => {
                    #[cfg(feature = "pi")]
                    {
                        // Send data in chunks of 8 bytes
                        for chunk in data.chunks(8) {
                            self.dmx_now_uart.write(chunk)?;
                        }
            
                        self.uart.drain()?;
                    }
                }
            }
        }
    }
}
