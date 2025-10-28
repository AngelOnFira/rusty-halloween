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

// Embassy net imports
use embassy_net::{Config as NetConfig, Stack, StackResources};

extern crate alloc;

use esp32s2_firmware::espnow::{add_broadcast_peer, send_packet, ReceivedPacket};
use esp32s2_firmware::protocol::{MeshPacket, PacketType, RoutingMessage, TimeSyncBeacon, OtaStartData, OtaChunkData};
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

// Runtime state for root status (determined dynamically at boot)
use embassy_sync::mutex::Mutex as AsyncMutex;
static IS_ROOT_STATE: AsyncMutex<CriticalSectionRawMutex, bool> = AsyncMutex::new(false);

// Static signal for root status
static IS_ROOT_SIGNAL: StaticCell<Signal<CriticalSectionRawMutex, bool>> = StaticCell::new();

// Static signal for IP address status (root node internet connectivity)
static HAS_IP_SIGNAL: StaticCell<Signal<CriticalSectionRawMutex, bool>> = StaticCell::new();

// Static ESP-NOW instances
static ESP_NOW_RX: StaticCell<EspNow> = StaticCell::new();

// Static network stack resources (for root node WiFi STA)
static NET_RESOURCES: StaticCell<StackResources<3>> = StaticCell::new();
static mut SAVED_NET_STACK: Option<&'static Stack<'static>> = None;

// Configuration
const NUM_LEDS: usize = 1;
const MESH_DISCOVERY_TIMEOUT_MS: u64 = 5000; // Wait 5 seconds for mesh discovery

// WiFi Station Configuration (for root node only)
const ROUTER_SSID: &str = "YourSSID"; // TODO: Configure your router SSID
const ROUTER_PASSWORD: &str = "YourPassword"; // TODO: Configure your router password

// OTA Firmware Download Configuration (for root node only)
const FIRMWARE_URL: &str = "http://192.168.1.100:8000/firmware.bin"; // TODO: Configure firmware download URL
const FIRMWARE_VERSION: &str = "0.1.0"; // Expected firmware version
const OTA_CHECK_INTERVAL_SECS: u64 = 3600; // Check for updates every hour

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

