use crate::MessageKind;

use tokio::sync::mpsc;

use super::FrameSendPack;

#[cfg(feature = "pi")]
use rppal::spi::{Bus, SlaveSelect, Spi};

#[cfg(feature = "pi")]
const BAUD: u32 = 9_600;

pub struct SPIProjectorController {
    #[cfg(feature = "pi")]
    pub spi: Spi,
    // #[allow(dead_code)]
    // clicks: Vec<Click>,
}

impl SPIProjectorController {
    pub async fn init(_message_queue: mpsc::Sender<MessageKind>) -> Result<Self, anyhow::Error> {
        // Set up SPI
        #[cfg(feature = "pi")]
        let spi = Spi::new(Bus::Spi0, SlaveSelect::Ss0, BAUD, rppal::spi::Mode::Mode0)?;

        // let mut clicks = Vec::new();

        Ok(SPIProjectorController {
            #[cfg(feature = "pi")]
            spi,
            // clicks,
        })
    }

    #[allow(dead_code)]
    pub fn spi_send_projector(
        &mut self,
        #[allow(unused_variables)] projector_command: FrameSendPack,
    ) -> Result<(), anyhow::Error> {
        #[cfg(feature = "pi")]
        {
            // Send the header
            self.spi.write(&projector_command.header)?;

            // Send each draw instruction
            for draw_pack in &projector_command.draw_instructions {
                self.spi.write(draw_pack)?;
            }

            // Send extra frames to get to 51 total frames
            for _ in 0..(51 - projector_command.draw_instructions.len() - 1) {
                self.spi.write(&[0, 0, 0, 0])?;
            }
        }

        Ok(())
    }

    #[allow(dead_code)]
    pub fn spi_send_file(&mut self, file_string: &str) -> Result<(), anyhow::Error> {
        #[allow(unused_variables)]
        let frames = file_string
            .lines()
            .map(|s| u32::to_be_bytes(u32::from_str_radix(s, 16).unwrap()))
            .collect::<Vec<[u8; 4]>>();

        // Create frames in the proto format

        #[cfg(feature = "pi")]
        {
            // Send the header
            self.spi.write(&frames[0])?;

            // Send each draw instruction
            for frame in frames[1..].iter() {
                self.spi.write(frame)?;
            }

            // Send any extra frames required to get to 51 total frames
            for _ in 0..(51 - frames.len()) {
                self.spi.write(&[0, 0, 0, 0])?;
            }
        }

        Ok(())
    }
}
