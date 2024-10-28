use laser::FrameSendPack;
use prelude::LoadedSong;
use show::prelude::DmxStateVarPosition;

pub mod audio;
pub mod config;
pub mod dmx;
pub mod laser;
pub mod lights;
pub mod show;
pub mod structure;
pub mod uart;

pub mod prelude {
    pub use crate::{audio::*, config::*, laser::*, lights::*, show::*};
}

#[derive(Clone, Debug)]
pub enum InternalMessage {
    /// Change a light over GPIO
    Light { light_id: u8, enable: bool },
    /// Play an audio file
    Audio { audio_file_contents: LoadedSong },
    /// Stop audio playback
    AudioStop,
    /// Direct projector frames
    Laser(FrameSendPack),
    /// DMX data
    DmxUpdateState(Vec<DmxStateVarPosition>),
    /// DMX send request
    DmxSendRequest,
}

// Add new enum for audio controller messages
#[derive(Debug)]
pub enum AudioMessage {
    Play(LoadedSong),
    Stop,
}

/// Messages that should be processed in the queue
#[derive(Clone, Debug)]
pub enum MessageKind {
    // ExternalMessage(PicoMessage),
    InternalMessage(InternalMessage),
}
