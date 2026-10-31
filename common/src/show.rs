use serde::{Deserialize, Serialize};

#[cfg(not(feature = "std"))]
extern crate alloc;

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
#[cfg(not(feature = "std"))]
use alloc::string::String;

// Device count constants
pub const MAX_LIGHTS: usize = 7;
pub const MAX_LASERS: usize = 5;

// Type aliases for DMX state. Indexes are DMX channels (1-512), so they need
// more than 8 bits
pub type DmxStateData = u8;
pub type DmxStateIndex = u16;
pub type DmxStateVarPosition = (DmxStateIndex, DmxStateData);

/// A serializable show that contains just the frame data without audio
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SerializableShow {
    pub name: String,
    pub frames: Vec<Frame>,
}

/// A frame consists of a timestamp since the beginning of this show, a list of
/// commands for the GPIO lights, a list of commands for the lasers, and the
/// DMX channel writes for every DMX device in the frame.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Frame {
    pub timestamp: u64,
    pub lights: Vec<Option<bool>>,
    pub lasers: Vec<Option<Laser>>,
    pub dmx: Vec<DmxStateVarPosition>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Laser {
    // Laser config
    pub home: bool,
    pub point_count: u8,
    pub speed_profile: u8,
    pub enable: bool,
    // Laser data
    pub hex: [u8; 3],
    pub value: u8,
}

// Device instruction types for ESP32 devices

/// Device-specific instructions for a time window
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeviceInstructions {
    pub device_id: String,
    pub instructions: Vec<TimedInstruction>,
}

/// A single timed instruction for a device
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TimedInstruction {
    pub timestamp: u64, // milliseconds from show start
    pub command: DeviceCommand,
}

/// Commands that can be sent to ESP32 devices
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DeviceCommand {
    /// Turn a light on or off
    Light { enabled: bool },
    /// Set RGB color values
    Rgb { r: u8, g: u8, b: u8 },
    /// Custom command with arbitrary data
    Custom { data: Vec<u8> },
}
