use std::path::Path;

use self::pack::{DrawPack, HeaderPack};

use crate::projector::pack::CheckSum;
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
