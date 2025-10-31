#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use defmt::info;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    rmt::Rmt,
    time::Rate,
    timer::timg::TimerGroup,
};
use esp_println as _;
use smart_leds::{brightness, gamma, SmartLedsWrite, RGB8};
use esp_hal_smartled::{smart_led_buffer, SmartLedsAdapter};

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    // generator version: 1.0.0

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(#[unsafe(link_section = ".dram2_uninit")] size: 139264);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0);

    info!("Embassy initialized!");

    let radio_init = esp_radio::init().expect("Failed to initialize Wi-Fi/BLE controller");
    let (mut _wifi_controller, _interfaces) =
        esp_radio::wifi::new(&radio_init, peripherals.WIFI, Default::default())
            .expect("Failed to initialize Wi-Fi controller");

    // Initialize RMT peripheral for smart LEDs
    info!("Initializing smart LEDs on GPIO4...");
    let rmt = Rmt::new(peripherals.RMT, Rate::from_mhz(80)).unwrap();

    // Create SmartLED driver using esp-hal-smartled
    // The smart_led_buffer! macro allocates a static buffer for 35 LEDs
    const NUM_LEDS: usize = 35;
    let mut led = SmartLedsAdapter::new(rmt.channel0, peripherals.GPIO4, smart_led_buffer!(NUM_LEDS));
    let mut color_data: [RGB8; NUM_LEDS] = [RGB8::default(); NUM_LEDS];

    info!("Smart LEDs initialized! Starting color cycle...");

    // TODO: Spawn some tasks
    let _ = spawner;

    let mut hue: u8 = 0;

    loop {
        // Create a rainbow pattern across all LEDs
        for i in 0..NUM_LEDS {
            let pixel_hue = hue.wrapping_add((i as u8) * (255 / NUM_LEDS as u8));
            color_data[i] = hsv_to_rgb(pixel_hue, 255, 255);
        }

        // Simple write using SmartLedsWrite trait - much cleaner!
        led.write(brightness(gamma(color_data.iter().cloned()), 32))
            .ok();

        hue = hue.wrapping_add(1);

        Timer::after(Duration::from_millis(20)).await;
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0/examples/src/bin
}

/// Convert HSV to RGB color
fn hsv_to_rgb(h: u8, s: u8, v: u8) -> RGB8 {
    if s == 0 {
        return RGB8::new(v, v, v);
    }

    let region = h / 43;
    let remainder = (h - (region * 43)) * 6;

    let p = (v as u16 * (255 - s as u16)) / 255;
    let q = (v as u16 * (255 - ((s as u16 * remainder as u16) / 255))) / 255;
    let t = (v as u16 * (255 - ((s as u16 * (255 - remainder as u16)) / 255))) / 255;

    match region {
        0 => RGB8::new(v, t as u8, p as u8),
        1 => RGB8::new(q as u8, v, p as u8),
        2 => RGB8::new(p as u8, v, t as u8),
        3 => RGB8::new(p as u8, q as u8, v),
        4 => RGB8::new(t as u8, p as u8, v),
        _ => RGB8::new(v, p as u8, q as u8),
    }
}
