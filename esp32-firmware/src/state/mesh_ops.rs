//! Mesh communication operations
//!
//! This module provides type-safe wrappers around ESP-MESH communication APIs.
//! All mesh send/recv operations should go through these methods to ensure
//! consistent error handling and logging.

use super::types::WifiMeshState;
use esp_idf_svc::sys as sys;
use log::{debug, warn};
use std::ptr;

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

            let err = sys::esp_mesh_send(dest, &mesh_data, flag, ptr::null(), 0);

            if err == sys::ESP_OK {
                debug!("Mesh message sent: {} bytes", data.len());
                Ok(())
            } else {
                warn!("Failed to send mesh message: error {}", err);
                Err(sys::EspError::from(err).unwrap())
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

                debug!("Mesh message received: {} bytes from {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
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
                    warn!("Failed to receive mesh message: error {}", err);
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
            debug!("Mesh message sent: {} bytes", data.len());
            Ok(())
        } else {
            warn!("Failed to send mesh message: error {}", err);
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

            debug!("Mesh message received: {} bytes from {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
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
                warn!("Failed to receive mesh message: error {}", err);
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
