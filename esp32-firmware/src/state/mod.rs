//! WiFi/Mesh state machine module
//!
//! This module provides a typestate-based state machine for managing WiFi and ESP-MESH
//! states with compile-time safety guarantees.

use std::sync::{Arc, Mutex};
use once_cell::sync::Lazy;

// Core type definitions
pub mod types;

// Functional modules
pub mod wifi;
pub mod mesh;
pub mod scan;
pub mod ota;
pub mod mesh_ops;
pub mod query;

// Re-export core types from types module
pub use types::{
    // Marker types
    Uninitialized, Sta, Ap, StaAp,
    MeshInactive, MeshActive, MeshSelfOrganized,
    NotScanning, Scanning,
    OtaIdle, OtaDownloading, OtaDistributing, OtaReceiving, OtaReadyToReboot, OtaActive,

    // Core state machine
    WifiMeshState,

    // Type aliases
    InitialState, ScanCapableState, ScanningState, MeshReadyState, MeshRunningState, MeshManualState,

    // State container and runtime enums
    StateContainer,
    WifiModeRuntime, MeshStateRuntime, ScanStateRuntime, OtaStateRuntime,
    OtaRuntimeData,
};

// Re-export from wifi module
pub use wifi::ScanResults;

// Re-export from mesh module
pub use mesh::{MeshConfig, MESH_ID, MESH_PASSWORD, MESH_MAX_LAYER, MESH_AP_CONNECTIONS};

// Re-export from scan module
pub use scan::{NetworkDiscovery, load_channel_from_nvs, save_channel_to_nvs, scan_with_retry};

// Re-export from ota module
pub use ota::{
    OtaManager, OtaMessage, OtaState, FirmwareChunk, NodeProgress, CHUNK_SIZE,
};

// Re-export from mesh_ops
pub use mesh_ops::{
    MeshMessage, BROADCAST_ADDR,
    mesh_send, send_broadcast, mesh_recv,
    is_mesh_active, get_mesh_node_count,
};

// Re-export query functions
pub use query::{global_state, is_root, has_ip, layer};

// Global state singleton
pub(crate) static GLOBAL_STATE: Lazy<Arc<Mutex<Option<StateContainer>>>> =
    Lazy::new(|| Arc::new(Mutex::new(None)));
