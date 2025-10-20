use crate::instructions::{Instructions, InstructionStatus};
use crate::node::MeshNode;
use esp_idf_sys::{
    self as sys, esp_mesh_get_layer, esp_mesh_get_total_node_num, esp_mesh_is_device_active,
    esp_mesh_is_root, esp_mesh_recv, esp_mesh_send, esp_random, mesh_addr_t, mesh_data_t, ESP_OK,
};
use log::*;
use std::{
    ptr,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

/// Application state containing the instruction queue
pub struct State {
    pub instructions: Instructions,
}

/// Task for receiving mesh messages
pub fn mesh_rx_task(node: Arc<MeshNode>, state: Arc<Mutex<State>>) {
    let mut rx_buf = vec![0u8; 1500];
    let mut from_addr = mesh_addr_t { addr: [0; 6] };
    let mut flag = 0i32;

    loop {
        let mut mesh_data = mesh_data_t {
            data: rx_buf.as_mut_ptr(),
            size: rx_buf.len() as u16,
            proto: 0,
            tos: 0,
        };

        unsafe {
            let err = esp_mesh_recv(
                &mut from_addr,
                &mut mesh_data,
                5000,
                &mut flag,
                ptr::null_mut(),
                0,
            );

            if err == ESP_OK {
                let data_str = std::str::from_utf8(&rx_buf[..mesh_data.size as usize])
                    .unwrap_or("Invalid UTF-8");
                info!(
                    "Received from {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}: {}",
                    from_addr.addr[0],
                    from_addr.addr[1],
                    from_addr.addr[2],
                    from_addr.addr[3],
                    from_addr.addr[4],
                    from_addr.addr[5],
                    data_str
                );

                // Parse JSON commands (color, challenges, responses) with better error handling
                match serde_json::from_str::<serde_json::Value>(data_str) {
                    Ok(command) => {
                        // Handle different message types
                        if let Some(msg_type) = command["type"].as_str() {
                            match msg_type {
                                "challenge" => {
                                    if let Some(challenge_id) = command["id"].as_u64() {
                                        info!("📨 Received challenge ID: {}", challenge_id);
                                        node.send_challenge_response(challenge_id as u32);
                                    }
                                }
                                "challenge_response" => {
                                    if let Some(challenge_id) = command["id"].as_u64() {
                                        info!(
                                            "📥 Received response for challenge ID: {}",
                                            challenge_id
                                        );
                                        node.handle_challenge_response(challenge_id as u32);
                                    }
                                }
                                "data" => {
                                    if let Some(data) = command["data"].as_array() {
                                        for item in data {
                                            // Type is (i32, (u8, u8, u8))
                                            if let Some(value) = item.as_u64() {
                                                info!("Received data item: {}", value);
                                            }
                                        }
                                    }
                                }
                                _ => {
                                    warn!("Unknown message type: {}", msg_type);
                                }
                            }
                        } else if let (Some(r), Some(g), Some(b)) = (
                            command["r"].as_u64().map(|v| v as u8),
                            command["g"].as_u64().map(|v| v as u8),
                            command["b"].as_u64().map(|v| v as u8),
                        ) {
                            info!("Received valid color command: RGB({}, {}, {})", r, g, b);
                            node.set_color(r, g, b);
                        } else {
                            warn!("Invalid command format: missing type or r/g/b values");
                        }
                    }
                    Err(e) => {
                        warn!("Failed to parse JSON command: {} - Data: {}", e, data_str);
                    }
                }
            }
        }

        thread::sleep(Duration::from_millis(10));
    }
}

/// Task for transmitting mesh messages
pub fn mesh_tx_task(node: Arc<MeshNode>, state: Arc<Mutex<State>>) {
    let mut counter = 0u32;
    let mut _challenge_counter = 0u32;

    loop {
        thread::sleep(Duration::from_secs(5)); // Send updates every 5 second

        unsafe {
            if !esp_mesh_is_device_active() {
                continue;
            }

            let is_root = esp_mesh_is_root();

            if is_root {
                // Lock the state
                let mut state_lock = state.lock().unwrap();

                // If we only have a few seconds of data left, generate more
                if state_lock.instructions.get_buffer_left() < 5000 {
                    state_lock.instructions.generate_random_instructions(5);
                }

                // Send the next 5 seconds of colours to all nodes
                let color_command = serde_json::json!({
                    "type": "data",
                    "data": state_lock.instructions.get_next_seconds_instructions(5),
                });

                let message = color_command.to_string();
                let broadcast_addr = mesh_addr_t { addr: [0xFF; 6] };

                let mesh_data = mesh_data_t {
                    data: message.as_ptr() as *mut u8,
                    size: message.len() as u16,
                    proto: 0,
                    tos: 0,
                };

                let flag = 0x01; // MESH_DATA_GROUP flag

                // Try sending the message up to 3 times for better reliability
                let mut success = false;
                for attempt in 1..=3 {
                    let err = esp_mesh_send(&broadcast_addr, &mesh_data, flag, ptr::null(), 0);

                    if err == ESP_OK {
                        success = true;
                        break;
                    } else {
                        warn!(
                            "Failed to send color command on attempt {}: error {}",
                            attempt, err
                        );
                        if attempt < 3 {
                            thread::sleep(Duration::from_millis(100)); // Brief delay before retry
                        }
                    }
                }

                if !success {
                    warn!("All attempts to send color command failed",);
                }

                // Send packet loss test challenges every 5 seconds
                if counter % 5 == 0 {
                    _challenge_counter += 1;
                    let challenge_id = esp_random();
                    if node.send_challenge(challenge_id) {
                        info!("📡 Sent challenge ID: {}", challenge_id);
                    } else {
                        warn!("Failed to send challenge ID: {}", challenge_id);
                    }
                }

                // Print packet loss statistics every 30 seconds
                if counter % 30 == 0 && counter > 0 {
                    node.print_packet_loss_stats();
                }

                counter += 1;
            } else {
                // Non-root nodes can send periodic status updates
                if counter % 10 == 0 {
                    let layer = esp_mesh_get_layer();
                    let total_nodes = esp_mesh_get_total_node_num();
                    let message = format!(
                        "Status from layer {layer} (nodes: {total_nodes}, count: {counter})"
                    );

                    let broadcast_addr = mesh_addr_t { addr: [0xFF; 6] };

                    let mesh_data = mesh_data_t {
                        data: message.as_ptr() as *mut u8,
                        size: message.len() as u16,
                        proto: 0,
                        tos: 0,
                    };

                    let flag = 0x01; // MESH_DATA_GROUP flag

                    let err = esp_mesh_send(&broadcast_addr, &mesh_data, flag, ptr::null(), 0);

                    if err == ESP_OK {
                        info!("Status message sent: {message}");
                    } else {
                        warn!("Failed to send status: {err:?}");
                    }
                }

                counter += 1;
            }
        }
    }
}

/// Task for monitoring mesh status
pub fn monitor_task(node: Arc<MeshNode>) {
    loop {
        thread::sleep(Duration::from_secs(5));

        unsafe {
            let is_root = esp_mesh_is_root();
            let layer = esp_mesh_get_layer();
            let is_active = esp_mesh_is_device_active();
            let total_nodes = esp_mesh_get_total_node_num();

            *node.is_root.lock().unwrap() = is_root;
            *node.is_connected.lock().unwrap() = is_active;
            *node.layer.lock().unwrap() = layer;

            info!(
                "Status - Root: {is_root}, Layer: {layer}, Active: {is_active}, Total Nodes: {total_nodes}"
            );

            // Don't override synchronized colors - only show status when disconnected
            if !is_root && !is_active {
                node.update_status_color();
            }
        }
    }
}

/// Task for executing timed instructions
pub fn instruction_execution_task(node: Arc<MeshNode>, state: Arc<Mutex<State>>) {
    loop {
        let mut state_lock = state.lock().unwrap();

        let instruction = state_lock.instructions.get_next_instruction();

        match instruction {
            InstructionStatus::Sleep(duration) => {
                // Release lock before sleeping
                drop(state_lock);
                thread::sleep(Duration::from_millis(duration as u64));
            }
            InstructionStatus::SetColor(color) => {
                node.set_color(color.r, color.g, color.b);
                // Lock is automatically released when state_lock goes out of scope
            }
        }
    }
}
