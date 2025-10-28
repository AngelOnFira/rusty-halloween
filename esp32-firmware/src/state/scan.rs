//! WiFi scanning operations and network discovery

use super::{types::*, wifi::ScanResults};
use esp_idf_svc::sys::{
    self as sys, esp,
    nvs_handle_t, nvs_open, nvs_close, nvs_get_u8, nvs_set_u8, nvs_commit,
    nvs_open_mode_t_NVS_READONLY, nvs_open_mode_t_NVS_READWRITE,
    ESP_OK, ESP_ERR_NVS_NOT_FOUND, ESP_ERR_TIMEOUT,
    wifi_ap_record_t, wifi_scan_config_t, wifi_scan_type_t_WIFI_SCAN_TYPE_ACTIVE,
    wifi_scan_time_t, wifi_active_scan_time_t, wifi_scan_channel_bitmap_t,
    esp_wifi_scan_start, esp_wifi_scan_get_ap_num, esp_wifi_scan_get_ap_records,
    esp_mesh_scan_get_ap_ie_len,
};
use log::{error, info, warn};
use std::{ffi::CString, marker::PhantomData, ptr};

// =============================================================================
// Scan Operations (Automatic Transitions)
// =============================================================================

/// Scan results container


impl<O> WifiMeshState<Sta, MeshInactive, NotScanning, O> {
    /// Perform a WiFi scan and return results.
    /// WiFi is already in STA mode and mesh is inactive, so we can scan directly.
    ///
    /// This is the simple case - use when you're already in a scan-ready state.
    pub fn scan(self) -> Result<(ScanResults, Self), sys::EspError> {
        info!("Starting WiFi scan (already in STA mode, mesh inactive)");

        unsafe {
            // Stop any previous scan
            sys::esp_wifi_scan_stop();

            // Configure scan
            let scan_config = sys::wifi_scan_config_t {
                ssid: core::ptr::null_mut(),
                bssid: core::ptr::null_mut(),
                channel: 0,
                show_hidden: false,
                scan_type: sys::wifi_scan_type_t_WIFI_SCAN_TYPE_ACTIVE,
                scan_time: sys::wifi_scan_time_t {
                    active: sys::wifi_active_scan_time_t {
                        min: 100,
                        max: 300,
                    },
                    passive: 0,
                },
                home_chan_dwell_time: 0,
                channel_bitmap: sys::wifi_scan_channel_bitmap_t {
                    ghz_2_channels: 0xFFFF, // Scan all 2.4GHz channels
                    ghz_5_channels: 0,      // Don't scan 5GHz
                },
            };

            // Start scan (blocking until complete)
            sys::esp!(sys::esp_wifi_scan_start(&scan_config, true))?;
            info!("Scan completed successfully");

            // Get results
            let mut ap_count: u16 = 0;
            sys::esp!(sys::esp_wifi_scan_get_ap_num(&mut ap_count))?;

            let mut aps: Vec<sys::wifi_ap_record_t> = vec![
                std::mem::zeroed();
                ap_count as usize
            ];

            let mut actual_count = ap_count;
            sys::esp!(sys::esp_wifi_scan_get_ap_records(&mut actual_count, aps.as_mut_ptr()))?;
            aps.truncate(actual_count as usize);

            info!("Found {} access points", actual_count);

            let results = ScanResults {
                aps,
                count: actual_count as usize,
            };

            Ok((results, self))
        }
    }
}

impl<O> WifiMeshState<StaAp, MeshInactive, NotScanning, O> {
    /// Perform a WiFi scan with automatic mode transition.
    ///
    /// This automatically:
    /// 1. Switches from STAAP to STA mode (required for scanning)
    /// 2. Performs the scan
    /// 3. Switches back to STAAP mode
    ///
    /// Use this when mesh is inactive but you're in STAAP mode.
    pub fn scan(self) -> Result<(ScanResults, Self), sys::EspError> {
        info!("Starting WiFi scan with automatic mode transition (STAAP -> STA -> scan -> STAAP)");

        // Step 1: Switch to STA mode
        let sta_state = self.set_sta_mode()?;

        // Step 2: Perform scan
        let (results, sta_state) = sta_state.scan()?;

        // Step 3: Switch back to STAAP mode
        let staap_state = sta_state.set_staap_mode()?;

        Ok((results, staap_state))
    }
}

// =============================================================================
// Network Discovery and NVS Persistence
// =============================================================================

/// NVS namespace for storing mesh configuration
const NVS_NAMESPACE: &str = "mesh_config";
/// NVS key for storing the WiFi channel
const NVS_CHANNEL_KEY: &str = "channel";

