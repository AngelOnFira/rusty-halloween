use anyhow::{Context, Result};
use crc::{Crc, CRC_32_ISO_HDLC};
use esp_ota::OtaUpdate;
use log::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use crate::version::{GitHubRelease, Version};

/// OTA update chunk size (bytes) - sized for mesh transmission
pub const CHUNK_SIZE: usize = 512;

/// CRC32 algorithm for chunk validation
const CRC32: Crc<u32> = Crc::<u32>::new(&CRC_32_ISO_HDLC);

/// OTA state machine states
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OtaState {
    /// No OTA in progress
    Idle,
    /// Root node: downloading firmware from GitHub
    Downloading { progress: u32, total: u32 },
    /// Root node: distributing firmware to mesh
    Distributing {
        total_chunks: u32,
        nodes_complete: u32,
        total_nodes: u32,
    },
    /// Child node: receiving firmware chunks
    Receiving {
        received_chunks: u32,
        total_chunks: u32,
    },
    /// Firmware received, waiting for reboot command
    ReadyToReboot,
    /// Rebooting
    Rebooting,
}

/// Firmware chunk for mesh distribution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirmwareChunk {
    /// Sequence number (0-based)
    pub sequence: u32,
    /// Total number of chunks
    pub total_chunks: u32,
    /// Firmware version being distributed
    pub version: String,
    /// Chunk data
    #[serde(with = "serde_bytes")]
    pub data: Vec<u8>,
    /// CRC32 checksum of data
    pub crc32: u32,
}

impl FirmwareChunk {
    /// Create a new chunk
    pub fn new(sequence: u32, total_chunks: u32, version: String, data: Vec<u8>) -> Self {
        let crc32 = CRC32.checksum(&data);
        Self {
            sequence,
            total_chunks,
            version,
            data,
            crc32,
        }
    }

    /// Validate chunk CRC
    pub fn validate(&self) -> bool {
        let calculated = CRC32.checksum(&self.data);
        if calculated != self.crc32 {
            warn!(
                "Chunk {} CRC mismatch: expected {}, got {}",
                self.sequence, self.crc32, calculated
            );
            false
        } else {
            true
        }
    }
}

/// OTA message types (to be sent over mesh)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum OtaMessage {
    /// Root → All: Check for updates command from external server
    #[serde(rename = "check_update")]
    CheckUpdate,

    /// Root → All: OTA update starting
    #[serde(rename = "ota_start")]
    OtaStart {
        version: String,
        total_chunks: u32,
        firmware_size: u32,
    },

    /// Root → All: Firmware chunk
    #[serde(rename = "ota_chunk")]
    OtaChunk { chunk: FirmwareChunk },

    /// Child → Root: Acknowledge chunk receipt
    #[serde(rename = "ota_chunk_ack")]
    OtaChunkAck { sequence: u32, success: bool },

    /// Child → Root: Request retransmission of missing chunks
    #[serde(rename = "ota_chunk_req")]
    OtaChunkRequest { sequences: Vec<u32> },

    /// Child → Root: All chunks received, ready to reboot
    #[serde(rename = "ota_complete")]
    OtaComplete,

    /// Root → All: Synchronized reboot command
    #[serde(rename = "ota_reboot")]
    OtaReboot,

    /// Root → All: OTA cancelled
    #[serde(rename = "ota_cancel")]
    OtaCancel { reason: String },
}

/// Node tracking for mesh distribution
#[derive(Debug, Clone)]
pub struct NodeProgress {
    /// MAC address as string
    pub mac: String,
    /// Chunks received by this node
    pub received_chunks: HashSet<u32>,
    /// Ready to reboot
    pub ready: bool,
}

impl NodeProgress {
    pub fn new(mac: String) -> Self {
        Self {
            mac,
            received_chunks: HashSet::new(),
            ready: false,
        }
    }

    pub fn is_complete(&self, total_chunks: u32) -> bool {
        self.received_chunks.len() == total_chunks as usize
    }
}

/// OTA subsystem handle - represents the state of the OTA subsystem
enum OtaHandle {
    /// No OTA in progress
    None,
    /// Currently receiving OTA update
    Updating(OtaUpdate),
}

/// OTA Manager - handles entire OTA process
pub struct OtaManager {
    /// Current OTA state
    state: Arc<Mutex<OtaState>>,
    /// OTA subsystem handle (transitions between Ready and Updating states)
    ota_handle: OtaHandle,
    /// Firmware chunks (for root node distribution)
    chunks: Vec<FirmwareChunk>,
    /// Node progress tracking (for root node)
    node_progress: Arc<Mutex<HashMap<String, NodeProgress>>>,
    /// Received chunks buffer (for child nodes - stores out-of-order chunks)
    received_chunks_buffer: Arc<Mutex<HashMap<u32, FirmwareChunk>>>,
    /// Next expected chunk sequence (for child nodes - tracks writing order)
    next_expected_sequence: u32,
    /// Total chunks expected (for child nodes)
    total_chunks: u32,
    /// Target firmware version
    target_version: Option<Version>,
}

