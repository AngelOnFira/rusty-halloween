use std::path::Path;

use self::pack::{DrawPack, HeaderPack};

use crate::projector::pack::CheckSum;
use crate::proto_schema::schema::Projector;
use crate::show::LaserDataFrame;
use crate::{InternalMessage, MessageKind};

// use rillrate::prime::{Click, ClickOpts};
use rust_embed::RustEmbed;
use tokio::sync::mpsc;

pub mod pack;
pub mod spi;
pub mod uart;

type Frame = [u8; 4];

#[derive(PartialEq, Clone, Debug)]
pub struct FrameSendPack {
    pub header: Frame,
    pub draw_instructions: Vec<Frame>,
}

#[derive(PartialEq, Clone, Debug)]
pub struct MessageSendPack {
    pub header: HeaderPack,
    pub draw_instructions: Vec<DrawPack>,
}

impl MessageSendPack {
    pub fn new(header: HeaderPack, draw_instructions: Vec<LaserDataFrame>) -> Self {
        let draw_instructions = draw_instructions
            .into_iter()
            .map(|x| DrawPack::from(x))
            .collect();

        MessageSendPack {
            header,
            draw_instructions,
        }
    }
    pub fn home_message() -> Self {
        MessageSendPack {
            header: HeaderPack {
                projector_id: 15.into(),
                home: true,
                enable: true,
                ..Default::default()
            },
            draw_instructions: Vec::new(),
        }
    }
}

/// Change from a MessageSendPack to a FrameSendPack
impl From<MessageSendPack> for FrameSendPack {
    fn from(mut msg: MessageSendPack) -> FrameSendPack {
        println!("Message {:#?}", &msg);
        let pack = FrameSendPack {
            header: msg.header.checksum_pack(),
            draw_instructions: msg
                .draw_instructions
                .into_iter()
                .map(|mut x| x.checksum_pack())
                .collect(),
        };
        println!("Pack {:#?}", &pack);
        pack
    }
}

#[derive(RustEmbed)]
#[folder = "src/projector/visions/assets"]
struct VisionAsset;

impl SPIProjectorController {
    pub async fn init(message_queue: mpsc::Sender<MessageKind>) -> Result<Self, anyhow::Error> {
        // Set up SPI
        #[cfg(feature = "pi")]
        let spi = Spi::new(Bus::Spi0, SlaveSelect::Ss0, BAUD, rppal::spi::Mode::Mode0)?;

        // let mut clicks = Vec::new();

        // Load each vision
        for vision in VisionAsset::iter() {
            // let click = Click::new(
            //     format!(
            //         "app.dashboard.Visions.{}",
            //         Path::new(&vision.to_string())
            //             .file_stem()
            //             .unwrap()
            //             .to_str()
            //             .unwrap()
            //     ),
            //     ClickOpts::default().label("Play"),
            // );

            // let this = click.clone();

            // let message_queue_clone = message_queue.clone();
            // click.sync_callback(move |envelope| {
            //     if let Some(action) = envelope.action {
            //         log::warn!("ACTION: {:?}", action);
            //         this.apply();

            //         message_queue_clone
            //             .blocking_send(MessageKind::InternalMessage(InternalMessage::Vision {
            //                 vision_file_contents: std::str::from_utf8(
            //                     &VisionAsset::get(&vision).unwrap().data,
            //                 )
            //                 .unwrap()
            //                 .to_string(),
            //             }))
            //             .unwrap();
            //     }
            //     Ok(())
            // });

            // clicks.push(click);
        }

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

pub struct UARTProjectorController {
    #[cfg(feature = "pi")]
    pub uart: Uart,
}

const UART_BAUD: u32 = 56_000;

impl UARTProjectorController {
    pub async fn init() -> Result<Self, anyhow::Error> {
        // Set up UART
        #[cfg(feature = "pi")]
        let mut uart = Uart::with_path("/dev/serial0", UART_BAUD, Parity::None, 8, 1)?;

        let mut data = vec![
            0xF0, 0x4C, 0x00, 0x01, //
            0x32, 0x19, 0x38, 0x01, //
            0x96, 0x19, 0x38, 0x00, //
            0x96, 0x4B, 0x38, 0x01, //
            0x32, 0x4B, 0x38, 0x00, //
        ];

        for _ in 0..46 {
            data.append(&mut vec![0x00, 0x00, 0x00, 0x00]);
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
