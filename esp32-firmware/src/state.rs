//! Typestate-based WiFi and Mesh state machine
//!
//! This module provides a compile-time safe state machine for managing WiFi and ESP-MESH
//! states. It prevents invalid state combinations (like scanning while mesh is active)
//! and provides automatic transition methods that handle multi-step sequences.
//!
//! # Design
//!
//! The state machine uses the typestate pattern with four independent state dimensions:
//! - **WiFi Mode**: `Uninitialized`, `Sta`, `Ap`, `StaAp`
//! - **Mesh State**: `MeshInactive`, `MeshActive`, `MeshSelfOrganized`
//! - **Scan State**: `NotScanning`, `Scanning`
//! - **OTA State**: `OtaIdle`, `OtaActive`
//!
//! Each state combination is a distinct type, and only valid transitions are provided as methods.
//!
//! # ESP-MESH Programming Model Constraints
//!
//! This module encodes the constraints from the ESP-WIFI-MESH Programming Guide:
//! - WiFi API calls are forbidden when self-organized mode is active
//! - Scanning requires mesh to be inactive or self-organized mode disabled
//! - Scanning requires WiFi to be in STA mode (automatically handled)
//! - AP mode conflicts with scanning
//!
//! # Examples
//!
//! ## Basic Initialization and Mesh Startup
//!
//! ```rust,ignore
//! use state::{InitialState, MeshConfig, global_state};
//!
//! // Initialize the state machine
//! let state = InitialState::new();
//! let state = state.initialize_wifi()?;
//!
//! // Scan for networks to find the router channel
//! let (scan_results, state) = state.scan()?;
//! let channel = scan_results.get_channel("MyRouter").unwrap();
//!
//! // Start mesh network (auto-transitions to STAAP mode)
//! let config = MeshConfig {
//!     mesh_id: [0x77, 0x77, 0x77, 0x77, 0x77, 0x77],
//!     channel,
//!     router_ssid: "MyRouter".to_string(),
//!     router_password: "password".to_string(),
//!     max_connections: 10,
//! };
//! let state = state.start_mesh(config)?;
//!
//! // Mesh is now running with self-organized mode enabled
//! info!("Mesh started on channel {}", state.channel());
//! ```
//!
//! ## Scanning While Mesh Is Active
//!
//! Per ESP-MESH programming model, you must disable self-organized mode before
//! making WiFi API calls:
//!
//! ```rust,ignore
//! // Mesh is running in self-organized mode
//! let running_state = /* ... */;
//!
//! // Disable self-organized mode for WiFi operations
//! let manual_state = running_state.disable_self_organized()?;
//!
//! // Now safe to scan (auto-handles STA mode transition)
//! let (results, manual_state) = manual_state.scan()?;
//!
//! // Re-enable self-organized mode
//! let running_state = manual_state.enable_self_organized()?;
//! ```
//!
//! ## Using Global State in Event Handlers
//!
//! ```rust,ignore
//! use state::{global_state, has_ip, is_root, layer};
//!
//! // Quick queries without holding state machine
//! if is_root() {
//!     info!("This node is the root, layer: {}", layer());
//!     if has_ip() {
//!         // Can communicate with external network
//!     }
//! }
//!
//! // Update state from event handlers
//! if let Some(state) = global_state().lock().unwrap().as_mut() {
//!     state.set_has_ip(true);
//!     state.refresh_from_mesh()?;
//! }
//! ```
//!
//! ## OTA Operations
//!
//! ```rust,ignore
//! // Start OTA (can only be called when not scanning)
//! let ota_state = mesh_state.start_ota();
//!
//! // Perform OTA download and distribution...
//!
//! // Finish OTA
//! let mesh_state = ota_state.finish_ota();
//! ```

use esp_idf_svc::sys as sys;
use log::{info, debug};
use std::marker::PhantomData;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

// =============================================================================
// Marker Types (Zero-Sized Types for Compile-Time State)
// =============================================================================

// WiFi Mode States
/// WiFi subsystem not yet initialized
pub struct Uninitialized;

/// WiFi in Station mode only - can scan, connect to AP
pub struct Sta;

/// WiFi in Access Point mode only - provides AP for others
pub struct Ap;

/// WiFi in combined Station + Access Point mode - required for mesh
pub struct StaAp;

// Mesh States
/// Mesh is not active - full WiFi API access available
pub struct MeshInactive;