impl OtaManager {
    /// Create a new OTA manager
    pub fn new() -> Result<Self> {
        Ok(Self {
            state: Arc::new(Mutex::new(OtaState::Idle)),
            ota_handle: OtaHandle::None,
            chunks: Vec::new(),
            node_progress: Arc::new(Mutex::new(HashMap::new())),
            received_chunks_buffer: Arc::new(Mutex::new(HashMap::new())),
            next_expected_sequence: 0,
            total_chunks: 0,
            target_version: None,
        })
    }

    /// Get current OTA state
    pub fn get_state(&self) -> OtaState {
        self.state.lock().unwrap().clone()
    }

    /// Check GitHub for updates (root node only)
    pub fn check_for_updates(&mut self) -> Result<Option<GitHubRelease>> {
        use crate::version::{is_update_available, GITHUB_REPO_NAME, GITHUB_REPO_OWNER};
        use embedded_svc::http::client::Client;
        use esp_idf_svc::http::client::{Configuration, EspHttpConnection};

        info!("Checking GitHub for firmware updates...");

        let url = format!(
            "https://api.github.com/repos/{}/{}/releases/latest",
            GITHUB_REPO_OWNER, GITHUB_REPO_NAME
        );

        info!("Querying GitHub API: {}", url);

        let connection = EspHttpConnection::new(&Configuration {
            buffer_size: Some(4096),
            timeout: Some(std::time::Duration::from_secs(30)),
            ..Default::default()
        })?;

        let mut client = Client::wrap(connection);

        // Make HTTP GET request
        let request = client
            .get(&url)
            .context("Failed to create GET request")?;

        // Submit request and get response
        let mut response = request.submit().context("Failed to submit request")?;

        let status = response.status();
        info!("GitHub API response status: {}", status);

        if status != 200 {
            anyhow::bail!("GitHub API request failed with status: {}", status);
        }

        // Read response body into string
        let mut json_data = String::new();
        let mut buffer = [0u8; 1024];

        loop {
            match response.read(&mut buffer) {
                Ok(0) => break, // EOF
                Ok(n) => {
                    let chunk = std::str::from_utf8(&buffer[..n])
                        .context("Invalid UTF-8 in response")?;
                    json_data.push_str(chunk);
                }
                Err(e) => {
                    anyhow::bail!("Failed to read response: {:?}", e);
                }
            }
        }

        debug!("GitHub API response: {} bytes", json_data.len());

        // Parse JSON response
        let release: GitHubRelease = serde_json::from_str(&json_data)
            .context("Failed to parse GitHub release JSON")?;

        info!(
            "Latest release: {} (tag: {})",
            release.name, release.tag_name
        );

        // Parse version from tag and check if update is available
        let latest_version = release.version()?;

        if is_update_available(&latest_version)? {
            info!(
                "✨ Update available! Current: v{}, Latest: v{}",
                crate::version::FIRMWARE_VERSION,
                latest_version
            );
            Ok(Some(release))
        } else {
            info!(
                "Already running latest version: v{}",
                crate::version::FIRMWARE_VERSION
            );
            Ok(None)
        }
    }

    /// Download firmware from GitHub (root node only)
    pub fn download_firmware(&mut self, url: &str, expected_size: u32) -> Result<Vec<u8>> {
        info!("Starting firmware download from: {}", url);

        *self.state.lock().unwrap() = OtaState::Downloading {
            progress: 0,
            total: expected_size,
        };

        use embedded_svc::http::client::Client;
        use esp_idf_svc::http::client::{Configuration, EspHttpConnection};

        let connection = EspHttpConnection::new(&Configuration {
            buffer_size: Some(4096),
            ..Default::default()
        })?;

        let mut client = Client::wrap(connection);

        // Make HTTP GET request
        let request = client
            .get(url)
            .context("Failed to create GET request")?;

        // Submit request and get response
        let mut response = request.submit().context("Failed to submit request")?;

        let status = response.status();
        info!("HTTP Response status: {}", status);

        if status != 200 {
            anyhow::bail!("HTTP request failed with status: {}", status);
        }

        // Read response body
        let mut firmware_data = Vec::new();
        let mut buffer = [0u8; 4096];
        let mut total_read = 0u32;

        loop {
            match response.read(&mut buffer) {
                Ok(0) => break, // EOF
                Ok(n) => {
                    firmware_data.extend_from_slice(&buffer[..n]);
                    total_read += n as u32;

                    // Update progress
                    *self.state.lock().unwrap() = OtaState::Downloading {
                        progress: total_read,
                        total: expected_size,
                    };

                    if total_read % (100 * 1024) == 0 {
                        info!(
                            "Downloaded: {} KB / {} KB ({:.1}%)",
                            total_read / 1024,
                            expected_size / 1024,
                            (total_read as f32 / expected_size as f32) * 100.0
                        );
                    }
                }
                Err(e) => {
                    anyhow::bail!("Failed to read response: {:?}", e);
                }
            }
        }

        info!(
            "Firmware download complete: {} bytes",
            firmware_data.len()
        );

        if firmware_data.len() != expected_size as usize {
            warn!(
                "Downloaded size ({}) doesn't match expected size ({})",
                firmware_data.len(),
                expected_size
            );
        }

        Ok(firmware_data)
    }

