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
use embassy_sync::channel::Channel;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use esp_backtrace as _;
use esp_hal::{clock::CpuClock, rmt::Rmt, time::Rate, timer::timg::TimerGroup};
use esp_hal_smartled::{SmartLedsAdapter, smart_led_buffer};
use esp_println as _;
use smart_leds::{RGB8, SmartLedsWrite, brightness, gamma};

extern crate alloc;

use esp32_hal_firmware::wifi;
use esp32_hal_firmware::ntp::TimeSync;
use esp32_hal_firmware::led_executor::LedExecutor;
use esp32_hal_firmware::http_client::ShowClient;

use static_cell::StaticCell;
use portable_atomic::{AtomicU64, Ordering};

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
static INSTRUCTION_CHANNEL: StaticCell<Channel<NoopRawMutex, alloc::vec::Vec<esp32_hal_firmware::esp32_types::Esp32Instruction>, 4>> = StaticCell::new();
static SHOW_START_TIME: AtomicU64 = AtomicU64::new(0);
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

    // Initialize channel for instructions
    let instruction_channel = INSTRUCTION_CHANNEL.init(Channel::new());

    info!("System initialized! Starting show controller tasks...");

    // Spawn instruction executor task (handles LED control and initialization)
    spawner.spawn(instruction_executor_task(time_sync, instruction_channel, peripherals.RMT, peripherals.GPIO4)).ok();

    // Spawn polling task
    spawner.spawn(polling_task(stack, instruction_channel)).ok();

    info!("All tasks spawned! ESP32 light controller running.");

    // Main task just sleeps - all work is done by spawned tasks
    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}

/// Instruction executor task - manages timing, execution of instructions, and LED control
///
/// Receives instructions from polling task, filters out past ones, and executes at correct timestamps
#[embassy_executor::task]
async fn instruction_executor_task(
    time_sync: &'static TimeSync,
    instruction_channel: &'static Channel<NoopRawMutex, alloc::vec::Vec<esp32_hal_firmware::esp32_types::Esp32Instruction>, 4>,
    rmt_peripheral: esp_hal::peripherals::RMT<'static>,
    gpio4_peripheral: esp_hal::peripherals::GPIO4<'static>,
) -> ! {
    info!("Instruction executor task started");

    // Initialize LED hardware inside task
    let rmt = Rmt::new(rmt_peripheral, Rate::from_mhz(80)).unwrap();
    let mut led = SmartLedsAdapter::new(rmt.channel0, gpio4_peripheral, smart_led_buffer!(NUM_LEDS));

    info!("LED hardware initialized");

    // Test LEDs on boot - show white for 2 seconds
    let white_test = alloc::vec![RGB8::new(255, 255, 255); NUM_LEDS];
    info!("Testing LEDs: setting all to white");
    led.write(brightness(gamma(white_test.iter().cloned()), 255)).ok();
    Timer::after(Duration::from_secs(2)).await;

    // Turn off LEDs after test
    let off = alloc::vec![RGB8::new(0, 0, 0); NUM_LEDS];
    led.write(off.iter().cloned()).ok();
    info!("LED test complete, starting instruction executor");

    let instruction_receiver = instruction_channel.receiver();
    let mut led_executor = LedExecutor::new(NUM_LEDS);

    loop {
        // Check for new instructions with 10ms timeout
        let new_instructions = embassy_time::with_timeout(
            Duration::from_millis(10),
            instruction_receiver.receive()
        ).await;

        if let Ok(instructions) = new_instructions {
            // Get current show time
            let show_start_time = SHOW_START_TIME.load(Ordering::Relaxed);
            let current_show_time = if show_start_time > 0 {
                time_sync.show_time_ms(show_start_time)
            } else {
                0
            };

            // Filter out past instructions - only add future ones
            let mut future_count = 0;
            for instr in instructions {
                if instr.timestamp > current_show_time {
                    led_executor.add_instruction(instr);
                    future_count += 1;
                }
            }

            if future_count > 0 {
                info!("Added {} future instructions (current show time: {}ms)",
                      future_count, current_show_time);
            }
        }

        // Execute any due instructions
        let show_start_time = SHOW_START_TIME.load(Ordering::Relaxed);
        if show_start_time > 0 {
            let current_show_time = time_sync.show_time_ms(show_start_time);

            if let Some(led_data) = led_executor.execute_due_instructions(current_show_time) {
                // Update LEDs directly - no command channel needed
                if let Some(&color) = led_data.first() {
                    info!("Executing instruction at {}ms: RGB({},{},{})",
                          current_show_time, color.r, color.g, color.b);
                }
                led.write(brightness(gamma(led_data.iter().cloned()), 255)).ok();
            }
        }

        // Sleep until next instruction or default 10ms
        Timer::after(Duration::from_millis(10)).await;
    }
}

/// Polling task - fetches instructions from server
///
/// Polls server every second for new instructions and sends them to executor task
#[embassy_executor::task]
async fn polling_task(
    stack: &'static embassy_net::Stack<'static>,
    instruction_channel: &'static Channel<NoopRawMutex, alloc::vec::Vec<esp32_hal_firmware::esp32_types::Esp32Instruction>, 4>,
) -> ! {
    info!("Polling task started");

    let instruction_sender = instruction_channel.sender();
    let http_client = ShowClient::new_with_init(DEVICE_ID);
    let mut last_poll_ms = 0u64;

    info!("HTTP client initialized, starting polling loop with TEST pattern");

    loop {
        // Use test endpoint for now to verify LED functionality
        match http_client.fetch_test_instructions(stack).await {
            Ok(response) => {
                // Update show start time from server (authoritative source)
                if response.show_start_time > 0 {
                    let current = SHOW_START_TIME.load(Ordering::Relaxed);
                    if current != response.show_start_time {
                        info!("Show start time updated: {}ms", response.show_start_time);
                        SHOW_START_TIME.store(response.show_start_time, Ordering::Relaxed);
                    }
                }

                if !response.instructions.is_empty() {
                    info!("Received {} TEST instructions from server", response.instructions.len());

                    // Send instructions to executor task
                    instruction_sender.send(response.instructions).await;
                }
            }
            Err(e) => {
                defmt::error!("Failed to fetch instructions: {:?}", e);
            }
        }

        // Poll every 1 second
        Timer::after(Duration::from_millis(POLL_INTERVAL_MS)).await;
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
