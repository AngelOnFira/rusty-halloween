use crate::utils::{get_disconnect_reason_string, get_embedded_env_value};
use crate::scan::{self, NetworkDiscovery};
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
    ESP_EVENT_ANY_ID, IP_EVENT, MESH_EVENT, WIFI_EVENT, WIFI_INIT_CONFIG_MAGIC,
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
// MESH_CHANNEL is now discovered at runtime via WiFi scan (see scan.rs)
// and persisted to NVS for faster boot times
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
                info!("Mesh event: Mesh started");
            }
            sys::mesh_event_id_t_MESH_EVENT_STOPPED => {
                info!("Mesh event: Mesh stopped");
            }
            sys::mesh_event_id_t_MESH_EVENT_PARENT_CONNECTED => {
                info!("Mesh event: Parent connected");
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
                    info!("Mesh event: Node is now root - starting DHCP client for external IP");
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
                        "Mesh event: Parent disconnected, reason: {} ({})",
                        reason,
                        get_disconnect_reason_string(reason)
                    );
                } else {
                    info!("Mesh event: Parent disconnected");
                }
            }
            sys::mesh_event_id_t_MESH_EVENT_CHILD_CONNECTED => {
                if !event_data.is_null() {
                    let event = event_data as *const sys::mesh_event_child_connected_t;
                    let child_mac = (*event).mac;
                    info!(
                        "Mesh event: Child connected: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                        child_mac[0],
                        child_mac[1],
                        child_mac[2],
                        child_mac[3],
                        child_mac[4],
                        child_mac[5]
                    );
                } else {
                    info!("Mesh event: Child connected");
                }
            }
            sys::mesh_event_id_t_MESH_EVENT_CHILD_DISCONNECTED => {
                if !event_data.is_null() {
                    let event = event_data as *const sys::mesh_event_child_disconnected_t;
                    let child_mac = (*event).mac;
                    info!(
                        "Mesh event: Child disconnected: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                        child_mac[0],
                        child_mac[1],
                        child_mac[2],
                        child_mac[3],
                        child_mac[4],
                        child_mac[5]
                    );
                } else {
                    info!("Mesh event: Child disconnected");
                }
            }
            sys::mesh_event_id_t_MESH_EVENT_ROOT_ADDRESS => {
                info!("Mesh event: Root address changed");
            }
            sys::mesh_event_id_t_MESH_EVENT_VOTE_STARTED => {
                info!("Mesh event: Vote started");
            }
            sys::mesh_event_id_t_MESH_EVENT_VOTE_STOPPED => {
                info!("Mesh event: Vote stopped");
            }
            sys::mesh_event_id_t_MESH_EVENT_ROOT_SWITCH_REQ => {
                info!("Mesh event: Root switch request");
            }
            sys::mesh_event_id_t_MESH_EVENT_ROOT_SWITCH_ACK => {
                info!("Mesh event: Root switch acknowledged");
            }
            sys::mesh_event_id_t_MESH_EVENT_TODS_STATE => {
                info!("Mesh event: TODS state");
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
                info!("Mesh event: Root fixed: {is_root}");
            }
            sys::mesh_event_id_t_MESH_EVENT_NO_PARENT_FOUND => {
                warn!("Mesh event: No parent found - searching for mesh network");
            }
            sys::mesh_event_id_t_MESH_EVENT_FIND_NETWORK => {
                info!("Mesh event: Finding mesh network...");
            }
            sys::mesh_event_id_t_MESH_EVENT_ROUTER_SWITCH => {
                if !event_data.is_null() {
                    let event = event_data as *const sys::mesh_event_router_switch_t;
                    let bssid = (*event).bssid;
                    let channel = (*event).channel;
                    info!(
                        "Mesh event: Router switch: New BSSID {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}, Channel {}",
                        bssid[0], bssid[1], bssid[2],
                        bssid[3], bssid[4], bssid[5],
                        channel
                    );
                } else {
                    info!("Mesh event: Router switch occurred");
                }
            }
            _ => {
                debug!("Unknown mesh event: {event_id}");
            }
        }
    } else if event_base == IP_EVENT {
        match event_id as u32 {
            sys::ip_event_t_IP_EVENT_STA_GOT_IP => {
                info!("IP event: Station got IP");
                // Set global flag that we have IP address
                *HAS_IP.lock().unwrap() = true;

                if !event_data.is_null() {
                    let event = event_data as *const sys::ip_event_got_ip_t;
                    let ip = (*event).ip_info.ip;
                    let gw = (*event).ip_info.gw;
                    let netmask = (*event).ip_info.netmask;
                    info!(
                        "✅ IP: {}.{}.{}.{}, Gateway: {}.{}.{}.{}, Netmask: {}.{}.{}.{}",
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
            sys::ip_event_t_IP_EVENT_STA_LOST_IP => {
                warn!("IP event: Station lost IP - DHCP failed or connection lost");
                *HAS_IP.lock().unwrap() = false;
            }
            sys::ip_event_t_IP_EVENT_AP_STAIPASSIGNED => {
                info!("IP event: AP station assigned IP (event {})", event_id);
            }
            sys::ip_event_t_IP_EVENT_GOT_IP6 => {
                info!("IP event: Got IPv6 (event {})", event_id);
            }
            sys::ip_event_t_IP_EVENT_ETH_GOT_IP => {
                info!("IP event: Ethernet got IP (event {})", event_id);
            }
            sys::ip_event_t_IP_EVENT_ETH_LOST_IP => {
                warn!("IP event: Ethernet lost IP (event {})", event_id);
            }
            sys::ip_event_t_IP_EVENT_PPP_GOT_IP => {
                info!("IP event: PPP got IP (event {})", event_id);
            }
            sys::ip_event_t_IP_EVENT_PPP_LOST_IP => {
                warn!("IP event: PPP lost IP (event {})", event_id);
            }
            _ => {
                debug!("Unknown IP event: {}", event_id);
            }
        }
    } else if event_base == WIFI_EVENT {
        match event_id as u32 {
            sys::wifi_event_t_WIFI_EVENT_WIFI_READY => {
                info!("Wifi event: WIFI_READY");
            }
            sys::wifi_event_t_WIFI_EVENT_SCAN_DONE => {
                info!("Wifi event: SCAN_DONE");
            }
            sys::wifi_event_t_WIFI_EVENT_STA_START => {
                info!("Wifi event: STA_START");
            }
            sys::wifi_event_t_WIFI_EVENT_STA_STOP => {
                info!("Wifi event: STA_STOP");
            }
            sys::wifi_event_t_WIFI_EVENT_STA_CONNECTED => {
                info!("Wifi event: STA_CONNECTED");
            }
            sys::wifi_event_t_WIFI_EVENT_STA_DISCONNECTED => {
                info!("Wifi event: STA_DISCONNECTED");
            }
            sys::wifi_event_t_WIFI_EVENT_STA_AUTHMODE_CHANGE => {
                info!("Wifi event: STA_AUTHMODE_CHANGE");
            }
            sys::wifi_event_t_WIFI_EVENT_STA_WPS_ER_SUCCESS => {
                info!("Wifi event: STA_WPS_ER_SUCCESS");
            }
            sys::wifi_event_t_WIFI_EVENT_STA_WPS_ER_FAILED => {
                info!("Wifi event: STA_WPS_ER_FAILED");
            }
            sys::wifi_event_t_WIFI_EVENT_STA_WPS_ER_TIMEOUT => {
                info!("Wifi event: STA_WPS_ER_TIMEOUT");
            }
            sys::wifi_event_t_WIFI_EVENT_STA_WPS_ER_PIN => {
                info!("Wifi event: STA_WPS_ER_PIN");
            }
            sys::wifi_event_t_WIFI_EVENT_STA_WPS_ER_PBC_OVERLAP => {
                info!("Wifi event: STA_WPS_ER_PBC_OVERLAP");
            }
            sys::wifi_event_t_WIFI_EVENT_AP_START => {
                info!("Wifi event: AP_START");
            }
            sys::wifi_event_t_WIFI_EVENT_AP_STOP => {
                info!("Wifi event: AP_STOP");
            }
            sys::wifi_event_t_WIFI_EVENT_AP_STACONNECTED => {
                info!("Wifi event: AP_STACONNECTED");
            }
            sys::wifi_event_t_WIFI_EVENT_AP_STADISCONNECTED => {
                info!("Wifi event: AP_STADISCONNECTED");
            }
            sys::wifi_event_t_WIFI_EVENT_AP_PROBEREQRECVED => {
                info!("Wifi event: AP_PROBEREQRECVED");
            }
            sys::wifi_event_t_WIFI_EVENT_FTM_REPORT => {
                info!("Wifi event: FTM_REPORT");
            }
            sys::wifi_event_t_WIFI_EVENT_STA_BSS_RSSI_LOW => {
                info!("Wifi event: STA_BSS_RSSI_LOW");
            }
            sys::wifi_event_t_WIFI_EVENT_ACTION_TX_STATUS => {
                info!("Wifi event: ACTION_TX_STATUS");
            }
            sys::wifi_event_t_WIFI_EVENT_ROC_DONE => {
                info!("Wifi event: ROC_DONE");
            }
            sys::wifi_event_t_WIFI_EVENT_STA_BEACON_TIMEOUT => {
                info!("Wifi event: STA_BEACON_TIMEOUT");
            }
            sys::wifi_event_t_WIFI_EVENT_CONNECTIONLESS_MODULE_WAKE_INTERVAL_START => {
                info!("Wifi event: CONNECTIONLESS_MODULE_WAKE_INTERVAL_START");
            }
            sys::wifi_event_t_WIFI_EVENT_AP_WPS_RG_SUCCESS => {
                info!("Wifi event: AP_WPS_RG_SUCCESS");
            }
            sys::wifi_event_t_WIFI_EVENT_AP_WPS_RG_FAILED => {
                info!("Wifi event: AP_WPS_RG_FAILED");
            }
            sys::wifi_event_t_WIFI_EVENT_AP_WPS_RG_TIMEOUT => {
                info!("Wifi event: AP_WPS_RG_TIMEOUT");
            }
            sys::wifi_event_t_WIFI_EVENT_AP_WPS_RG_PIN => {
                info!("Wifi event: AP_WPS_RG_PIN");
            }
            sys::wifi_event_t_WIFI_EVENT_AP_WPS_RG_PBC_OVERLAP => {
                info!("Wifi event: AP_WPS_RG_PBC_OVERLAP");
            }
            sys::wifi_event_t_WIFI_EVENT_ITWT_SETUP => {
                info!("Wifi event: ITWT_SETUP");
            }
            sys::wifi_event_t_WIFI_EVENT_ITWT_TEARDOWN => {
                info!("Wifi event: ITWT_TEARDOWN");
            }
            sys::wifi_event_t_WIFI_EVENT_ITWT_PROBE => {
                info!("Wifi event: ITWT_PROBE");
            }
            sys::wifi_event_t_WIFI_EVENT_ITWT_SUSPEND => {
                info!("Wifi event: ITWT_SUSPEND");
            }
            sys::wifi_event_t_WIFI_EVENT_TWT_WAKEUP => {
                info!("Wifi event: TWT_WAKEUP");
            }
            sys::wifi_event_t_WIFI_EVENT_BTWT_SETUP => {
                info!("Wifi event: BTWT_SETUP");
            }
            sys::wifi_event_t_WIFI_EVENT_BTWT_TEARDOWN => {
                info!("Wifi event: BTWT_TEARDOWN");
            }
            sys::wifi_event_t_WIFI_EVENT_NAN_STARTED => {
                info!("Wifi event: NAN_STARTED");
            }
            sys::wifi_event_t_WIFI_EVENT_NAN_STOPPED => {
                info!("Wifi event: NAN_STOPPED");
            }
            sys::wifi_event_t_WIFI_EVENT_NAN_SVC_MATCH => {
                info!("Wifi event: NAN_SVC_MATCH");
            }
            sys::wifi_event_t_WIFI_EVENT_NAN_REPLIED => {
                info!("Wifi event: NAN_REPLIED");
            }
            sys::wifi_event_t_WIFI_EVENT_NAN_RECEIVE => {
                info!("Wifi event: NAN_RECEIVE");
            }
            sys::wifi_event_t_WIFI_EVENT_NDP_INDICATION => {
                info!("Wifi event: NDP_INDICATION");
            }
            sys::wifi_event_t_WIFI_EVENT_NDP_CONFIRM => {
                info!("Wifi event: NDP_CONFIRM");
            }
            sys::wifi_event_t_WIFI_EVENT_NDP_TERMINATED => {
                info!("Wifi event: NDP_TERMINATED");
            }
            sys::wifi_event_t_WIFI_EVENT_HOME_CHANNEL_CHANGE => {
                info!("Wifi event: HOME_CHANNEL_CHANGE");
            }
            sys::wifi_event_t_WIFI_EVENT_STA_NEIGHBOR_REP => {
                info!("Wifi event: STA_NEIGHBOR_REP");
            }
            sys::wifi_event_t_WIFI_EVENT_AP_WRONG_PASSWORD => {
                info!("Wifi event: AP_WRONG_PASSWORD");
            }
            sys::wifi_event_t_WIFI_EVENT_MAX => {
                info!("Wifi event: MAX");
            }
            _ => {
                debug!("Unknown WiFi event: {}", event_id);
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

    // let mut sta_netif: *mut sys::esp_netif_obj = std::ptr::null_mut();
    // let mut ap_netif: *mut sys::esp_netif_obj = std::ptr::null_mut();

    // unsafe {
    //     sys::esp_netif_create_default_wifi_mesh_netifs(&mut sta_netif, &mut ap_netif);
    // }

    // Save netif pointers globally for DHCP management (as usize for thread safety)
    // *STA_NETIF.lock().unwrap() = sta_netif as usize;
    // *AP_NETIF.lock().unwrap() = ap_netif as usize;
    // info!(
    //     "Network interfaces created - STA: {:p}, AP: {:p}",
    //     sta_netif, ap_netif
    // );

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
        // esp!(esp_wifi_set_mode(sys::wifi_ps_type_t_WIFI_PS_NONE))?;
        info!("WiFi initialized");
        esp!(esp_wifi_set_storage(sys::wifi_storage_t_WIFI_STORAGE_FLASH))?;
        info!("WiFi storage set to FLASH");
        // info!("WiFi mode set to STA/AP");
        esp!(esp_wifi_start())?;
        info!("WiFi started");

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

        // WIFI events
        esp!(esp_event_handler_register(
            WIFI_EVENT,
            ESP_EVENT_ANY_ID,
            Some(mesh_event_handler),
            ptr::null_mut()
        ))?;
    }

    Ok(())
}

/// Scan for 2.4GHz routers and select the best one
/// NOTE: Currently commented out - mesh auto-selects router
/// This function is preserved for potential future debugging
#[allow(dead_code)]
fn scan_for_2ghz_router() -> Result<()> {
    let channel_bitmap = wifi_scan_channel_bitmap_t {
        // Do all 2.4ghz channels
        ghz_2_channels: 0xFFFF,
        ghz_5_channels: 0,
    };

    let ssid = get_embedded_env_value("ROUTER_SSID");
    let ssid_cstring = CString::new(ssid.clone()).unwrap();
    let ssid_cstring_ptr = ssid_cstring.as_ptr() as *mut u8;
    unsafe {
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

        // Setting WiFi mode to STA for scan
        info!("WiFi mode set to STA");
        esp!(esp_wifi_set_mode(sys::wifi_mode_t_WIFI_MODE_STA))?;

        info!("Stopping WiFi scan");
        esp!(esp_wifi_scan_stop())?;
        info!("Starting WiFi scan");
        esp!(esp_wifi_scan_start(scan_config, true))?;
    }

    // Get scan results
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

    // Reset the WiFi mode to APSTA
    unsafe {
        esp!(esp_wifi_set_mode(sys::wifi_mode_t_WIFI_MODE_APSTA))?;
    }

    info!("WiFi mode reset to APSTA");

    Ok(())
}

/// Initialize mesh network
pub fn init_mesh() -> Result<()> {
    // Get router credentials from .env
    let router_ssid = get_embedded_env_value("ROUTER_SSID");
    let router_pass = get_embedded_env_value("ROUTER_PASSWORD");
    info!("Router SSID: {}, Password length: {}", router_ssid, router_pass.len());

    // Step 1: Try to load channel from NVS (persisted from previous boot)
    let mut mesh_channel: Option<u8> = scan::load_channel_from_nvs();

    // Step 2: If no saved channel, scan for networks to discover channel
    if mesh_channel.is_none() {
        info!("No saved channel found, scanning for networks...");

        // Scan with retry (infinite retry until found)
        let discovery = scan::scan_with_retry(&router_ssid, &MESH_ID, 30_000);

        match discovery {
            NetworkDiscovery::ExistingMesh { channel, ssid, bssid, rssi } => {
                info!(
                    "🔗 Discovered existing mesh network: '{}' on channel {}, RSSI: {}, BSSID: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                    ssid, channel, rssi,
                    bssid[0], bssid[1], bssid[2], bssid[3], bssid[4], bssid[5]
                );
                info!("Will join existing mesh on channel {}", channel);
                mesh_channel = Some(channel);
            }
            NetworkDiscovery::Router { channel, bssid, rssi } => {
                info!(
                    "📡 Discovered router '{}' on channel {}, RSSI: {}, BSSID: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                    router_ssid, channel, rssi,
                    bssid[0], bssid[1], bssid[2], bssid[3], bssid[4], bssid[5]
                );
                info!("Will create new mesh on channel {}", channel);
                mesh_channel = Some(channel);

                // Store router BSSID for connection
                *ROUTER_BSSID.lock().unwrap() = bssid;
            }
            NetworkDiscovery::NotFound => {
                // This should never happen with scan_with_retry, but handle it anyway
                return Err(anyhow::anyhow!("Network scan failed - no mesh or router found"));
            }
        }
    } else {
        info!("Using saved channel from NVS: {}", mesh_channel.unwrap());
    }

    let channel = mesh_channel.expect("Channel must be determined by this point");

    unsafe {
        // Initialize mesh
        esp!(esp_mesh_init())?;

        // Configure mesh using mesh_cfg_t structure
        let mesh_id = mesh_addr_t { addr: MESH_ID };

        // Prepare router SSID
        let ssid_bytes = router_ssid.as_bytes();
        let mut router_ssid_array = [0u8; 32];
        router_ssid_array[..ssid_bytes.len()].copy_from_slice(ssid_bytes);

        // Prepare router password
        let pass_bytes = router_pass.as_bytes();
        let mut router_password = [0u8; 64];
        router_password[..pass_bytes.len()].copy_from_slice(pass_bytes);

        // Get saved BSSID if available (may be all zeros for auto-select)
        let router_bssid = *ROUTER_BSSID.lock().unwrap();

        let router = mesh_router_t {
            ssid: router_ssid_array,
            ssid_len: ssid_bytes.len() as u8,
            bssid: router_bssid,
            password: router_password,
            allow_router_switch: true,
        };

        // Create mesh AP configuration
        info!(
            "Mesh AP password: '{}' (length: {})",
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

        let mesh_ap = sys::mesh_ap_cfg_t {
            password: mesh_ap_pwd,
            max_connection: MESH_AP_CONNECTIONS as u8,
            nonmesh_max_connection: 0,
        };

        // Create main mesh configuration with discovered channel
        let cfg = mesh_cfg_t {
            channel, // Use discovered/saved channel
            allow_channel_switch: false,
            mesh_id,
            router,
            mesh_ap,
            crypto_funcs: ptr::null(),
        };

        // Apply configuration
        info!("Setting mesh configuration with channel {}", channel);
        esp!(esp_mesh_set_config(&cfg))?;

        // Additional mesh settings
        info!("Setting mesh max layer");
        esp!(esp_mesh_set_max_layer(MESH_MAX_LAYER))?;
        info!("Setting mesh vote percentage");
        esp!(esp_mesh_set_vote_percentage(1.0))?;

        // Set auth mode to OPEN for inter-node communication
        info!("Setting mesh AP auth mode to OPEN (no password)");
        let auth_mode = sys::wifi_auth_mode_t_WIFI_AUTH_OPEN;
        esp!(esp_mesh_set_ap_authmode(auth_mode))?;

        // Start mesh
        info!("Starting mesh on channel {}", channel);
        esp!(esp_mesh_start())?;

        // Save channel to NVS for faster boot next time
        info!("Saving channel {} to NVS for persistence", channel);
        scan::save_channel_to_nvs(channel);
    }

    Ok(())
}
