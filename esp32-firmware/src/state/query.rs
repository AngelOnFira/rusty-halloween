//! State query helpers for accessing global state
//!
//! This module provides convenient helper functions for querying the global
//! state machine from event handlers and tasks.

use super::{GLOBAL_STATE, StateContainer};
use std::sync::{Arc, Mutex};

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
