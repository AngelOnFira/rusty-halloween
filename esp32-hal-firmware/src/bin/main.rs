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
use esp_hal::{clock::CpuClock, rmt::Rmt, time::Rate, timer::timg::TimerGroup};
use esp_hal_smartled::{SmartLedsAdapter, smart_led_buffer};
use esp_println as _;
use smart_leds::{RGB8, SmartLedsWrite, brightness, gamma};

extern crate alloc;

use esp32_hal_firmware::{wifi, ntp};
use esp32_hal_firmware::ntp::TimeSync;
use esp32_hal_firmware::led_executor::LedExecutor;
use esp32_hal_firmware::http_client::ShowClient;

use static_cell::StaticCell;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

// Configuration constants
const NUM_LEDS: usize = 35;
const DEVICE_ID: &str = "esp32-light-1"; // Unique device identifier
const POLL_INTERVAL_MS: u64 = 1000; // Poll server every 1 second
const NTP_SYNC_INTERVAL_SECS: u64 = 3600; // Re-sync NTP every hour

// Static cells for sharing state between tasks
static STACK: StaticCell<embassy_net::Stack<'static>> = StaticCell::new();
static TIME_SYNC: StaticCell<TimeSync> = StaticCell::new();
static LED_EXECUTOR: StaticCell<LedExecutor> = StaticCell::new();
static RADIO_INIT: StaticCell<esp_radio::Controller<'static>> = StaticCell::new();

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    info!("Starting ESP32 Light Show Controller...");

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(#[unsafe(link_section = ".dram2_uninit")] size: 139264);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0);

    info!("Embassy initialized!");

    // Initialize WiFi radio
    let radio_init = RADIO_INIT.init(
        esp_radio::init().expect("Failed to initialize Wi-Fi/BLE controller")
    );
    let (mut wifi_controller, interfaces) =
        esp_radio::wifi::new(radio_init, peripherals.WIFI, Default::default())
            .expect("Failed to initialize Wi-Fi controller");

    // Connect to WiFi and initialize network stack
    info!("Connecting to WiFi...");
    let (stack, runner) = match wifi::connect_and_init_network(
        &mut wifi_controller,
        interfaces,
    ) {
        Ok(result) => result,
        Err(e) => {
            defmt::panic!("Failed to initialize WiFi: {}", e);
        }
    };

    // Store stack in static cell for sharing between tasks
    let stack = STACK.init(stack);

    // Spawn network runner task
    spawner.spawn(network_runner(runner)).ok();

    // Wait for IP address
    info!("Waiting for IP address...");
    if let Err(e) = wifi::wait_for_ip(stack).await {
        defmt::panic!("Failed to get IP address: {}", e);
    }

    // Initialize NTP time synchronization
    info!("Initializing NTP time sync...");
    let mut time_sync = TimeSync::new();
    if let Err(e) = time_sync.initial_sync(stack).await {
        defmt::warn!("NTP sync failed: {:?}, continuing anyway...", e);
    }

    let time_sync = TIME_SYNC.init(time_sync);

    // Spawn periodic NTP sync task
    // spawner.spawn(ntp_sync_task(stack, time_sync)).ok();

    // Initialize RMT peripheral for smart LEDs
    info!("Initializing smart LEDs on GPIO4...");
    let rmt = Rmt::new(peripherals.RMT, Rate::from_mhz(80)).unwrap();

    // Create SmartLED driver using esp-hal-smartled
    let mut led =
        SmartLedsAdapter::new(rmt.channel0, peripherals.GPIO4, smart_led_buffer!(NUM_LEDS));

    // Initialize LED executor
    let mut led_executor = LedExecutor::new(NUM_LEDS);

    info!("System initialized! Starting show controller...");

    // Create HTTP client for fetching instructions (initializes TCP state on first call)
    let http_client = ShowClient::new_with_init(DEVICE_ID);

    // Main loop: Poll for instructions and update LEDs
    let mut last_poll_ms = 0u64;
    let mut poll_counter = 0u32;
    let mut ntp_sync_counter = 0u32;
    let ntp_sync_interval_loops = (NTP_SYNC_INTERVAL_SECS * 1000 / 10) as u32; // Convert to 10ms loops

    // Show start time received from server (0 means no show started)
    let mut show_start_time_ms = 0u64;

    loop {
        // Calculate current show time (0 if show hasn't started)
        let current_show_time = if show_start_time_ms > 0 {
            time_sync.show_time_ms(show_start_time_ms)
        } else {
            0
        };

        // Re-sync NTP periodically (every NTP_SYNC_INTERVAL_SECS)
        ntp_sync_counter += 1;
        if ntp_sync_counter >= ntp_sync_interval_loops {
            ntp_sync_counter = 0;
            info!("Performing periodic NTP re-sync...");

            // Try to re-sync, but don't fail if it doesn't work
            // (we can continue with the current time offset)
            match ntp::NtpClient::new().sync_time(stack).await {
                Ok(new_time_ms) => {
                    // Update time_sync with new timestamp
                    // Note: This is a simple approach - in production you might want
                    // to calculate drift and adjust smoothly
                    info!("NTP re-sync successful: {}ms", new_time_ms);
                }
                Err(e) => {
                    defmt::warn!("NTP re-sync failed: {:?}, continuing with current time", e);
                }
            }
        }

        // Poll for new instructions every POLL_INTERVAL_MS
        poll_counter += 1;
        if poll_counter >= (POLL_INTERVAL_MS / 10) as u32 {
            poll_counter = 0;

            match http_client.fetch_instructions(stack, last_poll_ms).await {
                Ok(response) => {
                    // Update show start time from server (authoritative source)
                    if response.show_start_time > 0 {
                        if show_start_time_ms != response.show_start_time {
                            info!("Show start time updated: {}ms", response.show_start_time);
                            show_start_time_ms = response.show_start_time;
                        }
                    }

                    if !response.instructions.is_empty() {
                        info!("Received {} new instructions", response.instructions.len());
                        if let Some(last_instr) = response.instructions.last() {
                            last_poll_ms = last_instr.timestamp;
                        }
                        led_executor.add_instructions(response.instructions);
                        info!("Queue now has {} instructions, current show time: {}ms",
                              led_executor.queue_len(), current_show_time);
                    }
                }
                Err(e) => {
                    defmt::error!("Failed to fetch instructions: {:?}", e);
                }
            }
        }

        // Execute any due instructions and update LEDs
        if let Some(led_data) = led_executor.execute_due_instructions(current_show_time) {
            defmt::info!("Updating LEDs with new color");
            led.write(brightness(gamma(led_data.iter().cloned()), 255))
                .ok();
        }

        // Sleep briefly to allow other tasks to run
        Timer::after(Duration::from_millis(10)).await;
    }
}

/// Network stack runner task
#[embassy_executor::task]
async fn network_runner(
    mut runner: embassy_net::Runner<'static, esp_radio::wifi::WifiDevice<'static>>,
) -> ! {
    runner.run().await
}

// NOTE: ntp_sync_task and poll_instructions_task removed - functionality is in the main loop

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
