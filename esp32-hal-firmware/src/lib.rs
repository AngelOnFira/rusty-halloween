#![no_std]

extern crate alloc;

pub mod esp32_types;
pub mod http_client;
pub mod led_executor;
pub mod ntp;
pub mod wifi;

// Provide random() function for esp-mbedtls RNG
// Uses ESP32-S2 hardware TRNG for cryptographically secure random numbers
#[unsafe(no_mangle)]
extern "C" fn random() -> core::ffi::c_ulong {
    use esp_hal::rng::Rng;

    // Rng is a zero-sized type that reads from hardware registers
    // Safe to create on each call
    let rng = Rng::new();
    rng.random() as core::ffi::c_ulong
}
