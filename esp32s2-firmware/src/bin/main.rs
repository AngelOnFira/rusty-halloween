#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use defmt::{debug, error, info, warn};
use embassy_executor::Spawner;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, signal::Signal};
use embassy_time::{Duration, Timer};
use esp_hal::clock::CpuClock;
use esp_hal::timer::timg::TimerGroup;
use esp_radio::esp_now::EspNow;
use heapless::Vec;
use static_cell::StaticCell;
use {esp_backtrace as _, esp_println as _};

extern crate alloc;

use esp32s2_firmware::espnow::{add_broadcast_peer, espnow_rx_task, send_packet, ReceivedPacket};
use esp32s2_firmware::protocol::{MeshPacket, PacketType, RoutingMessage, TimeSyncBeacon, OtaStartData, OtaChunkData, OtaAckData};
use esp32s2_firmware::instructions::{Instruction, InstructionStatus};

// Macro to create static values (from embassy examples)
macro_rules! mk_static {
    ($t:ty,$val:expr) => {{
        static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        #[deny(unused_attributes)]
        let x = STATIC_CELL.uninit().write(($val));
        x
    }};
}

// Static channels for ESP-NOW communication
static RX_CHANNEL: StaticCell<Channel<CriticalSectionRawMutex, ReceivedPacket, 8>> =
    StaticCell::new();
static TX_CHANNEL: StaticCell<Channel<CriticalSectionRawMutex, MeshPacket, 8>> =
    StaticCell::new();
static FORWARD_CHANNEL: StaticCell<Channel<CriticalSectionRawMutex, ReceivedPacket, 8>> =
    StaticCell::new();

// Static signal for root status
static IS_ROOT_SIGNAL: StaticCell<Signal<CriticalSectionRawMutex, bool>> = StaticCell::new();

// Static ESP-NOW instances
static ESP_NOW_RX: StaticCell<EspNow> = StaticCell::new();
static ESP_NOW_TX: StaticCell<EspNow> = StaticCell::new();

// Configuration
const IS_ROOT: bool = false; // Set to true for root node
const NUM_LEDS: usize = 1;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    info!("╔══════════════════════════════════════════════════════╗");
    info!("║  ESP32-S2 Mesh Firmware (no-std + ESP-NOW)          ║");
    info!("║  Version: 0.1.0                                      ║");
    info!("╚══════════════════════════════════════════════════════╝");

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(#[unsafe(link_section = ".dram2_uninit")] size: 139264);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0);

    info!("Embassy initialized!");

    // Initialize radio (make it 'static)
    let radio_init = &*mk_static!(
        esp_radio::Controller<'static>,
        esp_radio::init().expect("Failed to initialize radio controller")
    );
    info!("Radio controller initialized");

    // Initialize WiFi (required for ESP-NOW)
    let (wifi_controller, interfaces) =
        esp_radio::wifi::new(radio_init, peripherals.WIFI, Default::default())
            .expect("Failed to initialize Wi-Fi controller");

    info!("WiFi controller initialized");

    // Get MAC address
    // TODO: Read from efuse using proper API when available
    // For now, use a configurable MAC address
    let mac_address: [u8; 6] = [0xAA, 0xBB, 0xCC, 0x00, 0x00, 0x01]; // Change last byte per device
    info!(
        "MAC Address: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        mac_address[0], mac_address[1], mac_address[2],
        mac_address[3], mac_address[4], mac_address[5]
    );

    // Initialize ESP-NOW from interfaces
    let esp_now_rx = ESP_NOW_RX.init(interfaces.esp_now);

    // Add broadcast peer
    add_broadcast_peer(esp_now_rx)
        .expect("Failed to add broadcast peer");

    info!("ESP-NOW initialized with broadcast peer");

    // Initialize subsystems
    esp32s2_firmware::time_sync::init_time_sync(IS_ROOT).await;
    esp32s2_firmware::routing::init_routing(mac_address, IS_ROOT).await;
    esp32s2_firmware::led::init_led_controller(NUM_LEDS).await;
    esp32s2_firmware::instructions::init_instructions().await;
    esp32s2_firmware::ota::init_ota_manager().await;

    info!("All subsystems initialized (is_root={})", IS_ROOT);

    // Initialize channels
    let rx_channel = RX_CHANNEL.init(Channel::new());
    let tx_channel = TX_CHANNEL.init(Channel::new());
    let forward_channel = FORWARD_CHANNEL.init(Channel::new());
    let is_root_signal = IS_ROOT_SIGNAL.init(Signal::new());

    // Spawn all tasks
    spawner.spawn(espnow_handler(esp_now_rx, rx_channel, tx_channel))
        .expect("Failed to spawn ESP-NOW handler");

    spawner.spawn(packet_processor(rx_channel, forward_channel))
        .expect("Failed to spawn packet processor");

    spawner.spawn(packet_forwarder(forward_channel, tx_channel))
        .expect("Failed to spawn packet forwarder");

    spawner.spawn(time_sync_beacon(tx_channel, is_root_signal))
        .expect("Failed to spawn time sync beacon");

    spawner.spawn(routing_beacon(tx_channel, mac_address))
        .expect("Failed to spawn routing beacon");

    spawner.spawn(routing_cleanup())
        .expect("Failed to spawn routing cleanup");

    spawner.spawn(instruction_executor())
        .expect("Failed to spawn instruction executor");

    spawner.spawn(status_monitor())
        .expect("Failed to spawn status monitor");

    info!("ESP-NOW mesh node started!");
    info!("All tasks spawned successfully");

    // Main loop - just keep alive
    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}

