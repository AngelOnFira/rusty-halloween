//! Type definitions for the WiFi/Mesh state machine
//!
//! This module contains:
//! - Zero-sized marker types for compile-time state tracking
//! - The core WifiMeshState struct
//! - Type aliases for common state combinations
//! - Runtime state enums
//! - StateContainer for type-erased global state

use esp_idf_svc::sys as sys;
use std::marker::PhantomData;
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
/// - `O`: OTA state (OtaIdle, OtaActive, etc.)
///
/// Runtime state supplements compile-time guarantees with dynamic information
/// that can only be determined at runtime (root status, layer, etc.).
pub struct WifiMeshState<W, M, S, O> {
    // Compile-time state markers (zero runtime cost)
    pub(crate) _wifi_mode: PhantomData<W>,
    pub(crate) _mesh_state: PhantomData<M>,
    pub(crate) _scan_state: PhantomData<S>,
    pub(crate) _ota_state: PhantomData<O>,

    // Runtime state that supplements compile-time state
    pub(crate) is_root: bool,
    pub(crate) layer: i32,
    pub(crate) has_ip: bool,
    pub(crate) current_channel: u8,
    pub(crate) mesh_id: [u8; 6],

    // Network interface handles (Some when initialized)
    pub(crate) sta_netif: Option<*mut sys::esp_netif_t>,
    pub(crate) ap_netif: Option<*mut sys::esp_netif_t>,
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
    pub chunks: Vec<super::ota::FirmwareChunk>,
    /// OTA update handle (child node only - for reception)
    /// Note: Stored as raw pointer because OtaUpdate is not Send/Sync
    pub ota_handle: Option<*mut esp_ota::OtaUpdate>,
    /// Received chunks buffer (child node only - out-of-order chunks)
    pub received_chunks_buffer: HashMap<u32, super::ota::FirmwareChunk>,
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
// Runtime State Enums
// =============================================================================

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

// =============================================================================
// State Container (Type-Erased Global State)
// =============================================================================

/// Type-erased container for the state machine (since we can't store generic types in static)
pub struct StateContainer {
    // Runtime state fields (duplicated from WifiMeshState for type-erased access)
    pub(crate) is_root: bool,
    pub(crate) layer: i32,
    pub(crate) has_ip: bool,
    pub(crate) current_channel: u8,
    pub(crate) mesh_id: [u8; 6],
    pub(crate) sta_netif: Option<*mut sys::esp_netif_t>,
    pub(crate) ap_netif: Option<*mut sys::esp_netif_t>,

    // Current state type information (for runtime validation)
    pub(crate) wifi_mode: WifiModeRuntime,
    pub(crate) mesh_state: MeshStateRuntime,
    pub(crate) scan_state: ScanStateRuntime,
    pub(crate) ota_state: OtaStateRuntime,

    // OTA runtime data
    pub(crate) ota_data: OtaRuntimeData,
}

// Safety: The raw pointers in StateContainer are only set during initialization
// and are never dereferenced by the StateContainer itself. They're passed to
// ESP-IDF APIs which handle them correctly. The pointers remain valid for the
// lifetime of the program.
unsafe impl Send for StateContainer {}
unsafe impl Sync for StateContainer {}

impl StateContainer {
    pub(crate) fn new(
        is_root: bool,
        layer: i32,
        has_ip: bool,
        current_channel: u8,
        mesh_id: [u8; 6],
        sta_netif: Option<*mut sys::esp_netif_t>,
        ap_netif: Option<*mut sys::esp_netif_t>,
        wifi_mode: WifiModeRuntime,
        mesh_state: MeshStateRuntime,
        scan_state: ScanStateRuntime,
        ota_state: OtaStateRuntime,
    ) -> Self {
        Self {
            is_root,
            layer,
            has_ip,
            current_channel,
            mesh_id,
            sta_netif,
            ap_netif,
            wifi_mode,
            mesh_state,
            scan_state,
            ota_state,
            ota_data: OtaRuntimeData::new(),
        }
    }

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

            debug!("state::types: State refreshed: is_root={}, layer={}", self.is_root, self.layer);
        }
        Ok(())
    }

    /// Set IP status (called from IP event handlers)
    pub fn set_has_ip(&mut self, has_ip: bool) {
        self.has_ip = has_ip;
        info!("state::types: IP status updated: has_ip={}", has_ip);
    }

    /// Set root status (called from mesh event handlers)
    pub fn set_is_root(&mut self, is_root: bool) {
        self.is_root = is_root;
        info!("state::types: Root status updated: is_root={}", is_root);
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