/// Mesh is active but self-organized mode is disabled - limited WiFi API access
pub struct MeshActive;

/// Mesh is active with self-organized mode enabled - NO WiFi API access allowed
pub struct MeshSelfOrganized;

// Scan States
/// No scan in progress - can initiate scan or other operations
pub struct NotScanning;

/// WiFi scan in progress - must wait for completion
pub struct Scanning;

// OTA States
/// No OTA operation in progress
pub struct OtaIdle;

/// Root node: Downloading firmware from GitHub
pub struct OtaDownloading;

/// Root node: Distributing firmware chunks to mesh nodes
pub struct OtaDistributing;

/// Child node: Receiving firmware chunks from root
pub struct OtaReceiving;

/// Firmware received and validated, ready to reboot
pub struct OtaReadyToReboot;

/// OTA download or distribution in progress (generic - kept for compatibility)
pub struct OtaActive;

// =============================================================================
// Core State Machine Struct
// =============================================================================

/// Core WiFi/Mesh state machine with typestate pattern.
///
/// Type parameters encode the current state at compile time:
/// - `W`: WiFi mode (Uninitialized, Sta, Ap, StaAp)
/// - `M`: Mesh state (MeshInactive, MeshActive, MeshSelfOrganized)
/// - `S`: Scan state (NotScanning, Scanning)
/// - `O`: OTA state (OtaIdle, OtaActive)
///
/// Runtime state supplements compile-time guarantees with dynamic information
/// that can only be determined at runtime (root status, layer, etc.).
pub struct WifiMeshState<W, M, S, O> {
    // Compile-time state markers (zero runtime cost)
    _wifi_mode: PhantomData<W>,
    _mesh_state: PhantomData<M>,
    _scan_state: PhantomData<S>,
    _ota_state: PhantomData<O>,

    // Runtime state that supplements compile-time state
    is_root: bool,
    layer: i32,
    has_ip: bool,
    current_channel: u8,
    mesh_id: [u8; 6],

    // Network interface handles (Some when initialized)
    sta_netif: Option<*mut sys::esp_netif_t>,
    ap_netif: Option<*mut sys::esp_netif_t>,
}

// Implement Send + Sync since we're managing raw pointers safely
unsafe impl<W, M, S, O> Send for WifiMeshState<W, M, S, O> {}
unsafe impl<W, M, S, O> Sync for WifiMeshState<W, M, S, O> {}

// =============================================================================
// Type Aliases for Common States
// =============================================================================

/// Initial state before any initialization
pub type InitialState = WifiMeshState<Uninitialized, MeshInactive, NotScanning, OtaIdle>;

/// State ready for WiFi scanning (STA mode, mesh inactive)
pub type ScanCapableState = WifiMeshState<Sta, MeshInactive, NotScanning, OtaIdle>;

/// State with scan in progress
pub type ScanningState = WifiMeshState<Sta, MeshInactive, Scanning, OtaIdle>;

/// State ready for mesh operations (STAAP mode, mesh inactive)
pub type MeshReadyState = WifiMeshState<StaAp, MeshInactive, NotScanning, OtaIdle>;

/// State with mesh running and self-organized mode enabled
pub type MeshRunningState = WifiMeshState<StaAp, MeshSelfOrganized, NotScanning, OtaIdle>;

/// State with mesh active but self-organized disabled (can do limited WiFi ops)
pub type MeshManualState = WifiMeshState<StaAp, MeshActive, NotScanning, OtaIdle>;

// =============================================================================
// OTA Runtime Data
// =============================================================================

/// Runtime data for OTA operations (kept separate from compile-time state markers)
pub struct OtaRuntimeData {
    /// Progress tracking (bytes downloaded/received)
    pub progress: u32,
    /// Total size (bytes)
    pub total_size: u32,
    /// Firmware download URL (root node only)
    pub firmware_url: Option<String>,
    /// Firmware chunks (root node only - for distribution)
    pub chunks: Vec<crate::ota::FirmwareChunk>,
    /// OTA update handle (child node only - for reception)
    /// Note: Stored as raw pointer because OtaUpdate is not Send/Sync
    pub ota_handle: Option<*mut esp_ota::OtaUpdate>,
    /// Received chunks buffer (child node only - out-of-order chunks)
    pub received_chunks_buffer: HashMap<u32, crate::ota::FirmwareChunk>,
    /// Next expected chunk sequence (child node only)
    pub next_expected_sequence: u32,
    /// Total chunks expected
    pub total_chunks: u32,
    /// Target firmware version string
    pub target_version: Option<String>,
}