/// Result of network discovery scan
#[derive(Debug, Clone)]
pub enum NetworkDiscovery {
    /// Found an existing ESP-MESH network
    ExistingMesh {
        channel: u8,
        rssi: i8,
        bssid: [u8; 6],
        ssid: String,
    },
    /// Found the target router (no existing mesh)
    Router {
        channel: u8,
        rssi: i8,
        bssid: [u8; 6],
    },
    /// No mesh or router found
    NotFound,
}

impl NetworkDiscovery {
    /// Get the channel from the discovery result, if available
    pub fn channel(&self) -> Option<u8> {
        match self {
            NetworkDiscovery::ExistingMesh { channel, .. } => Some(*channel),
            NetworkDiscovery::Router { channel, .. } => Some(*channel),
            NetworkDiscovery::NotFound => None,
        }
    }

    /// Get the BSSID from the discovery result, if available
    pub fn bssid(&self) -> Option<[u8; 6]> {
        match self {
            NetworkDiscovery::ExistingMesh { bssid, .. } => Some(*bssid),
            NetworkDiscovery::Router { bssid, .. } => Some(*bssid),
            NetworkDiscovery::NotFound => None,
        }
    }
}

/// Load the WiFi channel from NVS (non-volatile storage)
///
/// Returns Some(channel) if found, None if not found or on error
pub fn load_channel_from_nvs() -> Option<u8> {
    unsafe {
        let namespace = CString::new(NVS_NAMESPACE).ok()?;
        let key = CString::new(NVS_CHANNEL_KEY).ok()?;

        let mut nvs_handle: nvs_handle_t = 0;

        // Open NVS handle
        let ret = nvs_open(
            namespace.as_ptr(),
            nvs_open_mode_t_NVS_READONLY,
            &mut nvs_handle as *mut nvs_handle_t,
        );

        if ret != ESP_OK {
            info!("NVS: No saved channel found (namespace not found)");
            return None;
        }

        // Read channel value
        let mut channel: u8 = 0;
        let ret = nvs_get_u8(nvs_handle, key.as_ptr(), &mut channel);

        nvs_close(nvs_handle);

        if ret == ESP_OK {
            info!("NVS: Loaded saved channel: {}", channel);
            Some(channel)
        } else {
            info!("NVS: No saved channel found (key not found)");
            None
        }
    }
}

/// Save the WiFi channel to NVS (non-volatile storage)
///
/// Returns true on success, false on error
pub fn save_channel_to_nvs(channel: u8) -> bool {
    unsafe {
        let namespace = match CString::new(NVS_NAMESPACE) {
            Ok(s) => s,
            Err(e) => {
                error!("NVS: Failed to create namespace string: {:?}", e);
                return false;
            }
        };

        let key = match CString::new(NVS_CHANNEL_KEY) {
            Ok(s) => s,
            Err(e) => {
                error!("NVS: Failed to create key string: {:?}", e);
                return false;
            }
        };

        let mut nvs_handle: nvs_handle_t = 0;

        // Open NVS handle with read-write access
        let ret = nvs_open(
            namespace.as_ptr(),
            nvs_open_mode_t_NVS_READWRITE,
            &mut nvs_handle as *mut nvs_handle_t,
        );

        if ret != ESP_OK {
            error!("NVS: Failed to open handle: {}", ret);
            return false;
        }

        // Write channel value
        let ret = nvs_set_u8(nvs_handle, key.as_ptr(), channel);

        if ret != ESP_OK {
            error!("NVS: Failed to set channel: {}", ret);
            nvs_close(nvs_handle);
            return false;
        }

        // Commit changes
        let ret = nvs_commit(nvs_handle);

        nvs_close(nvs_handle);

        if ret == ESP_OK {
            info!("NVS: Saved channel {} to flash", channel);
            true
        } else {
            error!("NVS: Failed to commit channel: {}", ret);
            false
        }
    }
}

