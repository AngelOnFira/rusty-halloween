use crate::hardware::WS2812Controller;
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_sys::{
    esp_mesh_send, mesh_addr_t, mesh_data_t, ESP_OK,
};
use log::*;
use smart_leds::RGB8;
use std::{
    collections::HashMap,
    ptr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use anyhow::Result;

/// Main mesh node structure containing hardware and network state
pub struct MeshNode {
    pub led: WS2812Controller,
    pub is_root: Arc<Mutex<bool>>,
    pub is_connected: Arc<Mutex<bool>>,
    pub layer: Arc<Mutex<i32>>,
    pub current_color: Arc<Mutex<RGB8>>,
    // Packet loss testing
    pub pending_challenges: Arc<Mutex<HashMap<u32, Instant>>>,
    pub total_challenges_sent: Arc<Mutex<u32>>,
    pub total_responses_received: Arc<Mutex<u32>>,
}

impl MeshNode {
    pub fn new(peripherals: Peripherals) -> Result<Self> {
        // Initialize WS2812 controller on GPIO18
        let led = WS2812Controller::new(peripherals.rmt.channel0, peripherals.pins.gpio18)?;

        Ok(Self {
            led,
            is_root: Arc::new(Mutex::new(false)),
            is_connected: Arc::new(Mutex::new(false)),
            layer: Arc::new(Mutex::new(-1)),
            current_color: Arc::new(Mutex::new(RGB8::new(0, 0, 0))),
            pending_challenges: Arc::new(Mutex::new(HashMap::new())),
            total_challenges_sent: Arc::new(Mutex::new(0)),
            total_responses_received: Arc::new(Mutex::new(0)),
        })
    }

    pub fn update_status_color(&self) {
        let is_connected = *self.is_connected.lock().unwrap();
        let is_root = *self.is_root.lock().unwrap();

        // Use different colors to indicate status
        let status_color = match (is_connected, is_root) {
            (false, _) => RGB8::new(0, 0, 0),     // Off when not connected
            (true, true) => RGB8::new(0, 10, 0),  // Green very dim when root and connected
            (true, false) => RGB8::new(0, 0, 10), // Blue very dim when child and connected
        };

        let _ = self.led.set_color(status_color);
    }

    pub fn set_color(&self, r: u8, g: u8, b: u8) {
        let color = RGB8::new(r, g, b);
        *self.current_color.lock().unwrap() = color;

        let _ = self.led.set_color(color);

        info!("Set WS2812 color to RGB({}, {}, {})", r, g, b);
    }

    pub fn send_challenge(&self, challenge_id: u32) -> bool {
        let challenge_command = serde_json::json!({
            "type": "challenge",
            "id": challenge_id
        });

        let message = challenge_command.to_string();
        let broadcast_addr = mesh_addr_t { addr: [0xFF; 6] };

        let mesh_data = mesh_data_t {
            data: message.as_ptr() as *mut u8,
            size: message.len() as u16,
            proto: 0,
            tos: 0,
        };

        let flag = 0x01; // MESH_DATA_GROUP flag

        unsafe {
            let err = esp_mesh_send(&broadcast_addr, &mesh_data, flag, ptr::null(), 0);
            if err == ESP_OK {
                // Record the challenge
                self.pending_challenges
                    .lock()
                    .unwrap()
                    .insert(challenge_id, Instant::now());
                *self.total_challenges_sent.lock().unwrap() += 1;
                true
            } else {
                false
            }
        }
    }

    pub fn handle_challenge_response(&self, challenge_id: u32) {
        if self
            .pending_challenges
            .lock()
            .unwrap()
            .remove(&challenge_id)
            .is_some()
        {
            *self.total_responses_received.lock().unwrap() += 1;
        }
    }

    pub fn print_packet_loss_stats(&self) {
        let challenges_sent = *self.total_challenges_sent.lock().unwrap();
        let responses_received = *self.total_responses_received.lock().unwrap();

        // Clean up expired challenges (older than 5 seconds)
        let now = Instant::now();
        let mut pending = self.pending_challenges.lock().unwrap();
        pending.retain(|_, timestamp| now.duration_since(*timestamp) < Duration::from_secs(5));
        let pending_count = pending.len();
        drop(pending);

        if challenges_sent > 0 {
            let success_rate = (responses_received as f32 / challenges_sent as f32) * 100.0;
            info!(
                "📊 PACKET LOSS STATS: Sent: {}, Received: {}, Pending: {}, Success: {:.1}%",
                challenges_sent, responses_received, pending_count, success_rate
            );
        }
    }

    pub fn send_challenge_response(&self, challenge_id: u32) {
        let response_command = serde_json::json!({
            "type": "challenge_response",
            "id": challenge_id
        });

        let message = response_command.to_string();
        let broadcast_addr = mesh_addr_t { addr: [0xFF; 6] };

        let mesh_data = mesh_data_t {
            data: message.as_ptr() as *mut u8,
            size: message.len() as u16,
            proto: 0,
            tos: 0,
        };

        let flag = 0x01; // MESH_DATA_GROUP flag

        unsafe {
            let err = esp_mesh_send(&broadcast_addr, &mesh_data, flag, ptr::null(), 0);
            if err == ESP_OK {
                info!("📤 Sent challenge response for ID: {}", challenge_id);
            } else {
                warn!("Failed to send challenge response: {}", err);
            }
        }
    }
}
