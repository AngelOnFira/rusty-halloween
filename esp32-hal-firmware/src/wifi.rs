/// WiFi connection management for ESP32-S2
///
/// This module handles WiFi initialization, connection, and provides
/// the embassy-net stack for network operations.

use defmt::{info, error};
use embassy_net::{Config as NetConfig, Stack, StackResources, Runner};
use esp_radio::wifi::{WifiController, WifiDevice, ClientConfig, ModeConfig};
use static_cell::StaticCell;
use alloc::string::String;

extern crate alloc;

/// WiFi credentials - hardcoded for now
/// TODO: Change these to your network credentials
pub const WIFI_SSID: &str = "YourSSID";
pub const WIFI_PASSWORD: &str = "YourPassword";

/// Connect to WiFi and initialize network stack
///
/// Returns (Stack, Runner) which need to be managed by the caller
pub fn connect_and_init_network<'a>(
    controller: &mut WifiController<'a>,
    sta_device: WifiDevice<'a>,
) -> Result<(Stack<'a>, Runner<'a, WifiDevice<'a>>), &'static str> {
    info!("Connecting to WiFi SSID: {}", WIFI_SSID);

    // Configure WiFi client (STA) mode
    let client_config = ClientConfig::default()
        .with_ssid(String::from(WIFI_SSID))
        .with_password(String::from(WIFI_PASSWORD));

    controller.set_config(&ModeConfig::Client(client_config))
        .map_err(|_| "Failed to set WiFi configuration")?;

    controller.start()
        .map_err(|_| "Failed to start WiFi")?;

    controller.connect()
        .map_err(|_| "Failed to initiate WiFi connection")?;

    info!("WiFi connection initiated");

    // Initialize embassy-net stack with DHCP
    let net_config = NetConfig::dhcpv4(Default::default());
    let seed = 0x1234_5678_u64; // TODO: Use hardware RNG

    static NET_RESOURCES: StaticCell<StackResources<3>> = StaticCell::new();
    let net_resources = NET_RESOURCES.init(StackResources::new());

    let (net_stack, net_runner) = embassy_net::new(
        sta_device,
        net_config,
        net_resources,
        seed,
    );

    info!("Network stack initialized with DHCP");
    Ok((net_stack, net_runner))
}

/// Wait for the network stack to get an IP address via DHCP
pub async fn wait_for_ip(stack: &Stack<'_>) -> Result<(), &'static str> {
    info!("Waiting for DHCP to assign IP address...");

    stack.wait_config_up().await;

    if let Some(config) = stack.config_v4() {
        info!("Got IP address: {}", config.address);
        Ok(())
    } else {
        error!("Failed to get IP address");
        Err("No IP address assigned")
    }
}
