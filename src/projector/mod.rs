use std::path::Path;

use self::pack::{DrawPack, HeaderPack};

use crate::projector::pack::CheckSum;
use crate::proto_schema::schema::Projector;
use crate::{InternalMessage, MessageKind};

use rillrate::prime::{Click, ClickOpts};
use rust_embed::RustEmbed;
use tokio::sync::mpsc;

#[cfg(feature = "pi")]
use rppal::spi::{Bus, SlaveSelect, Spi};
#[cfg(feature = "pi")]
use rppal::uart::{Parity, Uart};

pub mod helpers;
pub mod pack;

pub struct SPIProjectorController {
    #[cfg(feature = "pi")]
    pub spi: Spi,
    #[allow(dead_code)]
    clicks: Vec<Click>,
}

type Frame = [u8; 4];

#[cfg(feature = "pi")]
const BAUD: u32 = 9_600;

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

/// Change from a MessageSendPack to a FrameSendPack
impl From<MessageSendPack> for FrameSendPack {
    fn from(mut msg: MessageSendPack) -> FrameSendPack {
        FrameSendPack {
            header: msg.header.checksum_pack(),
            draw_instructions: msg
                .draw_instructions
                .into_iter()
                .map(|mut x| x.checksum_pack())
                .collect(),
        }
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

        let mut clicks = Vec::new();

        // Load each vision
        for vision in VisionAsset::iter() {
            let click = Click::new(
                format!(
                    "app.dashboard.Visions.{}",
                    Path::new(&vision.to_string())
                        .file_stem()
                        .unwrap()
                        .to_str()
                        .unwrap()
                ),
                ClickOpts::default().label("Play"),
            );

            let this = click.clone();

            let message_queue_clone = message_queue.clone();
            click.sync_callback(move |envelope| {
                if let Some(action) = envelope.action {
                    log::warn!("ACTION: {:?}", action);
                    this.apply();

                    message_queue_clone
                        .blocking_send(MessageKind::InternalMessage(InternalMessage::Vision {
                            vision_file_contents: std::str::from_utf8(
                                &VisionAsset::get(&vision).unwrap().data,
                            )
                            .unwrap()
                            .to_string(),
                        }))
                        .unwrap();
                }
                Ok(())
            });

            clicks.push(click);
        }

        Ok(SPIProjectorController {
            #[cfg(feature = "pi")]
            spi,
            clicks,
        })
    }

    #[allow(dead_code)]
    pub fn projector_to_frames(
        &mut self,
        projector_command: Projector,
    ) -> Result<(), anyhow::Error> {
        // Create the header from the message
        let header_command = projector_command.header;
        let header = HeaderPack {
            projector_id: (header_command.projector_id as u8).into(),
            point_count: (header_command.point_count as u8).into(),
            home: header_command.home,
            enable: header_command.enable,
            configuration_mode: header_command.configuration_mode,
            draw_boundary: header_command.draw_boundary,
            oneshot: header_command.oneshot,
            speed_profile: (header_command.speed_profile as u8).into(),
            checksum: header_command.checksum,
            ..HeaderPack::default()
        }
        .checksum_pack();

        let mut draw_instructions = Vec::new();
        for draw_command in projector_command.draw_instructions {
            let draw_pack = DrawPack {
                x: (draw_command.xCoOrd as u16).into(),
                y: (draw_command.yCoOrd as u16).into(),
                red: (draw_command.red as u8).into(),
                green: (draw_command.green as u8).into(),
                blue: (draw_command.blue as u8).into(),
                checksum: draw_command.checksum,
                ..DrawPack::default()
            }
            .checksum_pack();
            draw_instructions.push(draw_pack);
        }

        // Create a message pack
        let message_pack = FrameSendPack {
            header,
            draw_instructions,
        };

        self.spi_send_projector(message_pack)?;

        Ok(())
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
