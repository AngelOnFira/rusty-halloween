use anyhow::Error;
use chrono::Local;
use env_logger::Builder;
use interprocess::local_socket::{LocalSocketListener, LocalSocketStream};
use log::{debug, error, info, warn, LevelFilter};
use prelude::{uart::UARTProjectorController, LoadedSong};
use projector::FrameSendPack;
use show::prelude::{ShowChoice, ShowElement, ShowManager};
use std::io::{
    Write, {self},
};
use tokio::{signal, sync::mpsc};

pub mod audio;
pub mod config;
pub mod dmx;
pub mod lights;
pub mod projector;
pub mod show;
pub mod structure;

pub mod prelude {
    pub use crate::{audio::*, config::*, lights::*, projector::*, show::*};
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

#[derive(Clone, Debug)]
pub enum InternalMessage {
    /// Files that just have hex to be dumped to SPI
    Vision { vision_file_contents: String },
    /// Change a light over GPIO
    Light { light_id: u8, enable: bool },
    /// Play an audio file
    Audio { audio_file_contents: LoadedSong },
    /// Direct projector frames
    Projector(FrameSendPack),
    /// DMX data
    // Dmx(DmxMessageSendPack),
    /// DMX send request
    DmxSendRequest,
}

/// Messages that should be processed in the queue
#[derive(Clone, Debug)]
pub enum MessageKind {
    // ExternalMessage(PicoMessage),
    InternalMessage(InternalMessage),
}
