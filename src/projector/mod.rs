use std::fmt::{Debug, Display};

use self::pack::{DrawPack, HeaderPack};

use crate::projector::pack::CheckSum;
use crate::show::LaserDataFrame;

use log::info;
// use rillrate::prime::{Click, ClickOpts};
use rust_embed::RustEmbed;

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

        write!(
            f,
            "Sending to {} with {}",
            projector,
            task,
        )
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
        info!("{msg}");

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

#[derive(RustEmbed)]
#[folder = "src/projector/visions/assets"]
struct VisionAsset;
