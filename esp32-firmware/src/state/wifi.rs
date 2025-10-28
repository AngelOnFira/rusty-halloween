//! WiFi initialization and mode transitions
//!
//! This module handles WiFi subsystem initialization and transitions between
//! STA (Station) and STAAP (Station+AP) modes.

use crate::state::mesh_ops::mesh_event_handler;

use super::{types::*, GLOBAL_STATE};
use esp_idf_svc::sys;
use esp_idf_sys::*;
use once_cell::sync::Lazy;
use std::{marker::PhantomData, sync::{Arc, Mutex}};

/// Global network interface pointers for DHCP management
/// STA interface used by root node to connect to external router
/// AP interface used for mesh network communication
/// Stored as usize (pointer as integer) for thread safety
pub static STA_NETIF: Lazy<Arc<Mutex<usize>>> = Lazy::new(|| Arc::new(Mutex::new(0)));
pub static AP_NETIF: Lazy<Arc<Mutex<usize>>> = Lazy::new(|| Arc::new(Mutex::new(0)));

// =============================================================================
// Initialization
// =============================================================================

impl InitialState {
    /// Create a new uninitialized state machine.
    /// This should be called once during application startup.
    pub fn new() -> Self {
        info!("state::wifi: Creating new WiFi/Mesh state machine");
        WifiMeshState {
            _wifi_mode: PhantomData,
            _mesh_state: PhantomData,
            _scan_state: PhantomData,
            _ota_state: PhantomData,
            is_root: false,
            layer: -1,
            has_ip: false,
            current_channel: 0,
            mesh_id: [0u8; 6],
            sta_netif: None,
            ap_netif: None,
        }
    }

    /// Initialize WiFi subsystem and transition to STA mode.
    /// This calls esp_wifi_set_mode() to STA.
    ///
    /// Returns: State in STA mode ready for scanning or further configuration
    pub fn initialize_wifi(self) -> Result<ScanCapableState, sys::EspError> {
        info!("state::wifi: Initializing WiFi subsystem");

        unsafe {
            // Initialize NVS first
            info!("state::wifi: Initializing NVS");
            esp!(nvs_flash_init())?;

            info!("state::wifi: Initializing esp_netif");
            esp!(esp_netif_init())?;

            info!("state::wifi: Creating default event loop");
            esp!(esp_event_loop_create_default())?;
        }

        let mut sta_netif: *mut sys::esp_netif_obj = std::ptr::null_mut();
        let mut ap_netif: *mut sys::esp_netif_obj = std::ptr::null_mut();

        unsafe {
            info!("state::wifi: Creating default WiFi/Mesh netifs");
            esp!(sys::esp_netif_create_default_wifi_mesh_netifs(
                &mut sta_netif,
                &mut ap_netif
            ))?;
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
            info!("state::wifi: Initializing WiFi");
            esp!(esp_wifi_init(&cfg))?;
            // esp!(esp_wifi_set_mode(sys::wifi_ps_type_t_WIFI_PS_NONE))?;
            info!("state::wifi: WiFi initialized");
            esp!(esp_wifi_set_storage(sys::wifi_storage_t_WIFI_STORAGE_FLASH))?;
            info!("state::wifi: WiFi storage set to FLASH");
            // info!("WiFi mode set to STA/AP");
            esp!(esp_wifi_start())?;
            info!("state::wifi: WiFi started");

            // Register event handlers
            info!("state::wifi: Registering mesh event handler");
            esp!(esp_event_handler_register(
                MESH_EVENT,
                ESP_EVENT_ANY_ID,
                Some(mesh_event_handler),
                std::ptr::null_mut()
            ))?;

            info!("state::wifi: Registering IP event handler");
            esp!(esp_event_handler_register(
                IP_EVENT,
                ESP_EVENT_ANY_ID,
                Some(mesh_event_handler),
                std::ptr::null_mut()
            ))?;

            // WIFI events
            info!("state::wifi: Registering WIFI event handler");
            esp!(esp_event_handler_register(
                WIFI_EVENT,
                ESP_EVENT_ANY_ID,
                Some(mesh_event_handler),
                std::ptr::null_mut()
            ))?;
        }

        unsafe {
            // Set WiFi mode to STA for initial state
            if let Err(e) = esp!(sys::esp_wifi_set_mode(sys::wifi_mode_t_WIFI_MODE_STA)) {
                match e.code() {
                    esp_idf_sys::ESP_ERR_WIFI_NOT_INIT => {
                        error!("state::wifi: WiFi not initialized");
                    }
                    esp_idf_sys::ESP_ERR_INVALID_ARG => {
                        error!("state::wifi: Invalid argument");
                    }
                    _ => {
                        error!("state::wifi: Failed to set WiFi mode to STA: {}", e);
                    }
                }
            }
            info!("state::wifi: WiFi mode set to STA");
        }

        // Initialize global state container
        let container = StateContainer::new(
            self.is_root,
            self.layer,
            self.has_ip,
            self.current_channel,
            self.mesh_id,
            self.sta_netif,
            self.ap_netif,
            WifiModeRuntime::Sta,
            MeshStateRuntime::Inactive,
            ScanStateRuntime::NotScanning,
            OtaStateRuntime::Idle,
        );

        *GLOBAL_STATE.lock().unwrap() = Some(container);

        Ok(WifiMeshState {
            _wifi_mode: PhantomData,
            _mesh_state: PhantomData,
            _scan_state: PhantomData,
            _ota_state: PhantomData,
            is_root: self.is_root,
            layer: self.layer,
            has_ip: self.has_ip,
            current_channel: self.current_channel,
            mesh_id: self.mesh_id,
            sta_netif: self.sta_netif,
            ap_netif: self.ap_netif,
        })
    }
}

