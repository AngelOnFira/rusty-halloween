use defmt::{debug, error, info};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use smart_leds::RGB8;

/// LED controller state
pub struct LedController {
    /// Current LED color
    current_color: RGB8,
    /// Number of LEDs in the strip (for future RMT implementation)
    _num_leds: usize,
}

impl LedController {
    pub fn new(num_leds: usize) -> Self {
        Self {
            current_color: RGB8::new(0, 0, 0),
            _num_leds: num_leds,
        }
    }

    /// Set LED color
    pub fn set_color(&mut self, color: RGB8) {
        self.current_color = color;
        debug!("LED color set to RGB({}, {}, {})", color.r, color.g, color.b);
    }

    /// Get current color
    pub fn get_color(&self) -> RGB8 {
        self.current_color
    }

    /// Turn off all LEDs
    pub fn turn_off(&mut self) {
        self.current_color = RGB8::new(0, 0, 0);
        debug!("LEDs turned off");
    }
}

/// Global LED controller instance
pub static LED_CONTROLLER: Mutex<CriticalSectionRawMutex, Option<LedController>> =
    Mutex::new(None);

/// Initialize LED controller
pub async fn init_led_controller(num_leds: usize) {
    let mut led = LED_CONTROLLER.lock().await;
    *led = Some(LedController::new(num_leds));
    info!("LED controller initialized with {} LEDs", num_leds);
}

/// Set LED color
pub async fn set_color(color: RGB8) {
    let mut led = LED_CONTROLLER.lock().await;
    if let Some(controller) = led.as_mut() {
        controller.set_color(color);
    } else {
        error!("LED controller not initialized");
    }
}

/// Turn off LEDs
pub async fn turn_off() {
    let mut led = LED_CONTROLLER.lock().await;
    if let Some(controller) = led.as_mut() {
        controller.turn_off();
    }
}

/// Get current LED color
pub async fn get_color() -> Option<RGB8> {
    let led = LED_CONTROLLER.lock().await;
    led.as_ref().map(|controller| controller.get_color())
}
