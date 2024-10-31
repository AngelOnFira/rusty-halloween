use std::fmt::{Debug, Display};

use self::pack::{HeaderPack, PatternPack};

use crate::{laser::pack::CheckSum, show::LaserDataFrame, uart::UartMessage};

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
    pub draw_instruction: Frame,
}

impl FrameSendPack {
    pub fn into_bytes(self) -> Vec<u8> {
        let mut bytes = Vec::new();

        bytes.extend_from_slice(&self.header);
        bytes.extend_from_slice(&self.draw_instruction);

        bytes
    }
}

#[derive(PartialEq, Clone, Debug)]
pub struct MessageSendPack {
    pub header: HeaderPack,
    pub draw_instruction: PatternPack,
}

impl Display for MessageSendPack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut draw_instructions = String::new();
        draw_instructions.push_str(&format!("{}\n", self.draw_instruction));

        let laser = match self.header.laser_id.to_string().as_str() {
            "15" => "all lasers".to_string(),
            id => format!("laser {}", id),
        };

        let task = match self.header.home {
            true => "a homing request".to_string(),
            false => format!("{} draw instructions", 1),
        };

        write!(f, "Sending to {} with {}", laser, task,)
    }
}

impl MessageSendPack {
    pub fn new(header: HeaderPack, draw_instruction: LaserDataFrame) -> Self {
        let draw_instruction = PatternPack::from(draw_instruction);

        MessageSendPack {
            header,
            draw_instruction,
        }
    }
    pub fn home_message() -> Self {
        MessageSendPack {
            header: HeaderPack {
                laser_id: 15.into(),
                home: true,
                enable: true,
                ..Default::default()
            },
            draw_instruction: PatternPack::default(),
        }
    }
}

/// Change from a MessageSendPack to a FrameSendPack
impl From<MessageSendPack> for FrameSendPack {
    fn from(mut msg: MessageSendPack) -> FrameSendPack {
        let pack = FrameSendPack {
            header: msg.header.checksum_pack(),
            draw_instruction: msg.draw_instruction.checksum_pack(),
        };

        pack
    }
}
