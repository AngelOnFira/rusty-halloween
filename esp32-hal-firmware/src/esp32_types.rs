/// ESP32-specific types for LED instructions
///
/// This is intentionally simple and separate from the main show types
/// so that the ESP32 firmware remains stable and doesn't need reflashing
/// when server logic changes.

extern crate alloc;
use alloc::vec::Vec;
use alloc::string::String;
use serde::Deserialize;
use smart_leds::RGB8;

/// A timed LED instruction
#[derive(Debug, Clone, Deserialize)]
pub struct Esp32Instruction {
    /// Timestamp in milliseconds from show start
    pub timestamp: u64,

    /// RGB color (if present, sets color)
    #[serde(default)]
    pub r: Option<u8>,
    #[serde(default)]
    pub g: Option<u8>,
    #[serde(default)]
    pub b: Option<u8>,

    /// If true, turn off the lights
    #[serde(default)]
    pub off: Option<bool>,
}

impl Esp32Instruction {
    /// Convert to RGB8 color
    /// Returns None if this is an "off" command
    pub fn to_color(&self) -> Option<RGB8> {
        if self.off == Some(true) {
            return Some(RGB8::new(0, 0, 0));
        }

        // If we have RGB values, use them
        if let (Some(r), Some(g), Some(b)) = (self.r, self.g, self.b) {
            return Some(RGB8::new(r, g, b));
        }

        None
    }
}

/// Device instructions response from server
#[derive(Debug, Clone, Deserialize)]
pub struct Esp32Response {
    pub device_id: String,
    /// Server's authoritative show start time (Unix timestamp in milliseconds)
    /// ESP32 uses this to calculate show time: current_time - show_start_time
    pub show_start_time: u64,
    pub instructions: Vec<Esp32Instruction>,
}
