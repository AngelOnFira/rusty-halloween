//! Mesh lifecycle operations

use super::{types::*, wifi::ScanResults, GLOBAL_STATE};
use esp_idf_svc::sys as sys;
use std::marker::PhantomData;

// Mesh network configuration constants
pub const MESH_ID: [u8; 6] = [0x12, 0x34, 0x56, 0x78, 0x90, 0xAB];
pub const MESH_PASSWORD: &str = "mesh_password_123";
pub const MESH_MAX_LAYER: i32 = 6;
pub const MESH_AP_CONNECTIONS: i32 = 6;

/// Mesh configuration
pub struct MeshConfig {
    pub mesh_id: [u8; 6],
    pub channel: u8,
    pub router_ssid: String,
    pub router_password: String,
    pub max_connections: u8,
}

impl<O> WifiMeshState<Sta, MeshInactive, NotScanning, O> {
    /// Start mesh network from STA mode.
    /// Automatically transitions to STAAP mode and initializes mesh.
    pub fn start_mesh(self, config: MeshConfig) -> Result<MeshRunningState, sys::EspError> {
        info!("state::mesh: Starting mesh network (auto-transition STA -> STAAP -> mesh active)");

        // Step 1: Transition to STAAP mode (required for mesh)
        let staap_state = self.set_staap_mode()?;

        // Step 2: Start mesh from STAAP state
        staap_state.start_mesh(config)
    }
}

impl<O> WifiMeshState<StaAp, MeshInactive, NotScanning, O> {
    /// Start mesh network.
    /// WiFi must already be in STAAP mode.
    pub fn start_mesh(mut self, config: MeshConfig) -> Result<MeshRunningState, sys::EspError> {
        info!("state::mesh: Starting mesh network (already in STAAP mode)");

        unsafe {
            // Initialize mesh if not already done
            sys::esp!(sys::esp_mesh_init())?;
            info!("state::mesh: Mesh initialized");

            // Prepare router configuration
            let mut router_ssid = [0u8; 32];
            router_ssid[..config.router_ssid.len()].copy_from_slice(config.router_ssid.as_bytes());

            let mut router_password = [0u8; 64];
            router_password[..config.router_password.len()].copy_from_slice(config.router_password.as_bytes());

            let router = sys::mesh_router_t {
                ssid: router_ssid,
                ssid_len: config.router_ssid.len() as u8,
                bssid: [0u8; 6],
                password: router_password,
                allow_router_switch: false,
            };

            // Prepare mesh AP configuration
            let mesh_ap = sys::mesh_ap_cfg_t {
                password: [0u8; 64],
                max_connection: config.max_connections,
                nonmesh_max_connection: 0,
            };

            // Configure mesh
            let mesh_cfg = sys::mesh_cfg_t {
                channel: config.channel,
                allow_channel_switch: false,
                mesh_id: sys::mesh_addr_t {
                    addr: config.mesh_id,
                },
                router,
                mesh_ap,
                crypto_funcs: core::ptr::null(),
            };

            sys::esp!(sys::esp_mesh_set_config(&mesh_cfg))?;
            info!("state::mesh: Mesh configured with channel {}", config.channel);

            // Start mesh
            sys::esp!(sys::esp_mesh_start())?;
            info!("state::mesh: Mesh started");

            // Enable self-organized mode
            sys::esp!(sys::esp_mesh_set_self_organized(true, false))?;
            info!("state::mesh: Mesh self-organized mode enabled");

            sys::esp!(sys::esp_mesh_connect())?;
            info!("state::mesh: Mesh connected");
        }

        // Update runtime state and global state
        RuntimeState::with_mut(|runtime| {
            runtime.mesh_id = config.mesh_id;
            runtime.current_channel = config.channel;
        });

        if let Some(state) = GLOBAL_STATE.lock().unwrap().as_mut() {
            state.mesh_state = MeshStateRuntime::SelfOrganized;
        }

        Ok(WifiMeshState {
            _wifi_mode: PhantomData,
            _mesh_state: PhantomData,
            _scan_state: PhantomData,
            _ota_state: PhantomData,
        })
    }
}

impl<O> WifiMeshState<StaAp, MeshSelfOrganized, NotScanning, O> {
    /// Disable self-organized mode to allow manual WiFi operations.
    ///
    /// Per ESP-MESH programming model: "When using ESP-WIFI-MESH under self-organized
    /// mode, users must ensure that no calls to Wi-Fi API are made."
    ///
    /// Call this before any WiFi operations, then re-enable after.
    pub fn disable_self_organized(self) -> Result<MeshManualState, sys::EspError> {
        info!("state::mesh: Disabling mesh self-organized mode for WiFi operations");

        unsafe {
            sys::esp!(sys::esp_mesh_set_self_organized(false, false))?;
        }

        // Update global state
        if let Some(state) = GLOBAL_STATE.lock().unwrap().as_mut() {
            state.mesh_state = MeshStateRuntime::Active;
        }

        Ok(WifiMeshState {
            _wifi_mode: PhantomData,
            _mesh_state: PhantomData,
            _scan_state: PhantomData,
            _ota_state: PhantomData,
        })
    }

