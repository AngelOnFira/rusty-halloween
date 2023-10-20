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

const UART_BAUD: u32 = 57_600;

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
            let mut data = Vec::new();

            // Send the header
            data.extend_from_slice(&projector_command.header);

            // Send each draw instruction
            for draw_pack in &projector_command.draw_instructions {
                data.extend_from_slice(draw_pack);
            }

            // Send extra frames to get to 51 total frames
            for _ in 0..(51 - projector_command.draw_instructions.len() - 1) {
                data.extend_from_slice(&[0, 0, 0, 0]);
            }

            // Send the data buffer
            self.uart.write(&data)?;
        }

        Ok(())
    }
}