impl OtaRuntimeData {
    pub fn new() -> Self {
        Self {
            progress: 0,
            total_size: 0,
            firmware_url: None,
            chunks: Vec::new(),
            ota_handle: None,
            received_chunks_buffer: HashMap::new(),
            next_expected_sequence: 0,
            total_chunks: 0,
            target_version: None,
        }
    }
}

// Safety: OtaUpdate pointer is only used through esp_ota APIs which are thread-safe
unsafe impl Send for OtaRuntimeData {}
unsafe impl Sync for OtaRuntimeData {}

// =============================================================================
// Global State Singleton
// =============================================================================

/// Global singleton for the WiFi/Mesh state machine.
/// This replaces the previous scattered global variables (HAS_IP, STA_NETIF, etc.)
static GLOBAL_STATE: once_cell::sync::Lazy<Arc<Mutex<Option<StateContainer>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(None)));

/// Type-erased container for the state machine (since we can't store generic types in static)
pub struct StateContainer {
    // Runtime state fields (duplicated from WifiMeshState for type-erased access)
    is_root: bool,
    layer: i32,
    has_ip: bool,
    current_channel: u8,
    mesh_id: [u8; 6],
    sta_netif: Option<*mut sys::esp_netif_t>,
    ap_netif: Option<*mut sys::esp_netif_t>,

    // Current state type information (for runtime validation)
    wifi_mode: WifiModeRuntime,
    mesh_state: MeshStateRuntime,
    scan_state: ScanStateRuntime,
    ota_state: OtaStateRuntime,

    // OTA runtime data
    ota_data: OtaRuntimeData,
}