    /// Fragment downloaded firmware into chunks (root node only)
    pub fn fragment_firmware(&mut self, firmware: &[u8], version: String) -> Result<()> {
        info!(
            "Fragmenting firmware: {} bytes into {}-byte chunks",
            firmware.len(),
            CHUNK_SIZE
        );

        self.chunks.clear();
        let total_chunks = (firmware.len() + CHUNK_SIZE - 1) / CHUNK_SIZE;

        for (i, chunk_data) in firmware.chunks(CHUNK_SIZE).enumerate() {
            let chunk =
                FirmwareChunk::new(i as u32, total_chunks as u32, version.clone(), chunk_data.to_vec());
            self.chunks.push(chunk);
        }

        info!(
            "Firmware fragmented into {} chunks",
            self.chunks.len()
        );
        Ok(())
    }

    /// Get chunk by sequence number (root node only)
    pub fn get_chunk(&self, sequence: u32) -> Option<&FirmwareChunk> {
        self.chunks.get(sequence as usize)
    }

    /// Get all chunks (root node only)
    pub fn get_all_chunks(&self) -> &[FirmwareChunk] {
        &self.chunks
    }

    /// Start OTA update (child node only)
    pub fn start_ota_reception(&mut self, total_chunks: u32, firmware_size: u32) -> Result<()> {
        info!(
            "Starting OTA reception: {} chunks, {} bytes",
            total_chunks, firmware_size
        );

        self.total_chunks = total_chunks;
        self.next_expected_sequence = 0;
        self.received_chunks_buffer.lock().unwrap().clear();

        // Begin OTA update - this erases the next OTA partition
        let update = OtaUpdate::begin()
            .map_err(|e| anyhow::anyhow!("Failed to begin OTA update: {:?}", e))?;

        self.ota_handle = OtaHandle::Updating(update);

        *self.state.lock().unwrap() = OtaState::Receiving {
            received_chunks: 0,
            total_chunks,
        };

        Ok(())
    }

    /// Handle chunk reception (child node only)
    pub fn handle_chunk(&mut self, chunk: FirmwareChunk) -> Result<bool> {
        // Validate chunk
        if !chunk.validate() {
            warn!("Chunk {} failed CRC validation", chunk.sequence);
            return Ok(false);
        }

        // Store chunk in buffer if out of order
        if chunk.sequence != self.next_expected_sequence {
            debug!(
                "Buffering out-of-order chunk {} (expecting {})",
                chunk.sequence, self.next_expected_sequence
            );
            self.received_chunks_buffer
                .lock()
                .unwrap()
                .insert(chunk.sequence, chunk);
            return Ok(false);
        }

        // Write this chunk
        self.write_chunk_to_ota(&chunk)?;

        // Process any buffered chunks that are now in sequence
        loop {
            let next_seq = self.next_expected_sequence;
            let buffered_chunk = self
                .received_chunks_buffer
                .lock()
                .unwrap()
                .remove(&next_seq);

            if let Some(buffered) = buffered_chunk {
                self.write_chunk_to_ota(&buffered)?;
            } else {
                break;
            }
        }

        // Update state
        let received = self.next_expected_sequence;
        info!(
            "Received chunk {}/{} ({:.1}%)",
            received,
            chunk.total_chunks,
            (received as f32 / chunk.total_chunks as f32) * 100.0
        );

        *self.state.lock().unwrap() = OtaState::Receiving {
            received_chunks: received,
            total_chunks: chunk.total_chunks,
        };

        // Check if complete
        if received == chunk.total_chunks {
            info!("All chunks received! Finalizing OTA update...");
            self.finalize_ota()?;
            *self.state.lock().unwrap() = OtaState::ReadyToReboot;
            return Ok(true);
        }

        Ok(false)
    }

