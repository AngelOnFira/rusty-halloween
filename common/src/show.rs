use serde::{Deserialize, Serialize};

// Device count constants
pub const MAX_LIGHTS: usize = 7;
pub const MAX_LASERS: usize = 5;
pub const MAX_PROJECTORS: usize = 1;
pub const MAX_TURRETS: usize = 4;

// Type aliases for DMX state
pub type DmxStateData = u8;
pub type DmxStateIndex = u8;
pub type DmxStateVarPosition = (DmxStateIndex, DmxStateData);

/// A serializable show that contains just the frame data without audio
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SerializableShow {
    pub name: String,
    pub frames: Vec<Frame>,
}

/// A frame consists of a timestamp since the beginning of this show, a list of
/// commands for the lights, and a list of commands for the lasers.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Frame {
    pub timestamp: u64,
    pub lights: Vec<Option<bool>>,
    pub lasers: Vec<Option<Laser>>,
    pub projectors: Vec<Option<Projector>>,
    pub turrets: Vec<Option<Turret>>,
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Projector {
    pub state: DmxStateVarPosition,
    pub gallery: DmxStateVarPosition,
    pub pattern: DmxStateVarPosition,
    pub colour: DmxStateVarPosition,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Turret {
    pub state: DmxStateVarPosition,
    pub pan: DmxStateVarPosition,
    pub tilt: DmxStateVarPosition,
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
