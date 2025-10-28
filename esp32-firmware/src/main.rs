mod hardware;
mod instructions;
mod mesh;
mod node;
mod ota;
mod scan;
mod tasks;
mod utils;
mod version;

use anyhow::Result;
use esp_idf_hal::peripherals::Peripherals;
use log::*;
use std::{
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use instructions::Instructions;
use mesh::{init_mesh, init_wifi};
use node::MeshNode;
use ota::OtaManager;
use tasks::{
    instruction_execution_task, mesh_rx_task, mesh_tx_task, monitor_task,
    ota_distribution_task, State,
};
use version::FIRMWARE_VERSION;

fn main() -> Result<()> {
    esp_idf_sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    info!("╔══════════════════════════════════════════════════════╗");
    info!("║  ESP32 Mesh Firmware                                 ║");
    info!("║  Version: {:<43} ║", FIRMWARE_VERSION);
    info!("║  Built:   {:<43} ║", version::BUILD_TIMESTAMP);
    info!("╚══════════════════════════════════════════════════════╝");

    // Initialize OTA manager
    // Note: mark_app_valid() is NOT called on startup - it's only called
    // after completing an OTA update in finalize_ota()
    let ota_manager = OtaManager::new()?;

    let peripherals = Peripherals::take().unwrap();
    let node = Arc::new(MeshNode::new(peripherals)?);

    info!("Initializing WiFi...");
    init_wifi()?;

    info!("Initializing Mesh...");
    init_mesh()?;

    info!("Starting mesh tasks...");

    let state: Arc<Mutex<State>> = Arc::new(Mutex::new(State {
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
        .stack_size(32 * 1024) // 32KB stack for HTTPS operations
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

    info!("Mesh node started. Waiting for connections...");
    info!("WS2812 (GPIO18): Real addressable RGB LED with precise RMT timing!");
    info!("Status colors: Off=disconnected, Blue=child node, Green=root node");
    info!("Root node will send synchronized color updates every second");
    info!("Firmware version: v{}", FIRMWARE_VERSION);
    info!("OTA updates: Ready");

    loop {
        thread::sleep(Duration::from_secs(1));
    }
}
