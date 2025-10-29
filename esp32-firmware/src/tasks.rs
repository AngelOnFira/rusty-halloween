use crate::instructions::{Instructions, InstructionStatus};
use crate::node::MeshNode;
use crate::state::{
    self,
    mesh_send, mesh_recv, is_mesh_active, get_mesh_node_count,
    BROADCAST_ADDR,
    OtaManager, OtaMessage, OtaState,
};
use esp_idf_sys::{esp_random, mesh_addr_t, ESP_OK};
use std::{
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

/// Application state containing the instruction queue and OTA manager
pub struct ApplicationState {
    pub instructions: Instructions,
    pub ota_manager: Arc<Mutex<OtaManager>>,
}

/// Task for receiving mesh messages
pub fn mesh_rx_task(node: Arc<MeshNode>, state: Arc<Mutex<ApplicationState>>) {
    loop {
        // Use state machine's mesh_recv helper
        match mesh_recv(5000) {
            Ok(msg) => {
                let data_str = std::str::from_utf8(&msg.data)
                    .unwrap_or("Invalid UTF-8");
                unsafe {
                    info!(
                        "Received from {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}: {}",
                        msg.from_addr.addr[0],
                        msg.from_addr.addr[1],
                        msg.from_addr.addr[2],
                        msg.from_addr.addr[3],
                        msg.from_addr.addr[4],
                        msg.from_addr.addr[5],
                        data_str
                    );
                }

                // Parse JSON commands (color, challenges, responses, OTA) with better error handling
                match serde_json::from_str::<serde_json::Value>(data_str) {
                    Ok(command) => {
                        // Handle different message types
                        if let Some(msg_type) = command["type"].as_str() {
                            match msg_type {
                                "challenge" => {
                                    if let Some(challenge_id) = command["id"].as_u64() {
                                        info!("tasks: Received challenge ID: {}", challenge_id);
                                        node.send_challenge_response(challenge_id as u32);
                                    }
                                }
                                "challenge_response" => {
                                    if let Some(challenge_id) = command["id"].as_u64() {
                                        info!(
                                            "Received response for challenge ID: {}",
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
                                                info!("tasks: Received data item: {}", value);
                                            }
                                        }
                                    }
                                }
                                // OTA message types
                                "check_update" => {
                                    info!("tasks: Received check_update command");
                                    // This will be handled by the root node in mesh_tx_task
                                }
                                "ota_start" => {
                                    if let Ok(ota_msg) = serde_json::from_value::<OtaMessage>(command) {
                                        if let OtaMessage::OtaStart { version, total_chunks, firmware_size } = ota_msg {
                                            info!("tasks: OTA Update starting: v{} ({} chunks, {} bytes)", version, total_chunks, firmware_size);
                                            let state_lock = state.lock().unwrap();
                                            let mut ota_guard = state_lock.ota_manager.lock().unwrap();
                                            if let Err(e) = ota_guard.start_ota_reception(total_chunks, firmware_size) {
                                                warn!("tasks: Failed to start OTA reception: {:?}", e);
                                            }
                                        }
                                    }
                                }
                                "ota_chunk" => {
                                    if let Ok(ota_msg) = serde_json::from_value::<OtaMessage>(command) {
                                        if let OtaMessage::OtaChunk { chunk } = ota_msg {
                                            let state_lock = state.lock().unwrap();
                                            let mut ota_guard = state_lock.ota_manager.lock().unwrap();
                                            match ota_guard.handle_chunk(chunk.clone()) {
                                                Ok(complete) => {
                                                    // Send ACK
                                                    // TODO: Implement ACK sending
                                                    if complete {
                                                        info!("tasks: OTA update complete! Ready to reboot.");
                                                    }
                                                }
                                                Err(e) => {
                                                    warn!("tasks: Failed to handle OTA chunk {}: {:?}", chunk.sequence, e);
                                                }
                                            }
                                        }
                                    }
                                }
                                "ota_reboot" => {
                                    info!("tasks: Received OTA reboot command - rebooting in 2 seconds...");
                                    thread::sleep(Duration::from_secs(2));
                                    unsafe {
                                        esp_idf_sys::esp_restart();
                                    }
                                }
                                "ota_cancel" => {
                                    if let Some(reason) = command["reason"].as_str() {
                                        warn!("tasks: ❌ OTA update cancelled: {}", reason);
                                    }
                                }
                                _ => {
                                    warn!("tasks: Unknown message type: {}", msg_type);
                                }
                            }
                        } else if let (Some(r), Some(g), Some(b)) = (
                            command["r"].as_u64().map(|v| v as u8),
                            command["g"].as_u64().map(|v| v as u8),
                            command["b"].as_u64().map(|v| v as u8),
                        ) {
                            info!("tasks: Received valid color command: RGB({}, {}, {})", r, g, b);
                            node.set_color(r, g, b);
                        } else {
                            warn!("tasks: Invalid command format: missing type or r/g/b values");
                        }
                    }
                    Err(e) => {
                        warn!("tasks: Failed to parse JSON command: {} - Data: {}", e, data_str);
                    }
                }
            }
            Err(_) => {
                // Timeout or error receiving message (expected, don't log)
            }
        }

        thread::sleep(Duration::from_millis(10));
    }
}

