use std::fmt::{Debug, Display};

use self::pack::{DrawPack, HeaderPack};

use crate::{laser::pack::CheckSum, show::LaserDataFrame, uart::UartMessage};

use log::debug;
use rust_embed::RustEmbed;
use tokio::sync::mpsc;

pub mod pack;

type Frame = [u8; 4];

pub enum LaserMessage {
    Frame(FrameSendPack),
}

pub struct LaserController {}

impl LaserController {
    pub fn init() -> Self {
        Self {}
    }

    pub async fn start(
        &mut self,
        mut rx: mpsc::Receiver<LaserMessage>,
        uart_tx: mpsc::Sender<UartMessage>,
    ) {
        while let Some(message) = rx.recv().await {
            match message {
                LaserMessage::Frame(frame) => {
                    uart_tx
                        .send(UartMessage::Laser(frame.into_bytes()))
                        .await
                        .unwrap();
                }
            }
        }
    }
}

#[derive(PartialEq, Clone, Debug)]
pub struct FrameSendPack {
    pub header: Frame,
    pub draw_instructions: Vec<Frame>,
}

impl FrameSendPack {
    pub fn into_bytes(self) -> Vec<u8> {
        let mut bytes = Vec::new();

        bytes.extend_from_slice(&self.header);
        for draw_instruction in self.draw_instructions {
            bytes.extend_from_slice(&draw_instruction);
        }

        // Add extra bytes to pad up to 51 total frames including the header and
        // draw instructions
        while bytes.len() < 51 * 4 {
            bytes.push(0);
        }

        bytes
    }
}

#[derive(PartialEq, Clone, Debug)]
pub struct MessageSendPack {
    pub header: HeaderPack,
    pub draw_instructions: Vec<DrawPack>,
}

impl Display for MessageSendPack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut draw_instructions = String::new();
        for draw_pack in &self.draw_instructions {
            draw_instructions.push_str(&format!("{}\n", draw_pack));
        }

        let projector = match self.header.projector_id.to_string().as_str() {
            "15" => "all projectors".to_string(),
            id => format!("projector {}", id),
        };

        let task = match self.header.home {
            true => "a homing request".to_string(),
            false => format!("{} draw instructions", self.draw_instructions.len()),
        };

        write!(f, "Sending to {} with {}", projector, task,)
    }
}

impl MessageSendPack {
    pub fn new(header: HeaderPack, draw_instructions: Vec<LaserDataFrame>) -> Self {
        let draw_instructions = draw_instructions.into_iter().map(DrawPack::from).collect();

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
        debug!("{msg}");

        let pack = FrameSendPack {
            header: msg.header.checksum_pack(),
            draw_instructions: msg
                .draw_instructions
                .into_iter()
                .map(|mut x| x.checksum_pack())
                .collect(),
        };

        pack
    }
}
