/// WiFi connection management for ESP32-S2
///
/// This module handles WiFi initialization, connection, and provides
/// the embassy-net stack for network operations.

use defmt::{info, error, warn};
use embassy_net::{Config as NetConfig, Stack, StackResources, Runner};
use embassy_time::{Duration, Timer};
use esp_radio::wifi::{WifiController, WifiDevice, ClientConfig, ModeConfig, Interfaces};
use static_cell::StaticCell;
use alloc::string::String;

extern crate alloc;

/// WiFi credentials - hardcoded for now
/// TODO: Change these to your network credentials
pub const WIFI_SSID: &str = "YourSSID";
pub const WIFI_PASSWORD: &str = "YourPassword";

/// WiFi connection retry configuration
pub const WIFI_CONNECT_RETRY_COUNT: u32 = 5;
pub const WIFI_CONNECT_RETRY_DELAY_MS: u64 = 2000;

/// Connect to WiFi and initialize network stack
///
/// Returns (Stack, Runner) which need to be managed by the caller
pub fn connect_and_init_network<'a>(
    controller: &mut WifiController<'a>,
    interfaces: Interfaces<'a>,
) -> Result<(Stack<'a>, Runner<'a, WifiDevice<'a>>), &'static str> {
    // Extract the STA device from interfaces
    let sta_device = interfaces.sta;
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

/// Wait for the network stack to get an IP address via DHCP with retries
pub async fn wait_for_ip(stack: &Stack<'_>) -> Result<(), &'static str> {
    info!("Waiting for DHCP to assign IP address...");

    // Wait for stack to be configured
    stack.wait_config_up().await;

    // Retry checking for IP address
    for attempt in 1..=WIFI_CONNECT_RETRY_COUNT {
        if let Some(config) = stack.config_v4() {
            info!("Got IP address: {} (attempt {})", config.address, attempt);
            return Ok(());
        }

        if attempt < WIFI_CONNECT_RETRY_COUNT {
            warn!("No IP address yet, retrying in {}ms (attempt {}/{})",
                WIFI_CONNECT_RETRY_DELAY_MS, attempt, WIFI_CONNECT_RETRY_COUNT);
            Timer::after(Duration::from_millis(WIFI_CONNECT_RETRY_DELAY_MS)).await;
        }
    }

    error!("Failed to get IP address after {} attempts", WIFI_CONNECT_RETRY_COUNT);
    Err("No IP address assigned after retries")
}

/// Retry WiFi connection if it fails
pub async fn connect_with_retry(
    controller: &mut WifiController<'_>,
) -> Result<(), &'static str> {
    for attempt in 1..=WIFI_CONNECT_RETRY_COUNT {
        info!("Attempting WiFi connection (attempt {}/{})", attempt, WIFI_CONNECT_RETRY_COUNT);

        match controller.connect() {
            Ok(_) => {
                info!("WiFi connection initiated successfully");
                return Ok(());
            }
            Err(e) => {
                warn!("WiFi connection failed: {:?}", e);

                if attempt < WIFI_CONNECT_RETRY_COUNT {
                    warn!("Retrying in {}ms...", WIFI_CONNECT_RETRY_DELAY_MS);
                    Timer::after(Duration::from_millis(WIFI_CONNECT_RETRY_DELAY_MS)).await;
                }
            }
        }
    }

    error!("WiFi connection failed after {} attempts", WIFI_CONNECT_RETRY_COUNT);
    Err("Failed to connect to WiFi after retries")
}
