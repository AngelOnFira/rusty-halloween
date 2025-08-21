use anyhow::Result;
use esp_idf_hal::{
    gpio::Gpio18, peripheral::Peripheral, peripherals::Peripherals, rmt::RmtChannel,
};
use esp_idf_sys::{
    self as sys, esp, esp_event_base_t, esp_event_handler_register, esp_event_loop_create_default,
    esp_mesh_get_layer, esp_mesh_get_total_node_num, esp_mesh_get_tsf_time, esp_mesh_init,
    esp_mesh_is_device_active, esp_mesh_is_root, esp_mesh_recv, esp_mesh_send,
    esp_mesh_set_ap_authmode, esp_mesh_set_config, esp_mesh_set_max_layer,
    esp_mesh_set_vote_percentage, esp_mesh_start, esp_netif_init, esp_random, esp_wifi_init,
    esp_wifi_set_storage, esp_wifi_start, g_wifi_default_wpa_crypto_funcs, g_wifi_osi_funcs,
    mesh_addr_t, mesh_cfg_t, mesh_data_t, mesh_router_t, nvs_flash_init, wifi_init_config_t,
    wifi_storage_t_WIFI_STORAGE_RAM, ESP_EVENT_ANY_ID, IP_EVENT, MESH_EVENT,
    WIFI_INIT_CONFIG_MAGIC,
};
use log::*;
use serde::{Deserialize, Serialize};
use smart_leds::{SmartLedsWrite, RGB8};
use std::{
    collections::HashMap,
    os::raw::c_void,
    ptr,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use ws2812_esp32_rmt_driver::Ws2812Esp32Rmt;

const MESH_ID: [u8; 6] = [0x77, 0x77, 0x77, 0x77, 0x77, 0x77];
const MESH_PASSWORD: &str = "mesh_password_123";

// Include the entire .env file as a string at compile time
const ENV_FILE: &str = include_str!("../.env");

// Simple function to extract a value from the .env content
fn get_embedded_env_value(key: &str) -> String {
    let search_pattern = format!("{}=", key);

    for line in ENV_FILE.lines() {
        let line = line.trim();

        // Skip comments and empty lines
        if line.starts_with('#') || line.is_empty() {
            continue;
        }

        if let Some(value) = line.strip_prefix(&search_pattern) {
            // Remove surrounding quotes if present and return
            return value.trim_matches('"').trim_matches('\'').to_string();
        }
    }

    panic!("Environment variable '{}' not found in .env file. Make sure your .env file contains a line like: {}=your_value", key, key);
}

const MESH_CHANNEL: u8 = 6;
const MESH_MAX_LAYER: i32 = 6;
const MESH_AP_CONNECTIONS: i32 = 6;

struct State {
    instructions: Instructions,
}

/// A list of unique instructions sorted by timestamp
#[derive(Clone, PartialEq, Serialize, Deserialize)]
struct Instructions {
    pub instructions: Vec<Instruction>,
}

enum InstructionStatus {
    /// If we should sleep for some amount of time before the next instruction
    Sleep(i64),
    /// If we should set the color immediately
    SetColor(RGB8),
}

impl Instructions {
    fn new() -> Self {
        Self {
            instructions: Vec::new(),
        }
    }

    fn get_current_time(&self) -> i64 {
        unsafe { esp_mesh_get_tsf_time() }
    }

    /// Add an instruction to the list
    fn add_instruction(&mut self, instruction: Instruction) {
        self.instructions.push(instruction);
        self.instructions.sort_by_key(|i| i.timestamp);
    }

    /// Get the instructions between the current time and the current time + seconds
    fn get_next_seconds_instructions(&self, seconds: i64) -> Vec<Instruction> {
        let current_time = self.get_current_time();
        self.instructions
            .iter()
            .filter(|i| i.timestamp >= current_time && i.timestamp < current_time + seconds)
            .cloned()
            .collect()
    }

    /// Calculate how much of a buffer is left in the instructions list
    fn get_buffer_left(&self) -> i64 {
        let current_time = self.get_current_time();
        let next_instruction = self.instructions.first();
        if next_instruction.is_none() {
            return 0;
        }

        let next_instruction = next_instruction.unwrap();

        let buffer_left = next_instruction.timestamp - current_time;

        if buffer_left < 0 {
            return 0;
        }

        buffer_left
    }

    /// Get the next instruction, or find out if we should sleep for some amount
    /// of time. There are a few cases:
    ///
    /// - If we check too early (the instruction is more than 50ms in the
    ///   future), we should return an InstructionStatus::Sleep
    /// - If we check too late (the instruction is more than 50ms in the past),
    ///   drop that instruction, and log that we're late.
    /// - If we check at the right time (the instruction is within 50ms of the
    ///   current time), we should return an InstructionStatus::SetColor
    ///
    /// In any case where we set the colour, we should clean this instruction
    /// from the list.
    fn get_next_instruction(&mut self) -> InstructionStatus {
        // Get the next instruction
        let next_instruction = self.instructions.first();

        // If there's no next instruction, we should sleep for 100ms
        if next_instruction.is_none() {
            return InstructionStatus::Sleep(100);
        }

        // Get the next instruction
        let next_instruction = next_instruction.unwrap();

        // If the next instruction is in the past, we should drop it. We add a
        // 50ms buffer to account for the time it takes to process the
        // instruction, and this much drift shouldn't be noticeable.
        if next_instruction.timestamp < self.get_current_time() - 50 {
            self.instructions.remove(0);
            return InstructionStatus::Sleep(100);

            // TODO: Log that we're late
        }

        // If the next instruction is in the future, we should return an InstructionStatus::Sleep
        if next_instruction.timestamp > self.get_current_time() + 50 {
            return InstructionStatus::Sleep(
                next_instruction.timestamp - self.get_current_time() - 10,
            );
        }

        // If the next instruction is at the current time, we should return an InstructionStatus::SetColor
        let instruction = self.instructions.remove(0);
        return InstructionStatus::SetColor(instruction.color);
    }

    /// Combine a new list of instructions that will be buffered. A list will
    /// either come from the mesh root, or the mesh root might store the list it
    /// just generated. For this, there might be overlap with the current list,
    /// so we should only add timestamps that we don't have.
    fn combine_instructions(&mut self, instructions: Vec<Instruction>) {
        // Remove any instructions that are already in the list
        let new_instructions: Vec<Instruction> = instructions
            .iter()
            .filter(|i| !self.instructions.contains(i))
            .cloned()
            .collect();

        // Add the new instructions
        self.instructions.extend(new_instructions);

        // Sort the instructions by timestamp
        self.instructions.sort_by_key(|i| i.timestamp);
    }

    /// Generate a random list of new instructions for the next number of
    /// seconds passed in. This should only be called by the root node.
    fn generate_random_instructions(&mut self, seconds: i64) {
        let mut instructions = Vec::new();

        // Start with the current time
        let mut current_time = self.get_current_time();

        while current_time < self.get_current_time() + seconds {
            // Generate a random color
            let color = unsafe {
                RGB8::new(
                    (esp_random() % 256) as u8,
                    (esp_random() % 256) as u8,
                    (esp_random() % 256) as u8,
                )
            };

            // Add the instruction to the list
            instructions.push(Instruction {
                timestamp: current_time,
                color,
            });

            // Add a random delay between 100ms and 1000ms
            unsafe {
                current_time += (esp_random() % 1000 + 200) as i64;
            }
        }

        // Add the instructions to the list
        self.combine_instructions(instructions);
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
struct Instruction {
    timestamp: i64,
    color: RGB8,
}

// WS2812 controller using ESP32 RMT peripheral
struct WS2812Controller {
    driver: Arc<Mutex<Ws2812Esp32Rmt<'static>>>,
}

impl WS2812Controller {
    fn new<C>(channel: impl Peripheral<P = C> + 'static, pin: Gpio18) -> Result<Self>
    where
        C: RmtChannel,
    {
        let driver = Ws2812Esp32Rmt::new(channel, pin)?;

        Ok(Self {
            driver: Arc::new(Mutex::new(driver)),
        })
    }

    fn set_color(&self, color: RGB8) -> Result<()> {
        info!(
            "Setting WS2812 LED - RGB({}, {}, {})",
            color.r, color.g, color.b
        );

        if let Ok(mut driver) = self.driver.lock() {
            // Create array of one LED pixel
            let pixels = [color];

            // Write to the WS2812 LED using RMT peripheral
            driver.write(pixels.iter().cloned())?;
            info!("WS2812 color sent successfully");
        }

        Ok(())
    }
}

struct MeshNode {
    led: WS2812Controller,
    is_root: Arc<Mutex<bool>>,
    is_connected: Arc<Mutex<bool>>,
    layer: Arc<Mutex<i32>>,
    current_color: Arc<Mutex<RGB8>>,
    // Packet loss testing
    pending_challenges: Arc<Mutex<HashMap<u32, Instant>>>,
    total_challenges_sent: Arc<Mutex<u32>>,
    total_responses_received: Arc<Mutex<u32>>,
}

impl MeshNode {
    fn new(peripherals: Peripherals) -> Result<Self> {
        // Initialize WS2812 controller on GPIO18
        let led = WS2812Controller::new(peripherals.rmt.channel0, peripherals.pins.gpio18)?;

        Ok(Self {
            led,
            is_root: Arc::new(Mutex::new(false)),
            is_connected: Arc::new(Mutex::new(false)),
            layer: Arc::new(Mutex::new(-1)),
            current_color: Arc::new(Mutex::new(RGB8::new(0, 0, 0))),
            pending_challenges: Arc::new(Mutex::new(HashMap::new())),
            total_challenges_sent: Arc::new(Mutex::new(0)),
            total_responses_received: Arc::new(Mutex::new(0)),
        })
    }

    fn update_status_color(&self) {
        let is_connected = *self.is_connected.lock().unwrap();
        let is_root = *self.is_root.lock().unwrap();

        // Use different colors to indicate status
        let status_color = match (is_connected, is_root) {
            (false, _) => RGB8::new(0, 0, 0),     // Off when not connected
            (true, true) => RGB8::new(0, 10, 0),  // Green very dim when root and connected
            (true, false) => RGB8::new(0, 0, 10), // Blue very dim when child and connected
        };

        let _ = self.led.set_color(status_color);
    }

    fn set_color(&self, r: u8, g: u8, b: u8) {
        let color = RGB8::new(r, g, b);
        *self.current_color.lock().unwrap() = color;

        let _ = self.led.set_color(color);

        info!("Set WS2812 color to RGB({}, {}, {})", r, g, b);
    }

    fn send_challenge(&self, challenge_id: u32) -> bool {
        let challenge_command = serde_json::json!({
            "type": "challenge",
            "id": challenge_id
        });

        let message = challenge_command.to_string();
        let broadcast_addr = mesh_addr_t { addr: [0xFF; 6] };

        let mesh_data = mesh_data_t {
            data: message.as_ptr() as *mut u8,
            size: message.len() as u16,
            proto: 0,
            tos: 0,
        };

        let flag = 0x01; // MESH_DATA_GROUP flag

        unsafe {
            let err = esp_mesh_send(&broadcast_addr, &mesh_data, flag, ptr::null(), 0);
            if err == sys::ESP_OK {
                // Record the challenge
                self.pending_challenges
                    .lock()
                    .unwrap()
                    .insert(challenge_id, Instant::now());
                *self.total_challenges_sent.lock().unwrap() += 1;
                true
            } else {
                false
            }
        }
    }

    fn handle_challenge_response(&self, challenge_id: u32) {
        if self
            .pending_challenges
            .lock()
            .unwrap()
            .remove(&challenge_id)
            .is_some()
        {
            *self.total_responses_received.lock().unwrap() += 1;
        }
    }

    fn print_packet_loss_stats(&self) {
        let challenges_sent = *self.total_challenges_sent.lock().unwrap();
        let responses_received = *self.total_responses_received.lock().unwrap();

        // Clean up expired challenges (older than 5 seconds)
        let now = Instant::now();
        let mut pending = self.pending_challenges.lock().unwrap();
        pending.retain(|_, timestamp| now.duration_since(*timestamp) < Duration::from_secs(5));
        let pending_count = pending.len();
        drop(pending);

        if challenges_sent > 0 {
            let success_rate = (responses_received as f32 / challenges_sent as f32) * 100.0;
            info!(
                "📊 PACKET LOSS STATS: Sent: {}, Received: {}, Pending: {}, Success: {:.1}%",
                challenges_sent, responses_received, pending_count, success_rate
            );
        }
    }

    fn send_challenge_response(&self, challenge_id: u32) {
        let response_command = serde_json::json!({
            "type": "challenge_response",
            "id": challenge_id
        });

        let message = response_command.to_string();
        let broadcast_addr = mesh_addr_t { addr: [0xFF; 6] };

        let mesh_data = mesh_data_t {
            data: message.as_ptr() as *mut u8,
            size: message.len() as u16,
            proto: 0,
            tos: 0,
        };

        let flag = 0x01; // MESH_DATA_GROUP flag

        unsafe {
            let err = esp_mesh_send(&broadcast_addr, &mesh_data, flag, ptr::null(), 0);
            if err == sys::ESP_OK {
                info!("📤 Sent challenge response for ID: {}", challenge_id);
            } else {
                warn!("Failed to send challenge response: {}", err);
            }
        }
    }
}

fn get_disconnect_reason_string(reason: u8) -> &'static str {
    match reason {
        2 => "AUTH_EXPIRE",
        3 => "AUTH_LEAVE",
        4 => "ASSOC_EXPIRE",
        5 => "ASSOC_TOOMANY",
        6 => "NOT_AUTHED",
        7 => "NOT_ASSOCED",
        8 => "ASSOC_LEAVE",
        9 => "ASSOC_NOT_AUTHED",
        10 => "DISASSOC_PWRCAP_BAD",
        11 => "DISASSOC_SUPCHAN_BAD",
        13 => "IE_INVALID",
        14 => "MIC_FAILURE",
        15 => "4WAY_HANDSHAKE_TIMEOUT",
        16 => "GROUP_KEY_UPDATE_TIMEOUT",
        17 => "IE_IN_4WAY_DIFFERS",
        18 => "GROUP_CIPHER_INVALID",
        19 => "PAIRWISE_CIPHER_INVALID",
        20 => "AKMP_INVALID",
        21 => "UNSUPP_RSN_IE_VERSION",
        22 => "INVALID_RSN_IE_CAP",
        23 => "802_1X_AUTH_FAILED",
        24 => "CIPHER_SUITE_REJECTED",
        200 => "BEACON_TIMEOUT",
        201 => "NO_AP_FOUND",
        202 => "AUTH_FAIL",
        203 => "ASSOC_FAIL",
        204 => "HANDSHAKE_TIMEOUT",
        205 => "CONNECTION_FAIL",
        206 => "AP_TSF_RESET",
        207 => "ROAMING",
        208 => "ASSOC_COMEBACK_TIME_TOO_LONG",
        209 => "SA_QUERY_TIMEOUT",
        210 => "NO_AP_FOUND_W_COMPATIBLE_SECURITY",
        211 => "NO_AP_FOUND_IN_AUTHMODE_THRESHOLD",
        212 => "NO_AP_FOUND_IN_RSSI_THRESHOLD",
        _ => "UNKNOWN",
    }
}

unsafe extern "C" fn mesh_event_handler(
    _arg: *mut c_void,
    event_base: esp_event_base_t,
    event_id: i32,
    event_data: *mut c_void,
) {
    if event_base == MESH_EVENT {
        match event_id as u32 {
            sys::mesh_event_id_t_MESH_EVENT_STARTED => {
                info!("Mesh started");
            }
            sys::mesh_event_id_t_MESH_EVENT_STOPPED => {
                info!("Mesh stopped");
            }
            sys::mesh_event_id_t_MESH_EVENT_PARENT_CONNECTED => {
                info!("Parent connected");
                let layer = esp_mesh_get_layer();
                info!("Layer: {layer}");

                if !event_data.is_null() {
                    let event = event_data as *const sys::mesh_event_connected_t;
                    let parent_mac = (*event).connected.bssid;
                    info!(
                        "Connected to parent: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                        parent_mac[0],
                        parent_mac[1],
                        parent_mac[2],
                        parent_mac[3],
                        parent_mac[4],
                        parent_mac[5]
                    );
                }
            }
            sys::mesh_event_id_t_MESH_EVENT_PARENT_DISCONNECTED => {
                if !event_data.is_null() {
                    let event = event_data as *const sys::mesh_event_disconnected_t;
                    let reason = (*event).reason;
                    info!(
                        "Parent disconnected, reason: {} ({})",
                        reason,
                        get_disconnect_reason_string(reason)
                    );
                } else {
                    info!("Parent disconnected");
                }
            }
            sys::mesh_event_id_t_MESH_EVENT_CHILD_CONNECTED => {
                if !event_data.is_null() {
                    let event = event_data as *const sys::mesh_event_child_connected_t;
                    let child_mac = (*event).mac;
                    info!(
                        "Child connected: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                        child_mac[0],
                        child_mac[1],
                        child_mac[2],
                        child_mac[3],
                        child_mac[4],
                        child_mac[5]
                    );
                } else {
                    info!("Child connected");
                }
            }
            sys::mesh_event_id_t_MESH_EVENT_CHILD_DISCONNECTED => {
                if !event_data.is_null() {
                    let event = event_data as *const sys::mesh_event_child_disconnected_t;
                    let child_mac = (*event).mac;
                    info!(
                        "Child disconnected: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                        child_mac[0],
                        child_mac[1],
                        child_mac[2],
                        child_mac[3],
                        child_mac[4],
                        child_mac[5]
                    );
                } else {
                    info!("Child disconnected");
                }
            }
            sys::mesh_event_id_t_MESH_EVENT_ROOT_ADDRESS => {
                info!("Root address changed");
            }
            sys::mesh_event_id_t_MESH_EVENT_VOTE_STARTED => {
                info!("Vote started");
            }
            sys::mesh_event_id_t_MESH_EVENT_VOTE_STOPPED => {
                info!("Vote stopped");
            }
            sys::mesh_event_id_t_MESH_EVENT_ROOT_SWITCH_REQ => {
                info!("Root switch request");
            }
            sys::mesh_event_id_t_MESH_EVENT_ROOT_SWITCH_ACK => {
                info!("Root switch acknowledged");
            }
            sys::mesh_event_id_t_MESH_EVENT_TODS_STATE => {
                info!("TODS state update");
            }
            sys::mesh_event_id_t_MESH_EVENT_ROOT_FIXED => {
                let is_root = esp_mesh_is_root();
                info!("Root fixed: {is_root}");
            }
            _ => {
                debug!("Unknown mesh event: {event_id}");
            }
        }
    } else if event_base == IP_EVENT && event_id as u32 == sys::ip_event_t_IP_EVENT_STA_GOT_IP {
        info!("Station got IP");
    }
}

fn init_wifi() -> Result<()> {
    unsafe {
        // Initialize NVS first
        esp!(nvs_flash_init())?;

        esp!(esp_netif_init())?;
        esp!(esp_event_loop_create_default())?;

        let mut sta_netif: *mut sys::esp_netif_obj = std::ptr::null_mut();
        let mut ap_netif: *mut sys::esp_netif_obj = std::ptr::null_mut();
        sys::esp_netif_create_default_wifi_mesh_netifs(&mut sta_netif, &mut ap_netif);

        // Create proper WiFi configuration
        let cfg = wifi_init_config_t {
            osi_funcs: &raw mut g_wifi_osi_funcs,
            wpa_crypto_funcs: g_wifi_default_wpa_crypto_funcs,
            static_rx_buf_num: 10,
            dynamic_rx_buf_num: 32,
            tx_buf_type: 1,
            static_tx_buf_num: 0,
            dynamic_tx_buf_num: 32,
            cache_tx_buf_num: 0,
            csi_enable: 0,
            ampdu_rx_enable: 1,
            ampdu_tx_enable: 1,
            amsdu_tx_enable: 0,
            nvs_enable: 1,
            nano_enable: 0,
            rx_ba_win: 6,
            wifi_task_core_id: 0,
            beacon_max_len: 752,
            mgmt_sbuf_num: 32,
            feature_caps: sys::WIFI_FEATURE_CAPS as u64,
            sta_disconnected_pm: false,
            espnow_max_encrypt_num: 7,
            magic: WIFI_INIT_CONFIG_MAGIC as i32,
            dump_hesigb_enable: false,
            rx_mgmt_buf_num: 32,
            rx_mgmt_buf_type: 0,
            tx_hetb_queue_num: 0,
        };

        esp!(esp_wifi_init(&cfg))?;
        esp!(esp_wifi_set_storage(wifi_storage_t_WIFI_STORAGE_RAM))?;
        esp!(esp_wifi_start())?;
    }

    Ok(())
}

fn init_mesh() -> Result<()> {
    unsafe {
        esp!(esp_mesh_init())?;

        // Register event handlers
        esp!(esp_event_handler_register(
            MESH_EVENT,
            ESP_EVENT_ANY_ID,
            Some(mesh_event_handler),
            ptr::null_mut()
        ))?;

        esp!(esp_event_handler_register(
            IP_EVENT,
            ESP_EVENT_ANY_ID,
            Some(mesh_event_handler),
            ptr::null_mut()
        ))?;

        // Configure mesh using mesh_cfg_t structure
        let mesh_id = mesh_addr_t { addr: MESH_ID };

        let ssid = get_embedded_env_value("ROUTER_SSID");
        let pass = get_embedded_env_value("ROUTER_PASSWORD");

        info!("Router SSID: {}, Password length: {}", ssid, pass.len());

        let ssid_bytes = ssid.as_bytes();
        let mut router_ssid = [0u8; 32];
        router_ssid[..ssid_bytes.len()].copy_from_slice(ssid_bytes);

        let pass_bytes = pass.as_bytes();
        let mut router_password = [0u8; 64];
        router_password[..pass_bytes.len()].copy_from_slice(pass_bytes);

        let router = mesh_router_t {
            ssid: router_ssid,
            ssid_len: ssid_bytes.len() as u8,
            bssid: [0; 6], // Will connect to any BSSID
            password: router_password,
            allow_router_switch: true,
        };

        // Create mesh AP configuration
        info!(
            "Mesh password: '{}' (length: {})",
            MESH_PASSWORD,
            MESH_PASSWORD.len()
        );

        // Ensure password is properly formatted and null-terminated
        let mesh_ap_password = MESH_PASSWORD.as_bytes();
        let mut mesh_ap_pwd = [0u8; 64];
        for (i, &byte) in mesh_ap_password.iter().enumerate() {
            if i < 63 {
                // Leave room for null terminator
                mesh_ap_pwd[i] = byte;
            }
        }
        mesh_ap_pwd[mesh_ap_password.len()] = 0; // Null terminate

        info!(
            "Setting mesh AP with password length: {}, max connections: {}, password: {:?}",
            mesh_ap_password.len(),
            MESH_AP_CONNECTIONS,
            mesh_ap_pwd
        );

        let mesh_ap = sys::mesh_ap_cfg_t {
            password: mesh_ap_pwd,
            max_connection: MESH_AP_CONNECTIONS as u8,
            nonmesh_max_connection: 0,
        };

        // Create main mesh configuration
        let cfg = mesh_cfg_t {
            channel: MESH_CHANNEL,
            allow_channel_switch: false,
            mesh_id,
            router,
            mesh_ap,
            crypto_funcs: ptr::null(),
        };

        // Apply configuration
        esp!(esp_mesh_set_config(&cfg))?;

        // Additional mesh settings
        esp!(esp_mesh_set_max_layer(MESH_MAX_LAYER))?;
        esp!(esp_mesh_set_vote_percentage(1.0))?;

        // Set auth mode - if mesh password is empty, use OPEN, otherwise WPA2
        // let auth_mode = if MESH_PASSWORD.is_empty() {
        //     info!("Setting mesh AP auth mode to OPEN (no password)");
        //     sys::wifi_auth_mode_t_WIFI_AUTH_OPEN
        // } else {
        //     info!("Setting mesh AP auth mode to WPA2_PSK (password required)");
        //     sys::wifi_auth_mode_t_WIFI_AUTH_WPA2_PSK
        // };
        let auth_mode = sys::wifi_auth_mode_t_WIFI_AUTH_OPEN;
        esp!(esp_mesh_set_ap_authmode(auth_mode))?;
        // Set authentication for mesh AP - critical for inter-node communication
        // esp!(esp_mesh_set_ap_authmode(
        //     sys::wifi_auth_mode_t_WIFI_AUTH_WPA2_PSK
        // ))?;

        // // Also explicitly set the AP password (sometimes needed in addition to config)
        // esp!(esp_mesh_set_ap_password(
        //     mesh_ap_pwd.as_ptr(),
        //     mesh_ap_password.len() as i32
        // ))?;

        // Start mesh
        esp!(esp_mesh_start())?;
    }

    Ok(())
}

fn mesh_rx_task(node: Arc<MeshNode>, state: Arc<Mutex<State>>) {
    let mut rx_buf = vec![0u8; 1500];
    let mut from_addr = mesh_addr_t { addr: [0; 6] };
    let mut flag = 0i32;

    loop {
        let mut mesh_data = mesh_data_t {
            data: rx_buf.as_mut_ptr(),
            size: rx_buf.len() as u16,
            proto: 0,
            tos: 0,
        };

        unsafe {
            let err = esp_mesh_recv(
                &mut from_addr,
                &mut mesh_data,
                5000,
                &mut flag,
                ptr::null_mut(),
                0,
            );

            if err == sys::ESP_OK {
                let data_str = std::str::from_utf8(&rx_buf[..mesh_data.size as usize])
                    .unwrap_or("Invalid UTF-8");
                info!(
                    "Received from {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}: {}",
                    from_addr.addr[0],
                    from_addr.addr[1],
                    from_addr.addr[2],
                    from_addr.addr[3],
                    from_addr.addr[4],
                    from_addr.addr[5],
                    data_str
                );

                // Parse JSON commands (color, challenges, responses) with better error handling
                match serde_json::from_str::<serde_json::Value>(data_str) {
                    Ok(command) => {
                        // Handle different message types
                        if let Some(msg_type) = command["type"].as_str() {
                            match msg_type {
                                "challenge" => {
                                    if let Some(challenge_id) = command["id"].as_u64() {
                                        info!("📨 Received challenge ID: {}", challenge_id);
                                        node.send_challenge_response(challenge_id as u32);
                                    }
                                }
                                "challenge_response" => {
                                    if let Some(challenge_id) = command["id"].as_u64() {
                                        info!(
                                            "📥 Received response for challenge ID: {}",
                                            challenge_id
                                        );
                                        node.handle_challenge_response(challenge_id as u32);
                                    }
                                }
                                "data" => {
                                    if let Some(data) = command["data"].as_array() {
                                        for item in data {
                                            // Type is (i32, (u8, u8, u8))
                                            if let Some(value) = item.as_u64() {
                                                info!("Received data item: {}", value);
                                            }
                                        }
                                    }
                                }
                                _ => {
                                    warn!("Unknown message type: {}", msg_type);
                                }
                            }
                        } else if let (Some(r), Some(g), Some(b)) = (
                            command["r"].as_u64().map(|v| v as u8),
                            command["g"].as_u64().map(|v| v as u8),
                            command["b"].as_u64().map(|v| v as u8),
                        ) {
                            info!("Received valid color command: RGB({}, {}, {})", r, g, b);
                            node.set_color(r, g, b);
                        } else {
                            warn!("Invalid command format: missing type or r/g/b values");
                        }
                    }
                    Err(e) => {
                        warn!("Failed to parse JSON command: {} - Data: {}", e, data_str);
                    }
                }
            }
        }

        thread::sleep(Duration::from_millis(10));
    }
}

fn mesh_tx_task(node: Arc<MeshNode>, state: Arc<Mutex<State>>) {
    let mut counter = 0u32;
    let mut _challenge_counter = 0u32;

    loop {
        thread::sleep(Duration::from_secs(5)); // Send updates every 5 second

        unsafe {
            if !esp_mesh_is_device_active() {
                continue;
            }

            let is_root = esp_mesh_is_root();

            if is_root {
                // Lock the state
                let mut state_lock = state.lock().unwrap();

                // If we only have a few seconds of data left, generate more
                if state_lock.instructions.get_buffer_left() < 5000 {
                    state_lock.instructions.generate_random_instructions(5);
                }

                // Send the next 5 seconds of colours to all nodes
                let color_command = serde_json::json!({
                    "type": "data",
                    "data": state_lock.instructions.get_next_seconds_instructions(5),
                });

                let message = color_command.to_string();
                let broadcast_addr = mesh_addr_t { addr: [0xFF; 6] };

                let mesh_data = mesh_data_t {
                    data: message.as_ptr() as *mut u8,
                    size: message.len() as u16,
                    proto: 0,
                    tos: 0,
                };

                let flag = 0x01; // MESH_DATA_GROUP flag

                // Try sending the message up to 3 times for better reliability
                let mut success = false;
                for attempt in 1..=3 {
                    let err = esp_mesh_send(&broadcast_addr, &mesh_data, flag, ptr::null(), 0);

                    if err == sys::ESP_OK {
                        success = true;
                        break;
                    } else {
                        warn!(
                            "Failed to send color command on attempt {}: error {}",
                            attempt, err
                        );
                        if attempt < 3 {
                            thread::sleep(Duration::from_millis(100)); // Brief delay before retry
                        }
                    }
                }

                if !success {
                    warn!("All attempts to send color command failed",);
                }

                // Send packet loss test challenges every 5 seconds
                if counter % 5 == 0 {
                    _challenge_counter += 1;
                    let challenge_id = esp_random();
                    if node.send_challenge(challenge_id) {
                        info!("📡 Sent challenge ID: {}", challenge_id);
                    } else {
                        warn!("Failed to send challenge ID: {}", challenge_id);
                    }
                }

                // Print packet loss statistics every 30 seconds
                if counter % 30 == 0 && counter > 0 {
                    node.print_packet_loss_stats();
                }

                counter += 1;
            } else {
                // Non-root nodes can send periodic status updates
                if counter % 10 == 0 {
                    let layer = esp_mesh_get_layer();
                    let total_nodes = esp_mesh_get_total_node_num();
                    let message = format!(
                        "Status from layer {layer} (nodes: {total_nodes}, count: {counter})"
                    );

                    let broadcast_addr = mesh_addr_t { addr: [0xFF; 6] };

                    let mesh_data = mesh_data_t {
                        data: message.as_ptr() as *mut u8,
                        size: message.len() as u16,
                        proto: 0,
                        tos: 0,
                    };

                    let flag = 0x01; // MESH_DATA_GROUP flag

                    let err = esp_mesh_send(&broadcast_addr, &mesh_data, flag, ptr::null(), 0);

                    if err == sys::ESP_OK {
                        info!("Status message sent: {message}");
                    } else {
                        warn!("Failed to send status: {err:?}");
                    }
                }

                counter += 1;
            }
        }
    }
}

fn monitor_task(node: Arc<MeshNode>) {
    loop {
        thread::sleep(Duration::from_secs(5));

        unsafe {
            let is_root = esp_mesh_is_root();
            let layer = esp_mesh_get_layer();
            let is_active = esp_mesh_is_device_active();
            let total_nodes = esp_mesh_get_total_node_num();

            *node.is_root.lock().unwrap() = is_root;
            *node.is_connected.lock().unwrap() = is_active;
            *node.layer.lock().unwrap() = layer;

            info!(
                "Status - Root: {is_root}, Layer: {layer}, Active: {is_active}, Total Nodes: {total_nodes}"
            );

            // Don't override synchronized colors - only show status when disconnected
            if !is_root && !is_active {
                node.update_status_color();
            }
        }
    }
}

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
    
    let node_clone = node.clone();
    let state_clone = state.clone();
    thread::spawn(move || loop {
        let mut state_lock = state_clone.lock().unwrap();

        let instruction = state_lock.instructions.get_next_instruction();

        match instruction {
            InstructionStatus::Sleep(duration) => {
                thread::sleep(Duration::from_millis(duration as u64));
            }
            InstructionStatus::SetColor(color) => {
                node_clone.set_color(color.r, color.g, color.b);
            }
        }
    });

    info!("Mesh node started. Waiting for connections...");
    info!("WS2812 (GPIO18): Real addressable RGB LED with precise RMT timing!");
    info!("Status colors: Off=disconnected, Blue=child node, Green=root node");
    info!("Root node will send synchronized color updates every second");

    loop {
        thread::sleep(Duration::from_secs(1));
    }
}
