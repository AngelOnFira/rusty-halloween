use crate::utils::{get_disconnect_reason_string, get_embedded_env_value};
use anyhow::Result;
use esp_idf_sys::{
    self as sys, esp, esp_event_base_t, esp_event_handler_register, esp_event_loop_create_default,
    esp_mesh_get_layer, esp_mesh_init, esp_mesh_is_root, esp_mesh_set_ap_authmode,
    esp_mesh_set_config, esp_mesh_set_max_layer, esp_mesh_set_vote_percentage, esp_mesh_start,
    esp_netif_init, esp_wifi_init, esp_wifi_scan_get_ap_records, esp_wifi_scan_start,
    esp_wifi_scan_stop, esp_wifi_set_mode, esp_wifi_set_storage, esp_wifi_start,
    g_wifi_default_wpa_crypto_funcs, g_wifi_osi_funcs, mesh_addr_t, mesh_cfg_t, mesh_router_t,
    nvs_flash_init, wifi_active_scan_time_t, wifi_ap_record_t, wifi_init_config_t,
    wifi_scan_channel_bitmap_t, wifi_scan_config_t, wifi_scan_time_t,
    wifi_storage_t_WIFI_STORAGE_RAM, ESP_EVENT_ANY_ID, IP_EVENT, MESH_EVENT,
    WIFI_INIT_CONFIG_MAGIC,
};
use log::*;
use once_cell::sync::Lazy;
use std::{
    ffi::CString,
    os::raw::c_void,
    ptr,
    sync::{Arc, Mutex},
};

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

