//! Mesh communication operations
//!
//! This module provides type-safe wrappers around ESP-MESH communication APIs.
//! All mesh send/recv operations should go through these methods to ensure
//! consistent error handling and logging.

use crate::state::GLOBAL_STATE;

use super::types::WifiMeshState;
use esp_idf_svc::sys as sys;
use esp_idf_sys::* ;
use std::ptr;
use std::{
    ffi::CString,
    os::raw::c_void,
    sync::{Arc, Mutex},
};

/// Mesh message data
pub struct MeshMessage {
    pub from_addr: sys::mesh_addr_t,
    pub data: Vec<u8>,
    pub flag: i32,
}

/// Broadcast address (all nodes)
pub const BROADCAST_ADDR: sys::mesh_addr_t = sys::mesh_addr_t { addr: [0xFF; 6] };

/// Mesh operations available in any state
impl<W, M, S, O> WifiMeshState<W, M, S, O> {
    /// Send a message over the mesh network
    ///
    /// # Arguments
    /// * `dest` - Destination address (use BROADCAST_ADDR for broadcast)
    /// * `data` - Message data to send
    /// * `flag` - Mesh flags (e.g., 0x01 for MESH_DATA_GROUP)
    ///
    /// # Returns
    /// Ok(()) if message sent successfully
    pub fn send_message(
        &self,
        dest: &sys::mesh_addr_t,
        data: &[u8],
        flag: i32,
    ) -> Result<(), sys::EspError> {
        unsafe {
            let mesh_data = sys::mesh_data_t {
                data: data.as_ptr() as *mut u8,
                size: data.len() as u16,
                proto: 0,
                tos: 0,
            };

            match esp!(sys::esp_mesh_send(dest, &mesh_data, flag, ptr::null(), 0)) {
                Ok(_) => {
                    debug!("state::mesh_ops: Mesh message sent: {} bytes", data.len());
                    Ok(())
                }
                Err(err) => {
                    warn!("state::mesh_ops: Failed to send mesh message: error {}", err);
                    Err(err)
                }
            }
        }
    }

    /// Receive a message from the mesh network (blocking with timeout)
    ///
    /// # Arguments
    /// * `timeout_ms` - Timeout in milliseconds
    ///
    /// # Returns
    /// Ok(MeshMessage) if message received, Err if timeout or error
    pub fn recv_message(&self, timeout_ms: u32) -> Result<MeshMessage, sys::EspError> {
        let mut rx_buf = vec![0u8; 1500];
        let mut from_addr = sys::mesh_addr_t { addr: [0; 6] };
        let mut flag = 0i32;

        unsafe {
            let mut mesh_data = sys::mesh_data_t {
                data: rx_buf.as_mut_ptr(),
                size: rx_buf.len() as u16,
                proto: 0,
                tos: 0,
            };

            let err = sys::esp_mesh_recv(
                &mut from_addr,
                &mut mesh_data,
                timeout_ms as i32,
                &mut flag,
                ptr::null_mut(),
                0,
            );

            if err == sys::ESP_OK {
                let actual_size = mesh_data.size as usize;
                rx_buf.truncate(actual_size);

                debug!("state::mesh_ops: Mesh message received: {} bytes from {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                    actual_size,
                    from_addr.addr[0], from_addr.addr[1], from_addr.addr[2],
                    from_addr.addr[3], from_addr.addr[4], from_addr.addr[5]);

                Ok(MeshMessage {
                    from_addr,
                    data: rx_buf,
                    flag,
                })
            } else {
                // Don't log timeout as warning (it's expected behavior)
                if err != sys::ESP_ERR_TIMEOUT {
                    warn!("state::mesh_ops: Failed to receive mesh message: error {}", err);
                }
                Err(sys::EspError::from(err).unwrap())
            }
        }
    }

    /// Check if the mesh device is active
    ///
    /// # Returns
    /// true if device is connected to mesh network
    pub fn is_device_active(&self) -> bool {
        unsafe { sys::esp_mesh_is_device_active() }
    }

    /// Get the total number of nodes in the mesh network
    ///
    /// # Returns
    /// Number of nodes currently in the mesh
    pub fn get_total_node_count(&self) -> i32 {
        unsafe { sys::esp_mesh_get_total_node_num() }
    }
}

