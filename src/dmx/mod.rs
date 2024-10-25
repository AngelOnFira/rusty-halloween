use std::fmt::{Debug, Display};

use crate::projector::pack::CheckSum;
use crate::show::LaserDataFrame;

use log::{debug, info};
use pack::{DmxDataPack, DmxHeaderPack};
use rust_embed::RustEmbed;

pub mod pack;

type DmxFrame = u8;

const DMX_CHANNELS: usize = 255;

pub struct DmxState {
    pub device_name: String,
    pub channel_id: u64,
    pub values: [u8; DMX_CHANNELS],
}

#[derive(PartialEq, Clone, Debug)]
pub struct FrameSendPack {
    pub header: DmxFrame,
    pub dmx_channel_data: Vec<DmxFrame>,
}

impl FrameSendPack {
    pub fn into_bytes(self) -> Vec<u8> {
        let mut bytes = Vec::new();

        bytes.push(self.header);
        for channel_data in self.dmx_channel_data {
            bytes.push(channel_data);
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
pub struct DmxMessageSendPack {
    pub header: DmxHeaderPack,
    pub channel_data: [DmxDataPack; DMX_CHANNELS],
}

impl Display for DmxMessageSendPack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Show the data in the format:
        // Controller ID: 0x0A, Universe: 0x00
        // FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF
        // ... (255 bytes of channel data)
        writeln!(
            f,
            "Controller ID: 0x{:02X}, Universe: 0x{:02X}",
            self.header.controller_id.to_be(),
            self.header.universe.to_be()
        )?;

        for (i, data_pack) in self.channel_data.iter().enumerate() {
            if i % 16 == 0 && i != 0 {
                writeln!(f)?;
            }
            write!(f, "{:02X} ", data_pack.channel_data.to_be())?;
        }

        // Pad with zeros if less than 255 bytes of channel data
        for i in self.channel_data.len()..DMX_CHANNELS {
            if i % 16 == 0 && i != 0 {
                writeln!(f)?;
            }
            write!(f, "00 ")?;
        }

        // Add information about the projector and task
        let projector = match self.header.controller_id.to_be() {
            15 => "all projectors".to_string(),
            id => format!("projector {}", id),
        };

        let task = if self.header.universe.to_be() == 0 {
            "a homing request".to_string()
        } else {
            format!("{} draw instructions", self.channel_data.len())
        };

        write!(f, "\nSending to {} with {}", projector, task)
    }
}

impl DmxMessageSendPack {
    pub fn new(header: DmxHeaderPack, channel_data: Vec<DmxDataPack>) -> Self {
        DmxMessageSendPack {
            header,
            channel_data: channel_data.try_into().unwrap(),
        }
    }
}

/// Change from a MessageSendPack to a FrameSendPack
impl From<DmxMessageSendPack> for FrameSendPack {
    fn from(mut msg: DmxMessageSendPack) -> FrameSendPack {
        debug!("{msg}");

        let pack = FrameSendPack {
            header: msg.header.checksum_pack(),
            dmx_channel_data: msg
                .channel_data
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