/// Global BSSID of the 2.4GHz router to connect to
/// Selected during WiFi scan to ensure mesh connects to 2.4GHz (not 5GHz)
pub static ROUTER_BSSID: Lazy<Arc<Mutex<[u8; 6]>>> = Lazy::new(|| Arc::new(Mutex::new([0; 6])));

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
                        if stop_result == sys::ESP_OK
                            || stop_result == sys::ESP_ERR_ESP_NETIF_DHCP_ALREADY_STOPPED
                        {
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
                            info!(
                                "✅ TODS connected (state: {}) - Root has external network access",
                                tods_state
                            );
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
    } else if event_base == IP_EVENT {
        match event_id as u32 {
            sys::ip_event_t_IP_EVENT_STA_GOT_IP => {
                info!("✅ Station got IP");
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
                }
            }
            sys::ip_event_t_IP_EVENT_STA_LOST_IP
            | sys::ip_event_t_IP_EVENT_AP_STAIPASSIGNED
            | sys::ip_event_t_IP_EVENT_GOT_IP6
            | sys::ip_event_t_IP_EVENT_ETH_GOT_IP
            | sys::ip_event_t_IP_EVENT_ETH_LOST_IP
            | sys::ip_event_t_IP_EVENT_PPP_GOT_IP
            | sys::ip_event_t_IP_EVENT_PPP_LOST_IP => {
                info!("✅ IP event: {event_id}");
            }
            _ => {
                info!("✅ Station got IP - OTA updates enabled");
            }
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
    }

    let mut sta_netif: *mut sys::esp_netif_obj = std::ptr::null_mut();
    let mut ap_netif: *mut sys::esp_netif_obj = std::ptr::null_mut();

    unsafe {
        sys::esp_netif_create_default_wifi_mesh_netifs(&mut sta_netif, &mut ap_netif);
    }

    // Save netif pointers globally for DHCP management (as usize for thread safety)
    *STA_NETIF.lock().unwrap() = sta_netif as usize;
    *AP_NETIF.lock().unwrap() = ap_netif as usize;
    info!(
        "Network interfaces created - STA: {:p}, AP: {:p}",
        sta_netif, ap_netif
    );

    // Create proper WiFi configuration
    let cfg = wifi_init_config_t {
        osi_funcs: &raw mut g_wifi_osi_funcs,
        wpa_crypto_funcs: unsafe { g_wifi_default_wpa_crypto_funcs },
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

    unsafe {
        esp!(esp_wifi_init(&cfg))?;
        info!("WiFi initialized");
        esp!(esp_wifi_set_storage(wifi_storage_t_WIFI_STORAGE_RAM))?;
        info!("WiFi storage set to RAM");
        esp!(esp_wifi_set_mode(sys::wifi_mode_t_WIFI_MODE_STA))?;
        info!("WiFi mode set to STA");
        esp!(esp_wifi_start())?;
        info!("WiFi started");
    }

    let channel_bitmap = wifi_scan_channel_bitmap_t {
        // Do all 2.4ghz channels
        ghz_2_channels: 0xFFFF,
        ghz_5_channels: 0,
    };

    let ssid = get_embedded_env_value("ROUTER_SSID");
    let ssid_cstring = CString::new(ssid.clone()).unwrap();
    let ssid_cstring_ptr = ssid_cstring.as_ptr() as *mut u8;

    let scan_config: *const wifi_scan_config_t = &wifi_scan_config_t {
        ssid: ssid_cstring_ptr,
        bssid: std::ptr::null_mut(),
        channel: 0,
        show_hidden: false,
        scan_type: sys::wifi_scan_type_t_WIFI_SCAN_TYPE_ACTIVE,
        scan_time: wifi_scan_time_t {
            active: wifi_active_scan_time_t {
                min: 100,
                max: 1000,
            },
            passive: 0,
        },
        home_chan_dwell_time: 100,
        channel_bitmap,
    };

    unsafe {
        // Sleep for 1 second
        // std::thread::sleep(std::time::Duration::from_secs(1));
        info!("Stopping WiFi scan");
        esp!(esp_wifi_scan_stop())?;
        info!("Starting WiFi scan");
        esp!(esp_wifi_scan_start(scan_config, true))?;
    }

    // Print the results, then panic for now
    info!("Getting WiFi scan results");
    let mut scan_results: [wifi_ap_record_t; 30] = unsafe { std::mem::zeroed() };
    let mut ap_count: u16 = 30; // Max APs we can store

    unsafe {
        esp!(esp_wifi_scan_get_ap_records(
            &mut ap_count,
            scan_results.as_mut_ptr()
        ))?;
    }

    info!("Printing WiFi scan results (found {} APs)", ap_count);

    // Filter for 2.4GHz APs (channels 1-13) and find the one with best RSSI
    let mut best_2ghz_ap: Option<&wifi_ap_record_t> = None;
    let mut best_rssi: i8 = i8::MIN;

    for result in scan_results.iter().take(ap_count as usize) {
        // Convert SSID bytes to string for comparison
        let ssid_bytes: Vec<u8> = result
            .ssid
            .iter()
            .take_while(|&&b| b != 0)
            .copied()
            .collect();
        let ap_ssid = String::from_utf8_lossy(&ssid_bytes);

        let is_2ghz = result.primary >= 1 && result.primary <= 13;
        let band = if is_2ghz { "2.4GHz" } else { "5GHz" };

        info!(
            "  SSID: {}, BSSID: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}, Channel: {} ({}), RSSI: {}",
            ap_ssid,
            result.bssid[0],
            result.bssid[1],
            result.bssid[2],
            result.bssid[3],
            result.bssid[4],
            result.bssid[5],
            result.primary,
            band,
            result.rssi
        );

        // Check if this is a 2.4GHz AP matching our target SSID
        if is_2ghz && ap_ssid == ssid.as_str() && result.rssi > best_rssi {
            best_2ghz_ap = Some(result);
            best_rssi = result.rssi;
        }
    }

    // Store the selected BSSID or error if none found
    if let Some(ap) = best_2ghz_ap {
        *ROUTER_BSSID.lock().unwrap() = ap.bssid;
        info!(
            "✅ Selected 2.4GHz AP: BSSID {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}, Channel: {}, RSSI: {}",
            ap.bssid[0],
            ap.bssid[1],
            ap.bssid[2],
            ap.bssid[3],
            ap.bssid[4],
            ap.bssid[5],
            ap.primary,
            ap.rssi
        );
    } else {
        return Err(anyhow::anyhow!(
            "❌ No 2.4GHz AP found with SSID '{}'. ESP-MESH requires 2.4GHz!",
            ssid
        ));
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

        // Get the 2.4GHz BSSID selected during WiFi scan
        let router_bssid = *ROUTER_BSSID.lock().unwrap();
        info!(
            "Using 2.4GHz router BSSID: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            router_bssid[0],
            router_bssid[1],
            router_bssid[2],
            router_bssid[3],
            router_bssid[4],
            router_bssid[5]
        );

        let router = mesh_router_t {
            ssid: router_ssid,
            ssid_len: ssid_bytes.len() as u8,
            bssid: router_bssid, // Use specific 2.4GHz BSSID from scan
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
