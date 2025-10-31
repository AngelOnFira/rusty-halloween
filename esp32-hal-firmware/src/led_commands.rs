/// LED control commands
///
/// These commands are sent from various tasks to the LED driver task
/// to control the physical LED strip.

use smart_leds::RGB8;

/// Commands that can be sent to the LED driver task
#[derive(Debug, Clone, Copy)]
pub enum LedCommand {
    /// Display a rainbow wave pattern (diagnostic - shows during WiFi connect)
    Wave,

    /// Turn all LEDs off
    Off,

    /// Set all LEDs to a specific color
    SetColor(RGB8),
}