/// Unified ESP-NOW handler - handles both RX and TX
#[embassy_executor::task]
async fn espnow_handler(
    esp_now: &'static mut EspNow<'static>,
    rx_channel: &'static Channel<CriticalSectionRawMutex, ReceivedPacket, 8>,
    tx_channel: &'static Channel<CriticalSectionRawMutex, MeshPacket, 8>,
) {
    info!("ESP-NOW unified handler started");

    loop {
        // Try to send any pending TX packets first (non-blocking)
        if let Some(packet) = tx_channel.try_receive().ok() {
            match send_packet(esp_now, &packet).await {
                Ok(_) => {
                    debug!("Packet sent successfully");
                }
                Err(e) => {
                    error!("Failed to send packet: {:?}", e);
                }
            }
        }

        // Then try to receive (with timeout to prevent blocking TX too long)
        match embassy_futures::select::select(
            espnow_rx_task_once(esp_now),
            Timer::after(Duration::from_millis(10)),
        )
        .await
        {
            embassy_futures::select::Either::First(result) => {
                match result {
                    Ok(received) => {
                        if rx_channel.try_send(received).is_err() {
                            warn!("RX channel full, dropping packet");
                        }
                    }
                    Err(e) => {
                        error!("ESP-NOW receive error: {:?}", e);
                    }
                }
            }
            embassy_futures::select::Either::Second(_) => {
                // Timeout, loop back to check TX queue
            }
        }
    }
}

/// Helper function to receive one packet from ESP-NOW
async fn espnow_rx_task_once(
    esp_now: &mut EspNow<'_>,
) -> Result<ReceivedPacket, esp32s2_firmware::espnow::EspNowError> {
    esp32s2_firmware::espnow::receive_packet(esp_now).await
}

