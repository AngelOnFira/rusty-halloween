//! WiFi initialization and mode transitions
//!
//! This module handles WiFi subsystem initialization and transitions between
//! STA (Station) and STAAP (Station+AP) modes.

use super::{
    types::*,
    GLOBAL_STATE,
};
use esp_idf_svc::sys as sys;
use log::info;
use std::marker::PhantomData;

// =============================================================================
// Initialization
// =============================================================================

impl InitialState {
    /// Create a new uninitialized state machine.
    /// This should be called once during application startup.
    pub fn new() -> Self {
        info!("Creating new WiFi/Mesh state machine");
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
        info!("Initializing WiFi subsystem");

        unsafe {
            // Set WiFi mode to STA for initial state
            sys::esp!(sys::esp_wifi_set_mode(sys::wifi_mode_t_WIFI_MODE_STA))?;
            info!("WiFi mode set to STA");
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
        info!("Transitioning WiFi mode: STA -> STAAP");

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
        info!("Transitioning WiFi mode: STAAP -> STA");

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
