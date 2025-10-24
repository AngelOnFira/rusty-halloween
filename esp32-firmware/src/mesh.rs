use crate::utils::get_disconnect_reason_string;
use crate::utils::get_embedded_env_value;
use anyhow::Result;
use esp_idf_sys::{
    self as sys, esp, esp_event_base_t, esp_event_handler_register, esp_event_loop_create_default,
    esp_mesh_get_layer, esp_mesh_init, esp_mesh_is_root, esp_mesh_set_ap_authmode,
    esp_mesh_set_config, esp_mesh_set_max_layer, esp_mesh_set_vote_percentage, esp_mesh_start,
    esp_netif_init, esp_wifi_init, esp_wifi_set_storage, esp_wifi_start,
    g_wifi_default_wpa_crypto_funcs, g_wifi_osi_funcs, mesh_addr_t, mesh_cfg_t, mesh_router_t,
    nvs_flash_init, wifi_init_config_t, wifi_storage_t_WIFI_STORAGE_RAM, ESP_EVENT_ANY_ID,
    IP_EVENT, MESH_EVENT, WIFI_INIT_CONFIG_MAGIC,
};
use log::*;
use once_cell::sync::Lazy;
use std::{os::raw::c_void, ptr, sync::{Arc, Mutex}};

// Mesh network configuration constants
pub const MESH_ID: [u8; 6] = [0x77, 0x77, 0x77, 0x77, 0x77, 0x77];
pub const MESH_PASSWORD: &str = "mesh_password_123";
pub const MESH_CHANNEL: u8 = 6;
pub const MESH_MAX_LAYER: i32 = 6;
pub const MESH_AP_CONNECTIONS: i32 = 6;

/// Global flag indicating root node has received IP address
/// Set by IP_EVENT_STA_GOT_IP event handler, read by OTA check logic
pub static HAS_IP: Lazy<Arc<Mutex<bool>>> = Lazy::new(|| Arc::new(Mutex::new(false)));

/// Global network interface pointers for DHCP management
/// STA interface used by root node to connect to external router
/// AP interface used for mesh network communication
/// Stored as usize (pointer as integer) for thread safety
pub static STA_NETIF: Lazy<Arc<Mutex<usize>>> = Lazy::new(|| Arc::new(Mutex::new(0)));
pub static AP_NETIF: Lazy<Arc<Mutex<usize>>> = Lazy::new(|| Arc::new(Mutex::new(0)));

/// Mesh event handler callback for WiFi and mesh events
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

                // If this node is root, start DHCP client immediately
                if esp_mesh_is_root() {
                    info!("🌐 Node is now root - starting DHCP client for external IP");
                    let sta_netif_addr = *STA_NETIF.lock().unwrap();
                    if sta_netif_addr != 0 {
                        let sta_netif = sta_netif_addr as *mut sys::esp_netif_obj;

                        // Stop any existing DHCP session first
                        let stop_result = sys::esp_netif_dhcpc_stop(sta_netif);
                        if stop_result == sys::ESP_OK || stop_result == sys::ESP_ERR_ESP_NETIF_DHCP_ALREADY_STOPPED {
                            debug!("DHCP client stopped (result: {})", stop_result);
                        } else {
                            warn!("Failed to stop DHCP client: {}", stop_result);
                        }

                        // Start DHCP client to obtain IP from router
                        let start_result = sys::esp_netif_dhcpc_start(sta_netif);
                        if start_result == sys::ESP_OK {
                            info!("✅ DHCP client started - requesting IP from router");
                        } else if start_result == sys::ESP_ERR_ESP_NETIF_DHCP_ALREADY_STARTED {
                            info!("ℹ️  DHCP client already running");
                        } else {
                            error!("❌ Failed to start DHCP client: {}", start_result);
                        }
                    } else {
                        error!("❌ STA netif is null - cannot start DHCP");
                    }
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
                // TODS (To Distribution System) indicates connection to external network
                // This event fires AFTER the root node gets an IP via DHCP
                // DHCP is already started in PARENT_CONNECTED, this is just for verification
                if !event_data.is_null() {
                    let tods_state_ptr = event_data as *const sys::mesh_event_toDS_state_t;
                    let tods_state = *tods_state_ptr;

                    if esp_mesh_is_root() {
                        if tods_state != 0 {
                            info!("✅ TODS connected (state: {}) - Root has external network access", tods_state);
                        } else {
                            info!("ℹ️  TODS disconnected (state: 0) - Waiting for IP from router");
                        }
                    } else {
                        debug!("TODS state update: {} (non-root node)", tods_state);
                    }
                } else {
                    warn!("TODS state event with null data");
                }
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
        // Set global flag that we have IP address
        *HAS_IP.lock().unwrap() = true;

        if !event_data.is_null() {
            let event = event_data as *const sys::ip_event_got_ip_t;
            let ip = (*event).ip_info.ip;
            let gw = (*event).ip_info.gw;
            let netmask = (*event).ip_info.netmask;
            info!(
                "✅ Station got IP: {}.{}.{}.{}, Gateway: {}.{}.{}.{}, Netmask: {}.{}.{}.{}",
                (ip.addr & 0xFF),
                ((ip.addr >> 8) & 0xFF),
                ((ip.addr >> 16) & 0xFF),
                ((ip.addr >> 24) & 0xFF),
                (gw.addr & 0xFF),
                ((gw.addr >> 8) & 0xFF),
                ((gw.addr >> 16) & 0xFF),
                ((gw.addr >> 24) & 0xFF),
                (netmask.addr & 0xFF),
                ((netmask.addr >> 8) & 0xFF),
                ((netmask.addr >> 16) & 0xFF),
                ((netmask.addr >> 24) & 0xFF),
            );
            info!("🌐 Root node has internet connectivity - OTA updates enabled");
        } else {
            info!("✅ Station got IP - OTA updates enabled");
        }
    }
}

/// Initialize WiFi subsystem
pub fn init_wifi() -> Result<()> {
    unsafe {
        // Initialize NVS first
        esp!(nvs_flash_init())?;

        esp!(esp_netif_init())?;
        esp!(esp_event_loop_create_default())?;

        let mut sta_netif: *mut sys::esp_netif_obj = std::ptr::null_mut();
        let mut ap_netif: *mut sys::esp_netif_obj = std::ptr::null_mut();
        sys::esp_netif_create_default_wifi_mesh_netifs(&mut sta_netif, &mut ap_netif);

        // Save netif pointers globally for DHCP management (as usize for thread safety)
        *STA_NETIF.lock().unwrap() = sta_netif as usize;
        *AP_NETIF.lock().unwrap() = ap_netif as usize;
        info!("Network interfaces created - STA: {:p}, AP: {:p}", sta_netif, ap_netif);

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

        // let cfg = WIFI_INIT_CONFIG_DEFAULT();

        esp!(esp_wifi_init(&cfg))?;
        esp!(esp_wifi_set_storage(wifi_storage_t_WIFI_STORAGE_RAM))?;
        esp!(esp_wifi_start())?;
    }

    Ok(())
}

/// Initialize mesh network
pub fn init_mesh() -> Result<()> {
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