/// Attempt to discover an existing mesh network
/// Returns true if mesh beacons were detected, false otherwise
async fn discover_mesh(
    rx_channel: &Channel<CriticalSectionRawMutex, ReceivedPacket, 8>,
) -> bool {
    info!("🔍 Scanning for existing mesh network...");

    let start_time = embassy_time::Instant::now();
    let timeout = Duration::from_millis(MESH_DISCOVERY_TIMEOUT_MS);

    loop {
        // Try to receive packets with timeout
        match embassy_futures::select::select(
            rx_channel.receive(),
            Timer::after(Duration::from_millis(100)),
        )
        .await
        {
            embassy_futures::select::Either::First(received) => {
                // Check if it's a routing beacon
                if received.packet.packet_type == PacketType::Routing {
                    if let Ok(message) = postcard::from_bytes::<RoutingMessage>(&received.packet.payload) {
                        match message {
                            RoutingMessage::NeighborBeacon { distance_from_root } |
                            RoutingMessage::RouteUpdate { distance_from_root } => {
                                if distance_from_root < 255 {
                                    info!("✅ Mesh discovered! Distance from root: {}", distance_from_root);
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
            embassy_futures::select::Either::Second(_) => {
                // Timeout on receive, check if overall timeout expired
                if embassy_time::Instant::now().duration_since(start_time) >= timeout {
                    info!("⏱️  No mesh found after {}ms", MESH_DISCOVERY_TIMEOUT_MS);
                    return false;
                }
            }
        }
    }
}


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

    // Initialize channels early for mesh discovery
    let rx_channel = RX_CHANNEL.init(Channel::new());
    let tx_channel = TX_CHANNEL.init(Channel::new());
    let forward_channel = FORWARD_CHANNEL.init(Channel::new());
    let is_root_signal = IS_ROOT_SIGNAL.init(Signal::new());
    let _has_ip_signal = HAS_IP_SIGNAL.init(Signal::new());

    // Initialize radio (make it 'static)
    let radio_init = &*mk_static!(
        esp_radio::Controller<'static>,
        esp_radio::init().expect("Failed to initialize radio controller")
    );
    info!("Radio controller initialized");

    // Initialize WiFi (required for ESP-NOW)
    let (mut wifi_controller, interfaces) =
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

    // Split interfaces to use esp_now and sta separately
    let esp_radio::wifi::Interfaces { esp_now: espnow_iface, sta: sta_iface, .. } = interfaces;

    // Initialize ESP-NOW
    let esp_now_rx = ESP_NOW_RX.init(espnow_iface);

    // Add broadcast peer
    add_broadcast_peer(esp_now_rx)
        .expect("Failed to add broadcast peer");

    info!("ESP-NOW initialized with broadcast peer");

    // Start ESP-NOW handler early to receive beacons
    spawner.spawn(espnow_handler(esp_now_rx, rx_channel, tx_channel))
        .expect("Failed to spawn ESP-NOW handler");

    // === DYNAMIC ROOT ELECTION ===
    // Try to discover existing mesh network
    let mesh_found = discover_mesh(rx_channel).await;

    let is_root = if mesh_found {
        // Join existing mesh as a child node
        info!("📡 Joining existing mesh network as child");
        *IS_ROOT_STATE.lock().await = false;
        false
    } else {
        // No mesh found, try to become root by connecting to WiFi
        info!("🌐 No mesh found, attempting to become root node");
        use esp_radio::wifi::{ClientConfig, ModeConfig};
        use alloc::string::String;

        // Configure WiFi client (STA) mode
        let client_config = ClientConfig::default()
            .with_ssid(String::from(ROUTER_SSID))
            .with_password(String::from(ROUTER_PASSWORD));

        match wifi_controller.set_config(&ModeConfig::Client(client_config)) {
            Ok(_) => {
                match wifi_controller.start() {
                    Ok(_) => {
                        match wifi_controller.connect() {
                            Ok(_) => {
                                info!("✅ WiFi connection initiated");

                                // Initialize embassy-net stack with DHCP
                                let net_config = NetConfig::dhcpv4(Default::default());
                                let seed = 0x1234_5678_u64; // TODO: Use hardware RNG

                                let net_resources = NET_RESOURCES.init(StackResources::new());
                                let (net_stack, net_runner) = embassy_net::new(
                                    sta_iface,
                                    net_config,
                                    net_resources,
                                    seed,
                                );

                                // Make stack static for tasks
                                let net_stack = &*mk_static!(Stack<'static>, net_stack);
                                let net_runner = &mut *mk_static!(
                                    embassy_net::Runner<'static, esp_radio::wifi::WifiDevice<'static>>,
                                    net_runner
                                );

                                // Spawn network stack runner task
                                spawner.spawn(net_task_runner(net_runner))
                                    .expect("Failed to spawn net task runner");

                                // Spawn IP monitor task
                                spawner.spawn(net_monitor(net_stack))
                                    .expect("Failed to spawn net monitor");

                                info!("✅ Network stack initialized with DHCP");

                                unsafe { SAVED_NET_STACK = Some(net_stack); }
                                *IS_ROOT_STATE.lock().await = true;
                                true
                            }
                            Err(e) => {
                                error!("Failed to connect to WiFi: {:?}", e);
                                *IS_ROOT_STATE.lock().await = false;
                                false
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to start WiFi: {:?}", e);
                        *IS_ROOT_STATE.lock().await = false;
                        false
                    }
                }
            }
            Err(e) => {
                error!("Failed to set WiFi config: {:?}", e);
                *IS_ROOT_STATE.lock().await = false;
                false
            }
        }
    };

    // WiFi and network stack already initialized in try_wifi_connection if is_root==true

    // Initialize subsystems
    esp32s2_firmware::time_sync::init_time_sync(is_root).await;
    esp32s2_firmware::routing::init_routing(mac_address, is_root).await;
    esp32s2_firmware::led::init_led_controller(NUM_LEDS).await;
    esp32s2_firmware::instructions::init_instructions().await;
    esp32s2_firmware::ota::init_ota_manager().await;

    info!("All subsystems initialized (is_root={})", is_root);

    // Channels already initialized earlier for mesh discovery
    // ESP-NOW handler already spawned earlier

    // Spawn all tasks
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

    // Spawn OTA download task (root node only)
    if is_root {
        let net_stack = unsafe { SAVED_NET_STACK.expect("Network stack not initialized") };
        spawner.spawn(ota_download_task(net_stack, tx_channel))
            .expect("Failed to spawn OTA download task");
        info!("OTA download task spawned");
    }

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

/// Network stack runner task - runs the embassy-net stack
#[embassy_executor::task]
async fn net_task_runner(runner: &'static mut embassy_net::Runner<'static, esp_radio::wifi::WifiDevice<'static>>) {
    info!("Network stack runner started");
    runner.run().await
}

/// Network monitor task - monitors IP address and signals when available
#[embassy_executor::task]
async fn net_monitor(stack: &'static Stack<'static>) {
    info!("Network monitor started - waiting for IP address");

    loop {
        // Wait for IP configuration
        stack.wait_config_up().await;

        // Get IP address
        if let Some(config) = stack.config_v4() {
            info!("✅ Got IP address: {:?}", config.address.address());

            // Signal that we have internet connectivity
            // Note: We can't easily access HAS_IP_SIGNAL from here since it's in a StaticCell
            // For now, just log that we have an IP
            // TODO: Use a different synchronization mechanism if needed
        }

        // Wait for link down
        stack.wait_config_down().await;
        warn!("⚠️  Lost IP address");
    }
}

/// OTA download task - periodically checks for firmware updates (root node only)
#[embassy_executor::task]
async fn ota_download_task(
    stack: &'static Stack<'static>,
    tx_channel: &'static Channel<CriticalSectionRawMutex, MeshPacket, 8>,
) {
    use esp32s2_firmware::ota::{download_firmware, DownloadConfig};
    use esp32s2_firmware::protocol::{OtaStartData, OtaChunkData};

    info!("OTA download task started - waiting for internet connectivity");

    loop {
        // Wait for network to be available
        stack.wait_config_up().await;

        info!("🌐 Network available, checking for firmware updates...");

        // Download firmware
        let download_config = DownloadConfig {
            url: FIRMWARE_URL,
            version: FIRMWARE_VERSION,
        };

        match download_firmware(stack, &download_config).await {
            Ok(firmware_data) => {
                info!("✅ Firmware downloaded: {} bytes", firmware_data.len());

                // Fragment firmware into chunks for mesh distribution
                let chunk_size = 200; // Match OTA chunk data size
                let total_chunks = (firmware_data.len() + chunk_size - 1) / chunk_size;

                info!("Fragmenting firmware into {} chunks", total_chunks);

                // Send OTA start packet
                let start_data = OtaStartData {
                    version: heapless::String::try_from(FIRMWARE_VERSION)
                        .unwrap_or_default(),
                    total_chunks: total_chunks as u32,
                    firmware_size: firmware_data.len() as u32,
                };

                if let Ok(payload) = postcard::to_vec::<_, 236>(&start_data) {
                    let mut start_packet = MeshPacket::new(
                        PacketType::OtaStart,
                        0, // Timestamp (will be set by time sync)
                        [0xFF; 6], // Broadcast
                    );
                    start_packet.payload = payload;
                    tx_channel.send(start_packet).await;
                    info!("Sent OTA start packet");
                } else {
                    error!("Failed to serialize OTA start data");
                    continue;
                }

                // Wait a bit for nodes to prepare
                Timer::after(Duration::from_secs(2)).await;

                // Send chunks
                for (chunk_idx, chunk) in firmware_data.chunks(chunk_size).enumerate() {
                    let mut chunk_data_buf = [0u8; 200];
                    let len = chunk.len().min(200);
                    chunk_data_buf[..len].copy_from_slice(&chunk[..len]);

                    let chunk_data = OtaChunkData {
                        sequence: chunk_idx as u32,
                        total_chunks: total_chunks as u32,
                        version: heapless::String::try_from(FIRMWARE_VERSION)
                            .unwrap_or_default(),
                        data: heapless::Vec::from_slice(&chunk_data_buf[..len])
                            .unwrap_or_default(),
                        crc32: crc::Crc::<u32>::new(&crc::CRC_32_ISCSI)
                            .checksum(&chunk_data_buf[..len]),
                    };

                    if let Ok(payload) = postcard::to_vec::<_, 236>(&chunk_data) {
                        let mut chunk_packet = MeshPacket::new(
                            PacketType::OtaChunk,
                            0, // Timestamp
                            [0xFF; 6], // Broadcast
                        );
                        chunk_packet.payload = payload;
                        tx_channel.send(chunk_packet).await;

                        if (chunk_idx + 1) % 10 == 0 {
                            info!("Sent chunk {}/{}", chunk_idx + 1, total_chunks);
                        }

                        // Small delay between chunks to avoid overwhelming the network
                        Timer::after(Duration::from_millis(50)).await;
                    } else {
                        error!("Failed to serialize chunk {}", chunk_idx);
                    }
                }

                info!("✅ All chunks sent, waiting before sending reboot command");
                Timer::after(Duration::from_secs(5)).await;

                // Send reboot command
                let reboot_packet = MeshPacket::new(
                    PacketType::OtaReboot,
                    0, // Timestamp
                    [0xFF; 6], // Broadcast
                );
                tx_channel.send(reboot_packet).await;
                info!("Sent OTA reboot command");
            }
            Err(_) => {
                error!("Failed to download firmware");
            }
        }

        // Wait before next check
        info!("Waiting {} seconds before next update check", OTA_CHECK_INTERVAL_SECS);
        Timer::after(Duration::from_secs(OTA_CHECK_INTERVAL_SECS)).await;
    }
}