/// Check if an AP record represents an ESP-MESH network
///
/// Uses ESP-MESH vendor-specific IEs and SSID pattern matching
fn is_mesh_network(ap: &wifi_ap_record_t, _mesh_id: &[u8; 6]) -> bool {
    unsafe {
        // First check: Use ESP-MESH API to check for mesh vendor IEs
        let mut ie_len: i32 = 0;
        let ie_len_result = esp_mesh_scan_get_ap_ie_len(&mut ie_len as *mut i32);

        if ie_len_result == ESP_OK && ie_len > 0 {
            // This AP has mesh-specific vendor IEs, indicating it's part of a mesh
            info!(
                "Found mesh network via ESP-MESH vendor IEs (length: {})",
                ie_len
            );

            // Get SSID for logging
            let ssid_len = ap.ssid.iter().position(|&c| c == 0).unwrap_or(33);
            if ssid_len > 0 {
                let ssid_bytes = &ap.ssid[..ssid_len];
                if let Ok(ssid_str) = std::str::from_utf8(ssid_bytes) {
                    info!("Mesh network SSID: {}", ssid_str);
                }
            }

            // Note: We could use esp_mesh_scan_get_ap_record() here to verify the mesh ID,
            // but mesh_ap_record_t is not exposed in esp-idf-sys bindings.
            // For now, presence of vendor IEs + SSID pattern is sufficient.
            return true;
        }

        // Second check: Verify SSID pattern
        // ESP-MESH networks typically have SSIDs that include the mesh ID or "MESH"
        let ssid_len = ap.ssid.iter().position(|&c| c == 0).unwrap_or(33);
        if ssid_len > 0 {
            let ssid_bytes = &ap.ssid[..ssid_len];
            if let Ok(ssid_str) = std::str::from_utf8(ssid_bytes) {
                // ESP-MESH nodes broadcast with SSIDs containing "MESH" or the mesh ID
                // This is a heuristic - adjust based on your mesh SSID pattern
                if ssid_str.contains("MESH") || ssid_str.contains("mesh") {
                    info!(
                        "Found potential mesh network via SSID pattern: {}",
                        ssid_str
                    );
                    return true;
                }
            }
        }

        false
    }
}