// =============================================================================
// Global Helper Functions (for use in tasks without state instance)
// =============================================================================

/// Send a message to a specific mesh address (global helper)
pub fn mesh_send(dest: &sys::mesh_addr_t, data: &[u8], flag: i32) -> Result<(), sys::EspError> {
    unsafe {
        let mesh_data = sys::mesh_data_t {
            data: data.as_ptr() as *mut u8,
            size: data.len() as u16,
            proto: 0,
            tos: 0,
        };

        let err = sys::esp_mesh_send(dest, &mesh_data, flag, ptr::null(), 0);

        if err == sys::ESP_OK {
            debug!("state::mesh_ops: Mesh message sent: {} bytes", data.len());
            Ok(())
        } else {
            warn!("state::mesh_ops: Failed to send mesh message: error {}", err);
            Err(sys::EspError::from(err).unwrap())
        }
    }
}

/// Send a broadcast message to all mesh nodes (global helper)
pub fn send_broadcast(data: &[u8], flag: i32) -> Result<(), sys::EspError> {
    mesh_send(&BROADCAST_ADDR, data, flag)
}

/// Receive a message from the mesh network (global helper, blocking with timeout)
pub fn mesh_recv(timeout_ms: u32) -> Result<MeshMessage, sys::EspError> {
    let mut rx_buf = vec![0u8; 1500];
    let mut from_addr = sys::mesh_addr_t { addr: [0; 6] };
    let mut flag = 0i32;

    unsafe {
        let mut mesh_data = sys::mesh_data_t {
            data: rx_buf.as_mut_ptr(),
            size: rx_buf.len() as u16,
            proto: 0,
            tos: 0,
        };

        let err = sys::esp_mesh_recv(
            &mut from_addr,
            &mut mesh_data,
            timeout_ms as i32,
            &mut flag,
            ptr::null_mut(),
            0,
        );

        if err == sys::ESP_OK {
            let actual_size = mesh_data.size as usize;
            rx_buf.truncate(actual_size);

            debug!("state::mesh_ops: Mesh message received: {} bytes from {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                actual_size,
                from_addr.addr[0], from_addr.addr[1], from_addr.addr[2],
                from_addr.addr[3], from_addr.addr[4], from_addr.addr[5]);

            Ok(MeshMessage {
                from_addr,
                data: rx_buf,
                flag,
            })
        } else {
            // Don't log timeout as warning (it's expected behavior)
            if err != sys::ESP_ERR_TIMEOUT {
                warn!("state::mesh_ops: Failed to receive mesh message: error {}", err);
            }
            Err(sys::EspError::from(err).unwrap())
        }
    }
}

/// Check if the mesh device is active (global helper)
pub fn is_mesh_active() -> bool {
    unsafe { sys::esp_mesh_is_device_active() }
}

/// Get the total number of nodes in the mesh network (global helper)
pub fn get_mesh_node_count() -> i32 {
    unsafe { sys::esp_mesh_get_total_node_num() }
}


/// Mesh event handler callback for WiFi and mesh events
pub unsafe extern "C" fn mesh_event_handler(
    _arg: *mut c_void,
    event_base: sys::esp_event_base_t,
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

                // Update global state with mesh layer info
                if let Some(state) = GLOBAL_STATE.lock().unwrap().as_mut() {
                    let _ = state.refresh_from_mesh();
                }

                // If this node is root, start DHCP client immediately
                if esp_mesh_is_root() {
                    info!("Mesh event: Node is now root - starting DHCP client for external IP");

                    // Get STA netif from global state
                    let sta_netif_opt = GLOBAL_STATE.lock().unwrap().as_ref().and_then(|s| s.sta_netif());

                    if let Some(sta_netif) = sta_netif_opt {
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
                        "Mesh event: Parent disconnected, reason: {}",
                        reason,
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

                // Update global state
                if let Some(state) = GLOBAL_STATE.lock().unwrap().as_mut() {
                    state.set_has_ip(true);
                }

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

                // Update global state
                if let Some(state) = GLOBAL_STATE.lock().unwrap().as_mut() {
                    state.set_has_ip(false);
                }
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