    /// Stop mesh network and return to inactive state.
    pub fn stop_mesh(self) -> Result<MeshReadyState, sys::EspError> {
        info!("state::mesh: Stopping mesh network");

        unsafe {
            // Disable self-organized mode first
            sys::esp!(sys::esp_mesh_set_self_organized(false, false))?;

            // Stop mesh
            sys::esp!(sys::esp_mesh_stop())?;

            // Deinitialize mesh
            sys::esp!(sys::esp_mesh_deinit())?;

            info!("state::mesh: Mesh stopped and deinitialized");
        }

        // Reset runtime state fields
        RuntimeState::with_mut(|runtime| {
            runtime.is_root = false;
            runtime.layer = -1;
        });

        // Update global state
        if let Some(state) = GLOBAL_STATE.lock().unwrap().as_mut() {
            state.mesh_state = MeshStateRuntime::Inactive;
        }

        Ok(WifiMeshState {
            _wifi_mode: PhantomData,
            _mesh_state: PhantomData,
            _scan_state: PhantomData,
            _ota_state: PhantomData,
        })
    }
}

impl<O> WifiMeshState<StaAp, MeshActive, NotScanning, O> {
    /// Re-enable self-organized mode after manual WiFi operations.
    pub fn enable_self_organized(self) -> Result<MeshRunningState, sys::EspError> {
        info!("state::mesh: Re-enabling mesh self-organized mode");

        unsafe {
            sys::esp!(sys::esp_mesh_set_self_organized(true, false))?;
        }

        // Update global state
        if let Some(state) = GLOBAL_STATE.lock().unwrap().as_mut() {
            state.mesh_state = MeshStateRuntime::SelfOrganized;
        }

        Ok(WifiMeshState {
            _wifi_mode: PhantomData,
            _mesh_state: PhantomData,
            _scan_state: PhantomData,
            _ota_state: PhantomData,
        })
    }

    /// Scan while mesh is in manual mode (self-organized disabled).
    /// Temporarily switches to STA mode, scans, then returns to STAAP + manual mesh.
    pub fn scan(self) -> Result<(ScanResults, Self), sys::EspError> {
        info!("state::mesh: Scanning in mesh manual mode (will temporarily switch to STA)");

        // Transition to STA for scanning
        let sta_state: WifiMeshState<Sta, MeshActive, NotScanning, O> = WifiMeshState {
            _wifi_mode: PhantomData,
            _mesh_state: PhantomData,
            _scan_state: PhantomData,
            _ota_state: PhantomData,
        };

        // Set mode
        unsafe {
            sys::esp!(sys::esp_wifi_set_mode(sys::wifi_mode_t_WIFI_MODE_STA))?;
        }

        // Perform scan
        let (results, sta_state) = sta_state.scan_in_mesh_manual()?;

        // Switch back to STAAP
        unsafe {
            sys::esp!(sys::esp_wifi_set_mode(sys::wifi_mode_t_WIFI_MODE_APSTA))?;
        }

        let staap_state = WifiMeshState {
            _wifi_mode: PhantomData,
            _mesh_state: PhantomData,
            _scan_state: PhantomData,
            _ota_state: PhantomData,
        };

        Ok((results, staap_state))
    }

    /// Stop mesh network.
    pub fn stop_mesh(self) -> Result<MeshReadyState, sys::EspError> {
        info!("state::mesh: Stopping mesh from manual mode");

        unsafe {
            sys::esp!(sys::esp_mesh_stop())?;
            sys::esp!(sys::esp_mesh_deinit())?;
        }

        // Reset runtime state fields
        RuntimeState::with_mut(|runtime| {
            runtime.is_root = false;
            runtime.layer = -1;
        });

        // Update global state
        if let Some(state) = GLOBAL_STATE.lock().unwrap().as_mut() {
            state.mesh_state = MeshStateRuntime::Inactive;
        }

        Ok(WifiMeshState {
            _wifi_mode: PhantomData,
            _mesh_state: PhantomData,
            _scan_state: PhantomData,
            _ota_state: PhantomData,
        })
    }
}

// Helper for scanning while mesh is active but in manual mode
impl<O> WifiMeshState<Sta, MeshActive, NotScanning, O> {
    fn scan_in_mesh_manual(self) -> Result<(ScanResults, Self), sys::EspError> {
        info!("state::mesh: Performing scan in mesh manual mode");

        unsafe {
            sys::esp_wifi_scan_stop();

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
                    ghz_2_channels: 0xFFFF,
                    ghz_5_channels: 0,
                },
            };

            sys::esp!(sys::esp_wifi_scan_start(&scan_config, true))?;

            let mut ap_count: u16 = 0;
            sys::esp!(sys::esp_wifi_scan_get_ap_num(&mut ap_count))?;

            let mut aps: Vec<sys::wifi_ap_record_t> = vec![
                std::mem::zeroed();
                ap_count as usize
            ];

            let mut actual_count = ap_count;
            sys::esp!(sys::esp_wifi_scan_get_ap_records(&mut actual_count, aps.as_mut_ptr()))?;
            aps.truncate(actual_count as usize);

            let results = ScanResults {
                aps,
                count: actual_count as usize,
            };

            Ok((results, self))
        }
    }
}