// =============================================================================
// WiFi Mode Transitions
// =============================================================================

impl<M, S, O> WifiMeshState<Sta, M, S, O> {
    /// Transition from STA mode to STAAP (combined) mode.
    /// Required before starting mesh operations.
    pub fn set_staap_mode(self) -> Result<WifiMeshState<StaAp, M, S, O>, sys::EspError> {
        info!("state::wifi: Transitioning WiFi mode: STA -> STAAP");

        unsafe {
            sys::esp!(sys::esp_wifi_set_mode(sys::wifi_mode_t_WIFI_MODE_APSTA))?;
        }

        // Update global state
        if let Some(state) = GLOBAL_STATE.lock().unwrap().as_mut() {
            state.wifi_mode = WifiModeRuntime::StaAp;
        }

        Ok(WifiMeshState {
            _wifi_mode: PhantomData,
            _mesh_state: PhantomData,
            _scan_state: PhantomData,
            _ota_state: PhantomData,
            is_root: self.is_root,
            layer: self.layer,
            has_ip: self.has_ip,
            current_channel: self.current_channel,
            mesh_id: self.mesh_id,
            sta_netif: self.sta_netif,
            ap_netif: self.ap_netif,
        })
    }
}

impl<M, S, O> WifiMeshState<StaAp, M, S, O> {
    /// Transition from STAAP mode back to STA-only mode.
    /// Required for scanning when mesh is not active.
    pub fn set_sta_mode(self) -> Result<WifiMeshState<Sta, M, S, O>, sys::EspError> {
        info!("state::wifi: Transitioning WiFi mode: STAAP -> STA");

        unsafe {
            sys::esp!(sys::esp_wifi_set_mode(sys::wifi_mode_t_WIFI_MODE_STA))?;
        }

        // Update global state
        if let Some(state) = GLOBAL_STATE.lock().unwrap().as_mut() {
            state.wifi_mode = WifiModeRuntime::Sta;
        }

        Ok(WifiMeshState {
            _wifi_mode: PhantomData,
            _mesh_state: PhantomData,
            _scan_state: PhantomData,
            _ota_state: PhantomData,
            is_root: self.is_root,
            layer: self.layer,
            has_ip: self.has_ip,
            current_channel: self.current_channel,
            mesh_id: self.mesh_id,
            sta_netif: self.sta_netif,
            ap_netif: self.ap_netif,
        })
    }
}

// =============================================================================
// Scan Results
// =============================================================================

/// Scan results container
pub struct ScanResults {
    pub aps: Vec<sys::wifi_ap_record_t>,
    pub count: usize,
}

impl ScanResults {
    /// Find a 2.4GHz network matching the given SSID
    pub fn find_2ghz_network(&self, ssid: &str) -> Option<&sys::wifi_ap_record_t> {
        self.aps.iter().find(|ap| {
            let ap_ssid = std::ffi::CStr::from_bytes_until_nul(&ap.ssid)
                .ok()
                .and_then(|s| s.to_str().ok())
                .unwrap_or("");
            ap_ssid == ssid
        })
    }

    /// Get the channel of a specific AP
    pub fn get_channel(&self, ssid: &str) -> Option<u8> {
        self.find_2ghz_network(ssid).map(|ap| ap.primary)
    }
}