/// Packet processor - processes received packets by type
#[embassy_executor::task]
async fn packet_processor(
    rx_channel: &'static Channel<CriticalSectionRawMutex, ReceivedPacket, 8>,
    forward_channel: &'static Channel<CriticalSectionRawMutex, ReceivedPacket, 8>,
) {
    info!("Packet processor started");

    loop {
        let received = rx_channel.receive().await;

        // Forward packet to forwarding task for mesh routing
        let _ = forward_channel.try_send(received.clone());

        // Process packet based on type
        match received.packet.packet_type {
            PacketType::TimeSync => {
                if let Ok(beacon) = postcard::from_bytes::<TimeSyncBeacon>(&received.packet.payload) {
                    esp32s2_firmware::time_sync::handle_sync_beacon(&beacon).await;
                }
            }
            PacketType::Routing => {
                if let Ok(message) = postcard::from_bytes::<RoutingMessage>(&received.packet.payload) {
                    esp32s2_firmware::routing::handle_routing_message(
                        received.sender_mac,
                        &message,
                        received.rssi,
                    ).await;
                }
            }
            PacketType::Instructions => {
                if let Ok(instructions) = postcard::from_bytes::<Vec<Instruction, 100>>(&received.packet.payload) {
                    esp32s2_firmware::instructions::combine_instructions(&instructions).await;
                    info!("Received {} instructions", instructions.len());
                }
            }
            PacketType::OtaStart => {
                if let Ok(start_data) = postcard::from_bytes::<OtaStartData>(&received.packet.payload) {
                    match esp32s2_firmware::ota::start_update(&start_data).await {
                        Ok(_) => info!("OTA update started"),
                        Err(_) => error!("Failed to start OTA update"),
                    }
                }
            }
            PacketType::OtaChunk => {
                if let Ok(chunk_data) = postcard::from_bytes::<OtaChunkData>(&received.packet.payload) {
                    match esp32s2_firmware::ota::handle_chunk(&chunk_data, received.sender_mac).await {
                        Ok(true) => {
                            // New chunk received, send ACK
                            // TODO: Send ACK packet
                        }
                        Ok(false) => {
                            // Duplicate chunk, ignore
                        }
                        Err(_) => {
                            error!("Failed to process OTA chunk");
                        }
                    }
                }
            }
            PacketType::OtaAck => {
                // ACK handling would be implemented on sender side
                // For mesh network, just forward to root
                debug!("OTA ACK received");
            }
            PacketType::OtaReboot => {
                // Check if OTA is ready to apply
                if esp32s2_firmware::ota::is_ready_to_apply().await {
                    match esp32s2_firmware::ota::apply_update().await {
                        Ok(_) => {
                            info!("OTA update applied, rebooting...");
                            // TODO: Implement reboot
                        }
                        Err(_) => error!("Failed to apply OTA update"),
                    }
                } else {
                    warn!("Received OTA reboot but update not ready");
                }
            }
        }
    }
}

/// Packet forwarder - forwards packets for multi-hop routing
#[embassy_executor::task]
async fn packet_forwarder(
    forward_channel: &'static Channel<CriticalSectionRawMutex, ReceivedPacket, 8>,
    tx_channel: &'static Channel<CriticalSectionRawMutex, MeshPacket, 8>,
) {
    info!("Packet forwarder started");
    loop {
        let received = forward_channel.receive().await;
        if esp32s2_firmware::routing::should_forward(&received.packet).await {
            let mut forwarded = received.packet.clone();
            forwarded.increment_hop();
            tx_channel.send(forwarded).await;
        }
    }
}

/// Time sync beacon task
#[embassy_executor::task]
async fn time_sync_beacon(
    tx_channel: &'static Channel<CriticalSectionRawMutex, MeshPacket, 8>,
    is_root_signal: &'static Signal<CriticalSectionRawMutex, bool>,
) {
    esp32s2_firmware::time_sync::time_sync_beacon_task(tx_channel, is_root_signal).await
}

/// Routing beacon task
#[embassy_executor::task]
async fn routing_beacon(
    tx_channel: &'static Channel<CriticalSectionRawMutex, MeshPacket, 8>,
    mac_address: [u8; 6],
) {
    esp32s2_firmware::routing::neighbor_beacon_task(tx_channel, mac_address).await
}

/// Routing cleanup task
#[embassy_executor::task]
async fn routing_cleanup() {
    esp32s2_firmware::routing::neighbor_cleanup_task().await
}

/// Instruction executor task - executes LED instructions at the right time
#[embassy_executor::task]
async fn instruction_executor() {
    info!("Instruction executor started");

    loop {
        match esp32s2_firmware::instructions::get_next_instruction().await {
            InstructionStatus::Sleep(duration_us) => {
                // Sleep until next instruction
                Timer::after(Duration::from_micros(duration_us)).await;
            }
            InstructionStatus::SetColor(color) => {
                // Set LED color
                esp32s2_firmware::led::set_color(color).await;
            }
        }
    }
}

/// Status monitor task - periodically reports system status
#[embassy_executor::task]
async fn status_monitor() {
    info!("Status monitor started");

    loop {
        Timer::after(Duration::from_secs(10)).await;

        let is_synced = esp32s2_firmware::time_sync::is_synced().await;
        let distance = esp32s2_firmware::routing::our_distance().await;
        let buffer_left = esp32s2_firmware::instructions::get_buffer_left().await;

        info!(
            "Status: synced={}, distance={}, buffer={}ms",
            is_synced,
            distance,
            buffer_left / 1000
        );
    }
}
