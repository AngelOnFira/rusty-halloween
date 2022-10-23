use interprocess::local_socket::LocalSocketStream;

use projector::FrameSendPack;

use std::io::{self};

pub mod audio;
pub mod config;
pub mod dashboard;
pub mod lights;
pub mod projector;
pub mod proto_schema;
pub mod show;

pub mod prelude {
    pub use crate::audio::*;
    pub use crate::config::*;
    pub use crate::dashboard::*;
    pub use crate::lights::*;
    pub use crate::projector::*;
    pub use crate::proto_schema::*;
    pub use crate::show::*;
}

fn handle_error(conn: io::Result<LocalSocketStream>) -> Option<LocalSocketStream> {
    match conn {
        Ok(val) => Some(val),
        Err(error) => {
            eprintln!("Incoming connection failed: {}", error);
            None
        }
    }
}

#[derive(PartialEq, Clone, Debug)]
pub enum InternalMessage {
    /// Files that just have hex to be dumped to SPI
    Vision { vision_file_contents: String },
    /// Change a light over GPIO
    Light { light_id: u8, enable: bool },
    /// Play an audio file
    Audio { audio_file_contents: String },
    /// Direct projector frames
    Projector(FrameSendPack),
}

/// Messages that should be processed in the queue
#[derive(PartialEq, Clone, Debug)]
pub enum MessageKind {
    // ExternalMessage(PicoMessage),
    InternalMessage(InternalMessage),
}