    /// Write a single chunk to OTA partition (in sequence)
    fn write_chunk_to_ota(&mut self, chunk: &FirmwareChunk) -> Result<()> {
        if let OtaHandle::Updating(update) = &mut self.ota_handle {
            update
                .write(&chunk.data)
                .map_err(|e| anyhow::anyhow!("Failed to write chunk to OTA partition: {:?}", e))?;

            self.next_expected_sequence += 1;
            Ok(())
        } else {
            anyhow::bail!("No active OTA update")
        }
    }

    /// Finalize OTA update
    fn finalize_ota(&mut self) -> Result<()> {
        info!("Finalizing OTA update...");

        // Take ownership of the Updating variant and transition to None
        let ota_handle = std::mem::replace(&mut self.ota_handle, OtaHandle::None);
        if let OtaHandle::Updating(update) = ota_handle {
            // Finalize the update (validates image)
            let mut completed = update
                .finalize()
                .map_err(|e| anyhow::anyhow!("Failed to finalize OTA update: {:?}", e))?;

            // Set as boot partition
            completed
                .set_as_boot_partition()
                .map_err(|e| anyhow::anyhow!("Failed to set boot partition: {:?}", e))?;

            // Mark app as valid immediately after setting boot partition
            // This confirms the new firmware is ready and prevents rollback
            // Only called here after OTA completes, NOT on every boot
            esp_ota::mark_app_valid();

            info!("OTA update finalized successfully - will boot into new firmware on restart");
            info!("New firmware marked as valid - rollback cancelled");
            Ok(())
        } else {
            anyhow::bail!("No active OTA update")
        }
    }

    /// Get missing chunks (child node only)
    pub fn get_missing_chunks(&self) -> Vec<u32> {
        if self.total_chunks == 0 {
            return Vec::new();
        }

        let buffer = self.received_chunks_buffer.lock().unwrap();
        let written_up_to = self.next_expected_sequence;

        // Missing chunks are those we haven't written yet and aren't in the buffer
        (written_up_to..self.total_chunks)
            .filter(|seq| !buffer.contains_key(seq))
            .collect()
    }

    /// Handle chunk acknowledgment (root node only)
    pub fn handle_chunk_ack(&mut self, mac: String, sequence: u32, success: bool) {
        let mut progress = self.node_progress.lock().unwrap();
        let node = progress
            .entry(mac.clone())
            .or_insert_with(|| NodeProgress::new(mac.clone()));

        if success {
            node.received_chunks.insert(sequence);
            debug!("Node {} acknowledged chunk {}", mac, sequence);
        } else {
            warn!("Node {} failed to receive chunk {}", mac, sequence);
        }
    }

    /// Handle node completion (root node only)
    pub fn handle_node_complete(&mut self, mac: String) {
        let mut progress = self.node_progress.lock().unwrap();
        if let Some(node) = progress.get_mut(&mac) {
            node.ready = true;
            info!("Node {} is ready to reboot", mac);
        }
    }

    /// Check if all nodes are ready to reboot (root node only)
    pub fn all_nodes_ready(&self) -> bool {
        let progress = self.node_progress.lock().unwrap();
        if progress.is_empty() {
            return false;
        }
        progress.values().all(|node| node.ready)
    }

    /// Mark firmware as valid after successful boot
    /// This should be called on first boot after an OTA update to prevent rollback
    pub fn mark_valid(&self) -> Result<()> {
        info!("Marking current firmware as valid...");
        esp_ota::mark_app_valid();
        info!("Firmware marked as valid - rollback cancelled");
        Ok(())
    }

    /// Get current running firmware version
    pub fn get_running_version(&self) -> Result<String> {
        use crate::version::FIRMWARE_VERSION;
        Ok(FIRMWARE_VERSION.to_string())
    }

    /// Trigger complete OTA update workflow (root node only)
    /// Downloads firmware from URL, fragments it, and prepares for distribution
    pub fn trigger_ota_update(&mut self, firmware_url: &str, version: String, firmware_size: u32) -> Result<()> {
        info!("🚀 Triggering OTA update to version: {}", version);

        // Download firmware
        let firmware_data = self.download_firmware(firmware_url, firmware_size)?;

        // Fragment firmware
        self.fragment_firmware(&firmware_data, version.clone())?;

        // Update state to distributing
        *self.state.lock().unwrap() = OtaState::Distributing {
            total_chunks: self.chunks.len() as u32,
            nodes_complete: 0,
            total_nodes: 0, // Will be updated as nodes respond
        };

        info!(
            "OTA update prepared: {} chunks ready for distribution",
            self.chunks.len()
        );

        Ok(())
    }

    /// Trigger reboot
    pub fn reboot(&self) -> ! {
        info!("Rebooting device...");
        unsafe {
            esp_idf_sys::esp_restart();
        }
    }
}

// Serde helper for byte arrays
mod serde_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bytes(bytes)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Vec::<u8>::deserialize(deserializer)
    }
}