// Safety: The raw pointers in StateContainer are only set during initialization
// and are never dereferenced by the StateContainer itself. They're passed to
// ESP-IDF APIs which handle them correctly. The pointers remain valid for the
// lifetime of the program.
unsafe impl Send for StateContainer {}
unsafe impl Sync for StateContainer {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WifiModeRuntime {
    Uninitialized,
    Sta,
    Ap,
    StaAp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeshStateRuntime {
    Inactive,
    Active,
    SelfOrganized,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanStateRuntime {
    NotScanning,
    Scanning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OtaStateRuntime {
    Idle,
    Downloading,
    Distributing,
    Receiving,
    ReadyToReboot,
}

impl StateContainer {
    /// Query if this node is currently the root
    pub fn is_root(&self) -> bool {
        self.is_root
    }

    /// Query the current mesh layer
    pub fn layer(&self) -> i32 {
        self.layer
    }

    /// Query if this node has an IP address (only root nodes)
    pub fn has_ip(&self) -> bool {
        self.has_ip
    }

    /// Get the current WiFi channel
    pub fn channel(&self) -> u8 {
        self.current_channel
    }

    /// Get the mesh ID
    pub fn mesh_id(&self) -> [u8; 6] {
        self.mesh_id
    }

    /// Get STA netif pointer (for use with ESP-IDF APIs)
    pub fn sta_netif(&self) -> Option<*mut sys::esp_netif_t> {
        self.sta_netif
    }

    /// Get AP netif pointer (for use with ESP-IDF APIs)
    pub fn ap_netif(&self) -> Option<*mut sys::esp_netif_t> {
        self.ap_netif
    }

    /// Update runtime state from ESP-MESH APIs (call from event handlers)
    pub fn refresh_from_mesh(&mut self) -> Result<(), sys::EspError> {
        unsafe {
            // Update root status
            self.is_root = sys::esp_mesh_is_root();

            // Update layer
            self.layer = sys::esp_mesh_get_layer();

            debug!("State refreshed: is_root={}, layer={}", self.is_root, self.layer);
        }
        Ok(())
    }

    /// Set IP status (called from IP event handlers)
    pub fn set_has_ip(&mut self, has_ip: bool) {
        self.has_ip = has_ip;
        info!("IP status updated: has_ip={}", has_ip);
    }

    /// Set root status (called from mesh event handlers)
    pub fn set_is_root(&mut self, is_root: bool) {
        self.is_root = is_root;
        info!("Root status updated: is_root={}", is_root);
    }

    /// Get current OTA state
    pub fn ota_state(&self) -> OtaStateRuntime {
        self.ota_state
    }

    /// Get mutable reference to OTA runtime data
    pub fn ota_data_mut(&mut self) -> &mut OtaRuntimeData {
        &mut self.ota_data
    }

    /// Get immutable reference to OTA runtime data
    pub fn ota_data(&self) -> &OtaRuntimeData {
        &self.ota_data
    }
}

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
    /// This calls esp_wifi_init() and esp_wifi_start().
    ///
    /// Returns: State in STA mode ready for scanning or further configuration
    pub fn initialize_wifi(self) -> Result<ScanCapableState, sys::EspError> {
        info!("Initializing WiFi subsystem");

        unsafe {
            // WiFi should already be initialized by main, but verify
            // Set WiFi mode to STA for initial state
            sys::esp!(sys::esp_wifi_set_mode(sys::wifi_mode_t_WIFI_MODE_STA))?;
            info!("WiFi mode set to STA");
        }

        // Initialize global state container
        let container = StateContainer {
            is_root: self.is_root,
            layer: self.layer,
            has_ip: self.has_ip,
            current_channel: self.current_channel,
            mesh_id: self.mesh_id,
            sta_netif: self.sta_netif,
            ap_netif: self.ap_netif,
            wifi_mode: WifiModeRuntime::Sta,
            mesh_state: MeshStateRuntime::Inactive,
            scan_state: ScanStateRuntime::NotScanning,
            ota_state: OtaStateRuntime::Idle,
            ota_data: OtaRuntimeData::new(),
        };

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
// Global State Access Functions
// =============================================================================

/// Get a reference to the global state container.
/// Returns None if state machine hasn't been initialized yet.
pub fn global_state() -> Arc<Mutex<Option<StateContainer>>> {
    GLOBAL_STATE.clone()
}

/// Helper to check if node is root (for use in event handlers and tasks)
pub fn is_root() -> bool {
    GLOBAL_STATE.lock()
        .unwrap()
        .as_ref()
        .map(|s| s.is_root())
        .unwrap_or(false)
}

/// Helper to check if node has IP (for use in event handlers and tasks)
pub fn has_ip() -> bool {
    GLOBAL_STATE.lock()
        .unwrap()
        .as_ref()
        .map(|s| s.has_ip())
        .unwrap_or(false)
}

/// Helper to get current layer (for use in event handlers and tasks)
pub fn layer() -> i32 {
    GLOBAL_STATE.lock()
        .unwrap()
        .as_ref()
        .map(|s| s.layer())
        .unwrap_or(-1)
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
// Scan Operations (Automatic Transitions)
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
// Mesh Operations
// =============================================================================

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
        info!("Starting mesh network (auto-transition STA -> STAAP -> mesh active)");

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
        info!("Starting mesh network (already in STAAP mode)");

        unsafe {
            // Initialize mesh if not already done
            sys::esp!(sys::esp_mesh_init())?;
            info!("Mesh initialized");

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
            info!("Mesh configured");

            // Start mesh
            sys::esp!(sys::esp_mesh_start())?;
            info!("Mesh started");

            // Enable self-organized mode
            sys::esp!(sys::esp_mesh_set_self_organized(true, true))?;
            info!("Mesh self-organized mode enabled");
        }

        // Update state
        self.mesh_id = config.mesh_id;
        self.current_channel = config.channel;

        // Update global state
        if let Some(state) = GLOBAL_STATE.lock().unwrap().as_mut() {
            state.mesh_state = MeshStateRuntime::SelfOrganized;
            state.current_channel = config.channel;
            state.mesh_id = config.mesh_id;
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

impl<O> WifiMeshState<StaAp, MeshSelfOrganized, NotScanning, O> {
    /// Disable self-organized mode to allow manual WiFi operations.
    ///
    /// Per ESP-MESH programming model: "When using ESP-WIFI-MESH under self-organized
    /// mode, users must ensure that no calls to Wi-Fi API are made."
    ///
    /// Call this before any WiFi operations, then re-enable after.
    pub fn disable_self_organized(self) -> Result<MeshManualState, sys::EspError> {
        info!("Disabling mesh self-organized mode for WiFi operations");

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
            is_root: self.is_root,
            layer: self.layer,
            has_ip: self.has_ip,
            current_channel: self.current_channel,
            mesh_id: self.mesh_id,
            sta_netif: self.sta_netif,
            ap_netif: self.ap_netif,
        })
    }

    /// Stop mesh network and return to inactive state.
    pub fn stop_mesh(self) -> Result<MeshReadyState, sys::EspError> {
        info!("Stopping mesh network");

        unsafe {
            // Disable self-organized mode first
            sys::esp!(sys::esp_mesh_set_self_organized(false, false))?;

            // Stop mesh
            sys::esp!(sys::esp_mesh_stop())?;

            // Deinitialize mesh
            sys::esp!(sys::esp_mesh_deinit())?;

            info!("Mesh stopped and deinitialized");
        }

        // Update global state
        if let Some(state) = GLOBAL_STATE.lock().unwrap().as_mut() {
            state.mesh_state = MeshStateRuntime::Inactive;
            state.is_root = false;
            state.layer = -1;
        }

        Ok(WifiMeshState {
            _wifi_mode: PhantomData,
            _mesh_state: PhantomData,
            _scan_state: PhantomData,
            _ota_state: PhantomData,
            is_root: false,
            layer: -1,
            has_ip: self.has_ip,
            current_channel: self.current_channel,
            mesh_id: self.mesh_id,
            sta_netif: self.sta_netif,
            ap_netif: self.ap_netif,
        })
    }
}

impl<O> WifiMeshState<StaAp, MeshActive, NotScanning, O> {
    /// Re-enable self-organized mode after manual WiFi operations.
    pub fn enable_self_organized(self) -> Result<MeshRunningState, sys::EspError> {
        info!("Re-enabling mesh self-organized mode");

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
            is_root: self.is_root,
            layer: self.layer,
            has_ip: self.has_ip,
            current_channel: self.current_channel,
            mesh_id: self.mesh_id,
            sta_netif: self.sta_netif,
            ap_netif: self.ap_netif,
        })
    }

    /// Scan while mesh is in manual mode (self-organized disabled).
    /// Temporarily switches to STA mode, scans, then returns to STAAP + manual mesh.
    pub fn scan(self) -> Result<(ScanResults, Self), sys::EspError> {
        info!("Scanning in mesh manual mode (will temporarily switch to STA)");

        // Transition to STA for scanning
        let sta_state: WifiMeshState<Sta, MeshActive, NotScanning, O> = WifiMeshState {
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
            is_root: sta_state.is_root,
            layer: sta_state.layer,
            has_ip: sta_state.has_ip,
            current_channel: sta_state.current_channel,
            mesh_id: sta_state.mesh_id,
            sta_netif: sta_state.sta_netif,
            ap_netif: sta_state.ap_netif,
        };

        Ok((results, staap_state))
    }

    /// Stop mesh network.
    pub fn stop_mesh(self) -> Result<MeshReadyState, sys::EspError> {
        info!("Stopping mesh from manual mode");

        unsafe {
            sys::esp!(sys::esp_mesh_stop())?;
            sys::esp!(sys::esp_mesh_deinit())?;
        }

        // Update global state
        if let Some(state) = GLOBAL_STATE.lock().unwrap().as_mut() {
            state.mesh_state = MeshStateRuntime::Inactive;
        }

        Ok(WifiMeshState {
            _wifi_mode: PhantomData,
            _mesh_state: PhantomData,
            _scan_state: PhantomData,
            _ota_state: PhantomData,
            is_root: false,
            layer: -1,
            has_ip: self.has_ip,
            current_channel: self.current_channel,
            mesh_id: self.mesh_id,
            sta_netif: self.sta_netif,
            ap_netif: self.ap_netif,
        })
    }
}

// Helper for scanning while mesh is active but in manual mode
impl<O> WifiMeshState<Sta, MeshActive, NotScanning, O> {
    fn scan_in_mesh_manual(self) -> Result<(ScanResults, Self), sys::EspError> {
        info!("Performing scan in mesh manual mode");

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

// =============================================================================
// Query Methods (Available on all states)
// =============================================================================

impl<W, M, S, O> WifiMeshState<W, M, S, O> {
    /// Query if this node is currently the mesh root
    pub fn is_root(&self) -> bool {
        self.is_root
    }

    /// Query the current mesh layer (-1 if not in mesh)
    pub fn layer(&self) -> i32 {
        self.layer
    }

    /// Query if this node has an IP address (only root nodes)
    pub fn has_ip(&self) -> bool {
        self.has_ip
    }

    /// Get the current WiFi channel
    pub fn channel(&self) -> u8 {
        self.current_channel
    }

    /// Get the mesh ID
    pub fn mesh_id(&self) -> [u8; 6] {
        self.mesh_id
    }

    /// Refresh runtime state from ESP-MESH APIs
    /// Call this periodically or after mesh events
    pub fn refresh_state(&mut self) -> Result<(), sys::EspError> {
        unsafe {
            self.is_root = sys::esp_mesh_is_root();
            self.layer = sys::esp_mesh_get_layer();

            debug!("State refreshed: is_root={}, layer={}", self.is_root, self.layer);

            // Also update global state
            if let Some(state) = GLOBAL_STATE.lock().unwrap().as_mut() {
                state.is_root = self.is_root;
                state.layer = self.layer;
            }
        }
        Ok(())
    }

    /// Set IP status (called from IP event handlers)
    pub fn set_has_ip(&mut self, has_ip: bool) {
        self.has_ip = has_ip;

        if let Some(state) = GLOBAL_STATE.lock().unwrap().as_mut() {
            state.has_ip = has_ip;
        }
    }

    /// Set network interface pointers (called during initialization)
    pub fn set_netif(&mut self, sta: Option<*mut sys::esp_netif_t>, ap: Option<*mut sys::esp_netif_t>) {
        self.sta_netif = sta;
        self.ap_netif = ap;

        if let Some(state) = GLOBAL_STATE.lock().unwrap().as_mut() {
            state.sta_netif = sta;
            state.ap_netif = ap;
        }
    }
}

// =============================================================================
// OTA State Transitions
// =============================================================================

/// OTA Idle state - ready to start OTA operations
impl<W, M, S> WifiMeshState<W, M, S, OtaIdle> {
    /// Begin OTA download (root node only)
    /// Transitions from Idle → Downloading
    pub fn begin_ota_download(self, firmware_url: String, firmware_size: u32, version: String)
        -> anyhow::Result<WifiMeshState<W, M, S, OtaDownloading>>
    {
        info!("Beginning OTA download: v{} ({} bytes) from {}", version, firmware_size, firmware_url);

        // Update global state
        if let Some(state) = GLOBAL_STATE.lock().unwrap().as_mut() {
            state.ota_state = OtaStateRuntime::Downloading;
            state.ota_data.firmware_url = Some(firmware_url);
            state.ota_data.total_size = firmware_size;
            state.ota_data.progress = 0;
            state.ota_data.target_version = Some(version);
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

    /// Begin OTA reception (child node only)
    /// Transitions from Idle → Receiving
    pub fn begin_ota_reception(self, total_chunks: u32, firmware_size: u32)
        -> anyhow::Result<WifiMeshState<W, M, S, OtaReceiving>>
    {
        info!("Beginning OTA reception: {} chunks ({} bytes)", total_chunks, firmware_size);

        // Update global state
        if let Some(state) = GLOBAL_STATE.lock().unwrap().as_mut() {
            state.ota_state = OtaStateRuntime::Receiving;
            state.ota_data.total_chunks = total_chunks;
            state.ota_data.total_size = firmware_size;
            state.ota_data.progress = 0;
            state.ota_data.next_expected_sequence = 0;
            state.ota_data.received_chunks_buffer.clear();
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

/// OTA Downloading state (root node)
impl<W, M, S> WifiMeshState<W, M, S, OtaDownloading> {
    /// Complete download and transition to distributing
    /// Transitions from Downloading → Distributing
    pub fn complete_download(self, firmware_data: Vec<u8>)
        -> anyhow::Result<WifiMeshState<W, M, S, OtaDistributing>>
    {
        info!("Completing OTA download, fragmenting firmware...");

        // Fragment firmware into chunks
        let total_chunks = (firmware_data.len() + crate::ota::CHUNK_SIZE - 1) / crate::ota::CHUNK_SIZE;
        let version = GLOBAL_STATE.lock().unwrap()
            .as_ref()
            .and_then(|s| s.ota_data.target_version.clone())
            .unwrap_or_else(|| "unknown".to_string());

        let mut chunks = Vec::new();
        for (i, chunk_data) in firmware_data.chunks(crate::ota::CHUNK_SIZE).enumerate() {
            let chunk = crate::ota::FirmwareChunk::new(
                i as u32,
                total_chunks as u32,
                version.clone(),
                chunk_data.to_vec()
            );
            chunks.push(chunk);
        }

        // Update global state
        if let Some(state) = GLOBAL_STATE.lock().unwrap().as_mut() {
            state.ota_state = OtaStateRuntime::Distributing;
            state.ota_data.chunks = chunks;
            state.ota_data.total_chunks = total_chunks as u32;
        }

        info!("Firmware fragmented into {} chunks, ready to distribute", total_chunks);

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

    /// Cancel OTA and return to idle
    pub fn cancel_ota(self) -> WifiMeshState<W, M, S, OtaIdle> {
        info!("Cancelling OTA download");

        if let Some(state) = GLOBAL_STATE.lock().unwrap().as_mut() {
            state.ota_state = OtaStateRuntime::Idle;
            *state.ota_data_mut() = OtaRuntimeData::new();
        }

        WifiMeshState {
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
        }
    }
}

/// OTA Distributing state (root node)
impl<W, M, S> WifiMeshState<W, M, S, OtaDistributing> {
    /// Complete distribution (all nodes ready) and transition to ready to reboot
    /// Transitions from Distributing → ReadyToReboot
    pub fn complete_distribution(self)
        -> anyhow::Result<WifiMeshState<W, M, S, OtaReadyToReboot>>
    {
        info!("All nodes ready, transitioning to ready to reboot");

        if let Some(state) = GLOBAL_STATE.lock().unwrap().as_mut() {
            state.ota_state = OtaStateRuntime::ReadyToReboot;
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

    /// Cancel OTA and return to idle
    pub fn cancel_ota(self) -> WifiMeshState<W, M, S, OtaIdle> {
        info!("Cancelling OTA distribution");

        if let Some(state) = GLOBAL_STATE.lock().unwrap().as_mut() {
            state.ota_state = OtaStateRuntime::Idle;
            *state.ota_data_mut() = OtaRuntimeData::new();
        }

        WifiMeshState {
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
        }
    }
}

/// OTA Receiving state (child node)
impl<W, M, S> WifiMeshState<W, M, S, OtaReceiving> {
    /// Complete reception (all chunks received and validated)
    /// Transitions from Receiving → ReadyToReboot
    pub fn complete_reception(self)
        -> anyhow::Result<WifiMeshState<W, M, S, OtaReadyToReboot>>
    {
        info!("All chunks received and validated, transitioning to ready to reboot");

        if let Some(state) = GLOBAL_STATE.lock().unwrap().as_mut() {
            state.ota_state = OtaStateRuntime::ReadyToReboot;
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

    /// Cancel OTA and return to idle
    pub fn cancel_ota(self) -> WifiMeshState<W, M, S, OtaIdle> {
        info!("Cancelling OTA reception");

        if let Some(state) = GLOBAL_STATE.lock().unwrap().as_mut() {
            state.ota_state = OtaStateRuntime::Idle;
            *state.ota_data_mut() = OtaRuntimeData::new();
        }

        WifiMeshState {
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
        }
    }
}

/// OTA Ready to Reboot state
impl<W, M, S> WifiMeshState<W, M, S, OtaReadyToReboot> {
    /// Reboot the device (never returns)
    pub fn reboot(self) -> ! {
        info!("Rebooting device to apply OTA update...");
        unsafe {
            sys::esp_restart();
        }
    }

    /// Cancel OTA and return to idle (in case of abort before reboot)
    pub fn cancel_ota(self) -> WifiMeshState<W, M, S, OtaIdle> {
        info!("Cancelling OTA before reboot");

        if let Some(state) = GLOBAL_STATE.lock().unwrap().as_mut() {
            state.ota_state = OtaStateRuntime::Idle;
            *state.ota_data_mut() = OtaRuntimeData::new();
        }

        WifiMeshState {
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
        }
    }
}

// Keep OtaActive for backwards compatibility
impl<W, M, S> WifiMeshState<W, M, S, OtaActive> {
    /// Complete OTA operation and return to idle state.
    /// Call this whether OTA succeeded or failed.
    pub fn finish_ota(self) -> WifiMeshState<W, M, S, OtaIdle> {
        info!("Finishing OTA operation");

        // Update global state
        if let Some(state) = GLOBAL_STATE.lock().unwrap().as_mut() {
            state.ota_state = OtaStateRuntime::Idle;
        }

        WifiMeshState {
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
        }
    }

    /// Query if OTA is currently active
    /// (Always returns true for this state, provided for API consistency)
    pub fn is_ota_active(&self) -> bool {
        true
    }
}
