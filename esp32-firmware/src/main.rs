mod hardware;
mod instructions;
mod mesh;
mod node;
mod tasks;
mod utils;

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
use tasks::{instruction_execution_task, mesh_rx_task, mesh_tx_task, monitor_task, State};

fn main() -> Result<()> {
    esp_idf_sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    info!("ESP32 Mesh Demo Starting...");

    let peripherals = Peripherals::take().unwrap();
    let node = Arc::new(MeshNode::new(peripherals)?);

    info!("Initializing WiFi...");
    init_wifi()?;

    info!("Initializing Mesh...");
    init_mesh()?;

    info!("Starting mesh tasks...");

    let state: Arc<Mutex<State>> = Arc::new(Mutex::new(State {
        instructions: Instructions::new(),
    }));

    let node_rx = Arc::clone(&node);
    let state_clone = state.clone();
    thread::spawn(move || {
        mesh_rx_task(node_rx, state_clone);
    });

    let node_tx = Arc::clone(&node);
    let state_clone = state.clone();
    thread::spawn(move || {
        mesh_tx_task(node_tx, state_clone);
    });

    let node_monitor = Arc::clone(&node);
    thread::spawn(move || {
        monitor_task(node_monitor);
    });

    let node_execution = Arc::clone(&node);
    let state_execution = state.clone();
    thread::spawn(move || {
        instruction_execution_task(node_execution, state_execution);
    });

    info!("Mesh node started. Waiting for connections...");
    info!("WS2812 (GPIO18): Real addressable RGB LED with precise RMT timing!");
    info!("Status colors: Off=disconnected, Blue=child node, Green=root node");
    info!("Root node will send synchronized color updates every second");

    loop {
        thread::sleep(Duration::from_secs(1));
    }
}