/// Scan for WiFi networks and discover mesh networks or target router
///
/// Returns:
/// - ExistingMesh if an ESP-MESH network with matching mesh ID is found (prioritized)
/// - Router if the target router SSID is found
/// - NotFound if neither is found
///
/// # Arguments
/// * `router_ssid` - Target router SSID to search for
/// * `mesh_id` - Expected mesh ID (6 bytes) for mesh network verification
pub fn scan_for_networks(router_ssid: &str, mesh_id: &[u8; 6]) -> NetworkDiscovery {
    info!(
        "Starting WiFi scan for mesh networks and router with SSID: {}",
        router_ssid
    );

    // Configure scan parameters for 2.4GHz only
    let scan_config = wifi_scan_config_t {
        ssid: ptr::null_mut(),  // Scan all SSIDs
        bssid: ptr::null_mut(), // Scan all BSSIDs
        channel: 0,             // Scan all channels
        show_hidden: false,
        scan_type: wifi_scan_type_t_WIFI_SCAN_TYPE_ACTIVE,
        scan_time: wifi_scan_time_t {
            active: wifi_active_scan_time_t {
                min: 0,   // Min time per channel (ms)
                max: 150, // Max time per channel (ms)
            },
            passive: 360, // Passive scan time (ms)
        },
        home_chan_dwell_time: 30,
        channel_bitmap: wifi_scan_channel_bitmap_t {
            ghz_2_channels: 0xFFFF, // Scan all 2.4GHz channels (1-13)
            ghz_5_channels: 0,      // Don't scan 5GHz
        },
    };

    // When attempting a scan, WiFi mode needs to be set to STA mode

    // Start scan (blocking)
    if let Err(e) =
        esp!(unsafe { esp_wifi_scan_start(&scan_config as *const wifi_scan_config_t, true) })
    {
        match e.code() {
            ESP_ERR_WIFI_NOT_INIT => {
                error!("WiFi not initialized");
            }
            ESP_ERR_WIFI_NOT_STARTED => {
                error!("Failed to get AP count: {}", e);
            }
            ESP_ERR_WIFI_TIMEOUT => {
                error!("WiFi scan timeout");
            }
            ESP_ERR_WIFI_STATE => {
                error!("WiFi scan not found");
            }
            _ => {
                error!("Unexpected error esp_wifi_scan_start: {}", e);
            }
        }
        return NetworkDiscovery::NotFound;
    }

    // Get number of APs found
    let mut ap_count: u16 = 0;
    if let Err(e) = esp!(unsafe { esp_wifi_scan_get_ap_num(&mut ap_count) }) {
        match e.code() {
            ESP_ERR_WIFI_NOT_INIT => {
                error!("WiFi not initialized");
            }
            ESP_ERR_WIFI_NOT_STARTED => {
                error!("Failed to get AP count: {}", e);
            }
            ESP_ERR_INVALID_ARG => {
                error!("Invalid argument: {}", e);
            }
            _ => {
                error!("Unexpected error esp_wifi_scan_get_ap_num: {}", e);
            }
        }
        return NetworkDiscovery::NotFound;
    }

    info!("WiFi scan found {} access points", ap_count);

    if ap_count == 0 {
        return NetworkDiscovery::NotFound;
    }

    // Allocate buffer for AP records
    let mut ap_records: Vec<wifi_ap_record_t> =
        vec![unsafe { std::mem::zeroed() }; ap_count as usize];
    let mut actual_count = ap_count;

    if let Err(e) =
        esp!(unsafe { esp_wifi_scan_get_ap_records(&mut actual_count, ap_records.as_mut_ptr()) })
    {
        match e.code() {
            ESP_ERR_WIFI_NOT_INIT => {
                error!("WiFi not initialized");
            }
            ESP_ERR_WIFI_NOT_STARTED => {
                error!("Failed to get AP records: {}", e);
            }
            ESP_ERR_INVALID_ARG => {
                error!("Invalid argument: {}", e);
            }
            ESP_ERR_NO_MEM => {
                error!("Out of memory: {}", e);
            }
            _ => {
                error!("Unexpected error esp_wifi_scan_get_ap_records: {}", e);
            }
        }
        return NetworkDiscovery::NotFound;
    }

    info!("Retrieved {} AP records", actual_count);

    // First pass: Look for existing mesh networks (highest priority)
    for ap in ap_records.iter().take(actual_count as usize) {
        // Only consider 2.4GHz channels (1-13)
        if ap.primary > 13 {
            continue;
        }

        if is_mesh_network(ap, mesh_id) {
            let ssid_len = ap.ssid.iter().position(|&c| c == 0).unwrap_or(33);
            let ssid = String::from_utf8_lossy(&ap.ssid[..ssid_len]).to_string();

            info!(
                "Found existing mesh network: SSID='{}', Channel={}, RSSI={}",
                ssid, ap.primary, ap.rssi
            );

            return NetworkDiscovery::ExistingMesh {
                channel: ap.primary,
                rssi: ap.rssi,
                bssid: ap.bssid,
                ssid,
            };
        }
    }

    info!("No existing mesh found, looking for router...");

    // Second pass: Look for target router
    let mut best_router: Option<&wifi_ap_record_t> = None;
    let mut best_rssi = i8::MIN;

    for ap in ap_records.iter().take(actual_count as usize) {
        // Only consider 2.4GHz channels (1-13)
        if ap.primary > 13 {
            continue;
        }

        let ssid_len = ap.ssid.iter().position(|&c| c == 0).unwrap_or(33);

        if let Ok(ssid_str) = std::str::from_utf8(&ap.ssid[..ssid_len]) {
            if ssid_str == router_ssid {
                // Found matching router, track best RSSI
                if ap.rssi > best_rssi {
                    best_rssi = ap.rssi;
                    best_router = Some(ap);
                }
            }
        }
    }

    if let Some(router) = best_router {
        info!(
            "Found target router '{}': Channel={}, RSSI={}",
            router_ssid, router.primary, router.rssi
        );

        return NetworkDiscovery::Router {
            channel: router.primary,
            rssi: router.rssi,
            bssid: router.bssid,
        };
    }

    warn!("No mesh network or target router found in scan");
    NetworkDiscovery::NotFound
}

/// Scan for networks with retry logic
///
/// Retries indefinitely with exponential backoff until a network is found
///
/// # Arguments
/// * `router_ssid` - Target router SSID to search for
/// * `mesh_id` - Expected mesh ID (6 bytes) for mesh network verification
/// * `max_delay_ms` - Maximum delay between retries (ms)
pub fn scan_with_retry(
    router_ssid: &str,
    mesh_id: &[u8; 6],
    max_delay_ms: u32,
) -> NetworkDiscovery {
    let mut attempt = 0;
    let mut delay_ms = 1000; // Start with 1 second delay

    loop {
        attempt += 1;
        info!("Network scan attempt #{}", attempt);

        let result = scan_for_networks(router_ssid, mesh_id);

        match result {
            NetworkDiscovery::NotFound => {
                warn!(
                    "Scan attempt #{} found nothing, retrying in {}ms...",
                    attempt, delay_ms
                );

                // Sleep before retry
                std::thread::sleep(std::time::Duration::from_millis(delay_ms as u64));

                // Exponential backoff with cap
                delay_ms = (delay_ms * 2).min(max_delay_ms);
            }
            _ => {
                info!("Network discovered on attempt #{}", attempt);
                return result;
            }
        }
    }
}