/// Task for transmitting mesh messages
pub fn mesh_tx_task(node: Arc<MeshNode>, state: Arc<Mutex<ApplicationState>>) {
    let mut counter = 0u32;
    let mut _challenge_counter = 0u32;
    let mut ota_check_done = false; // Only check for OTA updates once

    loop {
        thread::sleep(Duration::from_secs(5)); // Send updates every 5 second

        if !is_mesh_active() {
            continue;
        }

        unsafe {
            let is_root_node = state::is_root();

            if is_root_node {
                // Check for OTA updates as soon as root node gets IP address
                // Uses event-driven state tracking instead of arbitrary timer
                let has_ip_status = state::has_ip();
                if !ota_check_done && has_ip_status {
                    info!("tasks: Checking for firmware updates from GitHub...");
                    info!("tasks: Root node connected with IP - checking for OTA updates");
                    ota_check_done = true; // Only check once

                    let state_lock = state.lock().unwrap();
                    let mut ota_manager = state_lock.ota_manager.lock().unwrap();

                    match ota_manager.check_for_updates() {
                        Ok(Some(release)) => {
                            info!("tasks: Update available! Release: {}", release.name);

                            // Get the firmware asset
                            if let Some(asset) = release.get_firmware_asset() {
                                info!("tasks: Firmware asset: {} ({} bytes)", asset.name, asset.size);

                                // Parse version from tag
                                match release.version() {
                                    Ok(version) => {
                                        // Trigger OTA update
                                        info!("tasks: Starting OTA update to v{}...", version);
                                        if let Err(e) = ota_manager.trigger_ota_update(
                                            &asset.browser_download_url,
                                            version.to_string(),
                                            asset.size as u32,
                                        ) {
                                            warn!("tasks: Failed to trigger OTA update: {:?}", e);
                                        } else {
                                            info!("tasks: OTA update triggered successfully!");

                                            // Broadcast OTA start message to all nodes
                                            drop(ota_manager);
                                            drop(state_lock);

                                            let ota_start_msg = OtaMessage::OtaStart {
                                                version: version.to_string(),
                                                total_chunks: 0, // Will be updated by distribution task
                                                firmware_size: asset.size as u32,
                                            };

                                            let message = serde_json::to_string(&ota_start_msg).unwrap();
                                            let flag = 0x01;

                                            if mesh_send(&BROADCAST_ADDR, message.as_bytes(), flag).is_ok() {
                                                info!("tasks: Broadcasted OTA start message to mesh");
                                            } else {
                                                warn!("tasks: Failed to broadcast OTA start");
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        warn!("tasks: Failed to parse version from release: {:?}", e);
                                    }
                                }
                            } else {
                                warn!("tasks: No firmware binary found in release assets");
                            }
                        }
                        Ok(None) => {
                            info!("tasks: Already running latest firmware version");
                        }
                        Err(e) => {
                            warn!("tasks: Failed to check for updates: {:?}", e);
                        }
                    }
                }
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
                let flag = 0x01; // MESH_DATA_GROUP flag

                // Try sending the message up to 3 times for better reliability
                let mut success = false;
                for attempt in 1..=3 {
                    if mesh_send(&BROADCAST_ADDR, message.as_bytes(), flag).is_ok() {
                        success = true;
                        break;
                    } else {
                        warn!("tasks: Failed to send color command on attempt {}", attempt);
                        if attempt < 3 {
                            thread::sleep(Duration::from_millis(100)); // Brief delay before retry
                        }
                    }
                }

                if !success {
                    warn!("tasks: All attempts to send color command failed");
                }

                // Send packet loss test challenges every 5 seconds
                if counter % 5 == 0 {
                    _challenge_counter += 1;
                    let challenge_id = esp_random();
                    if node.send_challenge(challenge_id) {
                        info!("tasks: Sent challenge ID: {}", challenge_id);
                    } else {
                        warn!("tasks: Failed to send challenge ID: {}", challenge_id);
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
                    let current_layer = state::layer();
                    let total_nodes = get_mesh_node_count();
                    let message = format!(
                        "Status from layer {current_layer} (nodes: {total_nodes}, count: {counter})"
                    );

                    let flag = 0x01; // MESH_DATA_GROUP flag

                    if mesh_send(&BROADCAST_ADDR, message.as_bytes(), flag).is_ok() {
                        info!("tasks: Status message sent: {}", message);
                    } else {
                        warn!("tasks: Failed to send status");
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

        let is_root_node = state::is_root();
        let current_layer = state::layer();
        let is_active = is_mesh_active();
        let total_nodes = get_mesh_node_count();

        unsafe {

            *node.is_root.lock().unwrap() = is_root_node;
            *node.is_connected.lock().unwrap() = is_active;
            *node.layer.lock().unwrap() = current_layer;

            // Sync IP connectivity state
            let has_ip_status = state::has_ip();
            *node.has_ip.lock().unwrap() = has_ip_status;

            info!(
                "tasks: Status - Root: {}, Layer: {}, Active: {}, Has IP: {}, Total Nodes: {}",
                is_root_node, current_layer, is_active, has_ip_status, total_nodes
            );

            // Don't override synchronized colors - only show status when disconnected
            if !is_root_node && !is_active {
                node.update_status_color();
            }
        }
    }
}

/// Task for executing timed instructions
pub fn instruction_execution_task(node: Arc<MeshNode>, state: Arc<Mutex<ApplicationState>>) {
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

/// Task for OTA distribution (root node only)
pub fn ota_distribution_task(_node: Arc<MeshNode>, state: Arc<Mutex<ApplicationState>>) {
    loop {
        thread::sleep(Duration::from_secs(1));

        // Only root node distributes OTA updates
        if !state::is_root() {
            continue;
        }

        // Check if OTA manager has work to do
        let state_lock = state.lock().unwrap();
        let ota_state = state_lock.ota_manager.lock().unwrap().get_state();
        drop(state_lock);

        match ota_state {
            OtaState::Distributing { total_chunks, .. } => {
                // Send chunks to mesh one at a time (on-demand generation)
                info!("tasks: Starting OTA distribution of {} chunks", total_chunks);

                for i in 0..total_chunks {
                    // Generate chunk on-demand from OTA partition
                    let chunk = {
                        let state_lock = state.lock().unwrap();
                        let ota_manager = state_lock.ota_manager.lock().unwrap();

                        match ota_manager.get_chunk(i) {
                            Ok(chunk) => chunk,
                            Err(e) => {
                                error!("tasks: Failed to read chunk {}: {:?}", i, e);
                                continue;
                            }
                        }
                        // Lock is released here
                    };

                    // Send chunk to mesh
                    let ota_msg = OtaMessage::OtaChunk {
                        chunk: chunk.clone(),
                    };

                    let message = serde_json::to_string(&ota_msg).unwrap();
                    let flag = 0x01; // MESH_DATA_GROUP flag

                    if mesh_send(&BROADCAST_ADDR, message.as_bytes(), flag).is_ok() {
                        info!("tasks: Sent OTA chunk {}/{}", i + 1, total_chunks);
                    } else {
                        warn!("tasks: Failed to send OTA chunk {}", i);
                    }

                    // Small delay between chunks to avoid overwhelming the mesh
                    thread::sleep(Duration::from_millis(100));
                }

                // After sending all chunks, wait for nodes to complete
                info!("tasks: All chunks sent. Waiting for nodes to complete...");
                thread::sleep(Duration::from_secs(5));

                // Check if all nodes are ready
                let state_lock = state.lock().unwrap();
                let ota_manager = state_lock.ota_manager.lock().unwrap();
                if ota_manager.all_nodes_ready() {
                    info!("tasks: All nodes ready! Sending reboot command...");

                    // Send reboot command
                    let reboot_msg = OtaMessage::OtaReboot;
                    let message = serde_json::to_string(&reboot_msg).unwrap();
                    let flag = 0x01;

                    let _ = mesh_send(&BROADCAST_ADDR, message.as_bytes(), flag);

                    // Reboot ourselves
                    thread::sleep(Duration::from_secs(2));
                    unsafe {
                        esp_idf_sys::esp_restart();
                    }
                }
            }
            _ => {
                // No OTA in progress
            }
        }
    }
}
