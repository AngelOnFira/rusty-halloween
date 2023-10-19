use std::path::Path;


use crate::projector::pack::CheckSum;
use crate::proto_schema::schema::Projector;
use crate::{InternalMessage, MessageKind};

use rillrate::prime::{Click, ClickOpts};
use rust_embed::RustEmbed;
use tokio::sync::mpsc;

pub struct UARTProjectorController {
    #[cfg(feature = "pi")]
    pub uart: Uart,
}

const UART_BAUD: u32 = 115_200;

impl UARTProjectorController {
    pub async fn init() -> Result<Self, anyhow::Error> {
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

        let mut data = vec![0xF0, 0x4C, 0x00, 0x01, 0x32, 0x19, 0x38, 0x01, 0x96, 0x19, 0x38, 0x00, 0x96, 0x4B, 0x38, 0x01, 0x32, 0x4B, 0x38, 0x00];

        for _ in 0..46 {
            data.push(0);
        }


        // Send some data
        #[cfg(feature = "pi")]
        uart.write(&data)?;

        Ok(UARTProjectorController {
            #[cfg(feature = "pi")]
            uart,
        })
    }
}
