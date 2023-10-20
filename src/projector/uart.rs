use std::path::Path;

use crate::projector::pack::CheckSum;
use crate::proto_schema::schema::Projector;
use crate::{InternalMessage, MessageKind};

use rillrate::prime::{Click, ClickOpts};
use rust_embed::RustEmbed;
use tokio::sync::mpsc;

use super::pack::{DrawPack, HeaderPack};
use super::{FrameSendPack, VisionAsset};

#[cfg(feature = "pi")]
use rppal::uart::{Parity, Uart};

pub struct UARTProjectorController {
    #[cfg(feature = "pi")]
    pub uart: Uart,
}

const UART_BAUD: u32 = 115_200;

impl UARTProjectorController {
    pub async fn init(message_queue: mpsc::Sender<MessageKind>) -> Result<Self, anyhow::Error> {
        // Set up UART
        #[cfg(feature = "pi")]
        let mut uart = Uart::with_path("/dev/serial0", UART_BAUD, Parity::None, 8, 1)?;

        // ser.write(b'\xF0\x4C\x00\x01')
        // ser.write(b'\x32\x19\x38\x01')
        // ser.write(b'\x96\x19\x38\x00')
        // ser.write(b'\x96\x4B\x38\x01')
        // ser.write(b'\x32\x4B\x38\x00')

        // for i in range(46):
        //     ser.write(b'\x00\x00\x00\x00')

        // let mut data = vec![
        //     0xF0, 0x4C, 0x00, 0x01, 0x32, 0x19, 0x38, 0x01, 0x96, 0x19, 0x38, 0x00, 0x96, 0x4B,
        //     0x38, 0x01, 0x32, 0x4B, 0x38, 0x00,
        // ];

        // for _ in 0..46 {
        //     data.push(0);
        // }

        // // Send some data
        // #[cfg(feature = "pi")]
        // uart.write(&data)?;

        Ok(UARTProjectorController {
            #[cfg(feature = "pi")]
            uart,
        })
    }

    #[allow(dead_code)]
    pub fn uart_send_projector(
        &mut self,
        #[allow(unused_variables)] projector_command: FrameSendPack,
    ) -> Result<(), anyhow::Error> {
        #[cfg(feature = "pi")]
        {
            // Send the header
            self.uart.write(&projector_command.header)?;

            // Send each draw instruction
            for draw_pack in &projector_command.draw_instructions {
                self.uart.write(draw_pack)?;
            }

            // Send extra frames to get to 51 total frames
            for _ in 0..(51 - projector_command.draw_instructions.len() - 1) {
                self.uart.write(&[0, 0, 0, 0])?;
            }
        }

        Ok(())
    }

    #[allow(dead_code)]
    pub fn uart_send_file(&mut self, file_string: &str) -> Result<(), anyhow::Error> {
        #[allow(unused_variables)]
        let frames = file_string
            .lines()
            .map(|s| u32::to_be_bytes(u32::from_str_radix(s, 16).unwrap()))
            .collect::<Vec<[u8; 4]>>();

        // Create frames in the proto format

        #[cfg(feature = "pi")]
        {
            // Send the header
            self.uart.write(&frames[0])?;

            // Send each draw instruction
            for frame in frames[1..].iter() {
                self.uart.write(frame)?;
            }

            // Send any extra frames required to get to 51 total frames
            for _ in 0..(51 - frames.len()) {
                self.uart.write(&[0, 0, 0, 0])?;
            }
        }

        Ok(())
    }
}
