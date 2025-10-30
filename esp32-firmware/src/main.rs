#[macro_use]
mod logging;
mod diagnostics;

mod hardware;
mod instructions;
mod node;
mod state;
mod tasks;
mod utils;
mod version;

use anyhow::Result;
use esp_idf_hal::peripherals::Peripherals;
use instructions::Instructions;
use node::MeshNode;
use state::{
    load_channel_from_nvs, save_channel_to_nvs, scan_with_retry, InitialState, MeshConfig,
    NetworkDiscovery, OtaManager, MESH_ID,
};
use std::{
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};
use tasks::{
    instruction_execution_task, mesh_rx_task, mesh_tx_task, monitor_task, ota_distribution_task,
    ApplicationState,
};
use utils::get_embedded_env_value;
use version::FIRMWARE_VERSION;

fn main() -> Result<()> {
    // Memory tracking: Initial state
    diagnostics::print_memory_stats("STARTUP");
    let mem_after_startup = diagnostics::get_free_heap();

    esp_idf_sys::link_patches();

    diagnostics::print_memory_delta("After ESP IDF Sys Link Patches", mem_after_startup);

    let mem_after_esp_idf_sys_link_patches = diagnostics::get_free_heap();

    esp_idf_svc::log::EspLogger::initialize_default();

    diagnostics::print_memory_delta(
        "After ESP IDF Svc Log Esp Logger Initialize Default",
        mem_after_esp_idf_sys_link_patches,
    );

    info!("╔══════════════════════════════════════════════════════╗");
    info!("║  ESP32 Mesh Firmware                                 ║");
    info!(
        "║  Version: {}                              ║",
        FIRMWARE_VERSION
    );
    info!(
        "║  Built:   {}                              ║",
        version::BUILD_TIMESTAMP
    );
    info!("╚══════════════════════════════════════════════════════╝");

    let mem_after_startup = diagnostics::get_free_heap();

    // Initialize OTA manager
    // Note: mark_app_valid() is NOT called on startup - it's only called
    // after completing an OTA update in finalize_ota()
    let ota_manager = OtaManager::new()?;
    diagnostics::print_memory_delta("After OTA Manager Init", mem_after_startup);

    let peripherals = Peripherals::take().unwrap();
    let node = Arc::new(MeshNode::new(peripherals)?);
    let mem_before_wifi = diagnostics::get_free_heap();

    // Initialize state machine (this also initializes WiFi)
    info!("main: Initializing state machine and WiFi...");
    let wifi_state = InitialState::new();
    let wifi_state = wifi_state.initialize_wifi()?;
    diagnostics::print_memory_delta("After WiFi Init", mem_before_wifi);

    let mem_before_mesh = diagnostics::get_free_heap();
    info!("main: Initializing Mesh...");
    // Get router credentials
    let router_ssid = get_embedded_env_value("ROUTER_SSID");
    let router_pass = get_embedded_env_value("ROUTER_PASSWORD");
    info!(
        "main: Router SSID: {}, Password length: {}",
        router_ssid,
        router_pass.len()
    );

    // Try to load channel from NVS (persisted from previous boot). For now,
    // load it as none so that we can make sure we just join the best network.
    // let mut mesh_channel: Option<u8> = load_channel_from_nvs();
    let mut mesh_channel: Option<u8> = None;
    let mut router_bssid: Option<[u8; 6]> = None;

    // If no saved channel, scan for networks using state machine
    if mesh_channel.is_none() {
        info!("main: No saved channel found, scanning for networks using state machine...");

        // Use the old scan_with_retry for now since it handles the discovery logic
        // TODO: Eventually migrate this to use state machine's scan directly
        let discovery = scan_with_retry(&router_ssid, &MESH_ID, 30_000);

        match discovery {
            NetworkDiscovery::ExistingMesh {
                channel,
                ssid,
                bssid,
                rssi,
            } => {
                info!(
                    "main: NetworkDiscovery::ExistingMesh - Discovered existing mesh network: '{}' on channel {}, RSSI: {}, BSSID: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                    ssid, channel, rssi,
                    bssid[0], bssid[1], bssid[2], bssid[3], bssid[4], bssid[5]
                );
                mesh_channel = Some(channel);
                router_bssid = Some(bssid);
            }
            NetworkDiscovery::Router {
                channel,
                bssid,
                rssi,
            } => {
                info!(
                    "main: NetworkDiscovery::Router - Discovered router '{}' on channel {}, RSSI: {}, BSSID: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                    router_ssid, channel, rssi,
                    bssid[0], bssid[1], bssid[2], bssid[3], bssid[4], bssid[5]
                );
                mesh_channel = Some(channel);
                router_bssid = Some(bssid);
            }
            NetworkDiscovery::NotFound => {
                return Err(anyhow::anyhow!("main: NetworkDiscovery::NotFound - Network scan failed - no mesh or router found"));
            }
        }
    } else {
        info!(
            "main: Using saved channel from NVS: {}",
            mesh_channel.unwrap()
        );
    }

    let channel = mesh_channel.expect("Channel must be determined by this point");

    // Start mesh using state machine
    info!(
        "main: Starting mesh on channel {} using state machine...",
        channel
    );
    let mesh_config = MeshConfig {
        mesh_id: MESH_ID,
        channel,
        router_ssid: router_ssid.clone(),
        router_password: router_pass.clone(),
        router_bssid,
        allow_router_switch: false, // Prevent fallback to other APs with same SSID if specific BSSID is unavailable
        max_connections: 10,       // Using default from mesh.rs MESH_AP_CONNECTIONS
    };

    // Additional mesh settings (still using direct calls for now)
    unsafe {
        use esp_idf_sys::{
            esp, esp_mesh_set_ap_authmode, esp_mesh_set_max_layer, esp_mesh_set_vote_percentage,
        };
        esp!(esp_mesh_set_max_layer(25))?; // MESH_MAX_LAYER
        esp!(esp_mesh_set_vote_percentage(1.0))?;

        let auth_mode = esp_idf_sys::wifi_auth_mode_t_WIFI_AUTH_OPEN;
        esp!(esp_mesh_set_ap_authmode(auth_mode))?;
        info!("main: Mesh configuration completed");
    }

    let _mesh_state = wifi_state.start_mesh(mesh_config)?;
    info!("main: Mesh started successfully via state machine");
    diagnostics::print_memory_delta("After Mesh Init", mem_before_mesh);

    // Save channel to NVS for faster boot next time
    save_channel_to_nvs(channel);

    let mem_before_tasks = diagnostics::get_free_heap();
    info!("main: Starting mesh tasks...");

    let state: Arc<Mutex<ApplicationState>> = Arc::new(Mutex::new(ApplicationState {
        instructions: Instructions::new(),
        ota_manager: Arc::new(Mutex::new(ota_manager)),
    }));

    let node_rx = Arc::clone(&node);
    let state_clone = state.clone();
    thread::spawn(move || {
        mesh_rx_task(node_rx, state_clone);
    });

    let node_tx = Arc::clone(&node);
    let state_clone = state.clone();
    // mesh_tx_task needs larger stack for HTTPS/TLS operations (GitHub API calls)
    thread::Builder::new()
        .stack_size(12 * 1024) // 32KB stack for HTTPS operations
        .spawn(move || {
            mesh_tx_task(node_tx, state_clone);
        })
        .expect("Failed to spawn mesh_tx_task");

    let node_monitor = Arc::clone(&node);
    thread::spawn(move || {
        monitor_task(node_monitor);
    });

    let node_execution = Arc::clone(&node);
    let state_execution = state.clone();
    thread::spawn(move || {
        instruction_execution_task(node_execution, state_execution);
    });

    let node_ota = Arc::clone(&node);
    let state_ota = state.clone();
    thread::spawn(move || {
        ota_distribution_task(node_ota, state_ota);
    });

    diagnostics::print_memory_delta("After Task Spawning", mem_before_tasks);

    // Final memory summary
    info!("════════════════════════════════════════════════════");
    diagnostics::print_memory_stats("READY - All Systems Initialized");
    diagnostics::print_heap_watermark();
    info!("════════════════════════════════════════════════════");

    info!("main: Mesh node started. Waiting for connections...");
    info!("main: WS2812 (GPIO18): Real addressable RGB LED with precise RMT timing!");
    info!("main: Status colors: Off=disconnected, Blue=child node, Green=root node");
    info!("main: Root node will send synchronized color updates every second");
    info!("main: Firmware version: v{}", FIRMWARE_VERSION);
    info!("main: OTA updates: Ready");

    loop {
        thread::sleep(Duration::from_secs(1));
    }
}
