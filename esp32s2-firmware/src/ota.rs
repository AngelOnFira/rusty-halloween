use defmt::{debug, error, info, warn};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use heapless::{String, Vec};

use crate::protocol::{OtaChunkData, OtaStartData};

/// Maximum number of chunks to track
const MAX_CHUNKS: usize = 512; // For ~100KB firmware with 200-byte chunks

/// OTA state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OtaState {
    /// Idle, no update in progress
    Idle,
    /// Update announced, waiting for chunks
    WaitingForChunks,
    /// Receiving chunks
    ReceivingChunks,
    /// All chunks received, ready to apply
    ReadyToApply,
    /// Applying update
    Applying,
    /// Update failed
    Failed,
}

/// OTA chunk status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChunkStatus {
    /// Chunk not yet received
    Missing,
    /// Chunk received successfully
    Received,
}

/// OTA manager
pub struct OtaManager {
    /// Current OTA state
    state: OtaState,
    /// Firmware version being received
    version: String<16>,
    /// Total number of chunks
    total_chunks: u32,
    /// Total firmware size
    firmware_size: u32,
    /// Chunk status tracking
    chunk_status: Vec<ChunkStatus, MAX_CHUNKS>,
    /// Number of chunks received
    chunks_received: u32,
    /// Firmware buffer (stored in flash partitions in real impl)
    /// For now, we just track receipt, not store the data
    last_chunk_sequence: u32,
}

impl OtaManager {
    pub fn new() -> Self {
        Self {
            state: OtaState::Idle,
            version: String::new(),
            total_chunks: 0,
            firmware_size: 0,
            chunk_status: Vec::new(),
            chunks_received: 0,
            last_chunk_sequence: 0,
        }
    }

    /// Start a new OTA update
    pub fn start_update(&mut self, start_data: &OtaStartData) -> Result<(), ()> {
        if self.state != OtaState::Idle && self.state != OtaState::Failed {
            warn!("OTA already in progress");
            return Err(());
        }

        info!(
            "Starting OTA update: version={}, chunks={}, size={}",
            start_data.version.as_str(),
            start_data.total_chunks,
            start_data.firmware_size
        );

        self.state = OtaState::WaitingForChunks;
        self.version = start_data.version.clone();
        self.total_chunks = start_data.total_chunks;
        self.firmware_size = start_data.firmware_size;
        self.chunks_received = 0;
        self.last_chunk_sequence = 0;

        // Initialize chunk status
        self.chunk_status.clear();
        for _ in 0..start_data.total_chunks {
            if self.chunk_status.push(ChunkStatus::Missing).is_err() {
                error!("Too many chunks for buffer");
                self.state = OtaState::Failed;
                return Err(());
            }
        }

        self.state = OtaState::ReceivingChunks;
        Ok(())
    }

    /// Handle received OTA chunk
    pub fn handle_chunk(&mut self, chunk: &OtaChunkData, node_mac: [u8; 6]) -> Result<bool, ()> {
        if self.state != OtaState::ReceivingChunks {
            warn!("Not in receiving state, ignoring chunk");
            return Err(());
        }

        // Validate chunk
        if chunk.sequence >= self.total_chunks {
            error!("Invalid chunk sequence: {}", chunk.sequence);
            return Err(());
        }

        if chunk.version != self.version {
            error!("Version mismatch in chunk");
            return Err(());
        }

        // Check if we already have this chunk
        if self.chunk_status[chunk.sequence as usize] == ChunkStatus::Received {
            debug!("Chunk {} already received", chunk.sequence);
            return Ok(false); // Already have it
        }

        // Verify CRC (simplified check)
        let calculated_crc = crc::Crc::<u32>::new(&crc::CRC_32_ISCSI).checksum(&chunk.data);
        if calculated_crc != chunk.crc32 {
            error!("CRC mismatch for chunk {}", chunk.sequence);
            return Err(());
        }

        // Store chunk (in real implementation, write to flash partition)
        // For now, just mark as received
        self.chunk_status[chunk.sequence as usize] = ChunkStatus::Received;
        self.chunks_received += 1;
        self.last_chunk_sequence = chunk.sequence;

        info!(
            "Chunk {}/{} received from {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            self.chunks_received,
            self.total_chunks,
            node_mac[0], node_mac[1], node_mac[2],
            node_mac[3], node_mac[4], node_mac[5]
        );

        // Check if all chunks received
        if self.chunks_received == self.total_chunks {
            info!("All OTA chunks received!");
            self.state = OtaState::ReadyToApply;
        }

        Ok(true) // New chunk received
    }

    /// Get list of missing chunks for retransmission request
    pub fn get_missing_chunks(&self) -> Vec<u32, 32> {
        let mut missing = Vec::new();
        for (i, status) in self.chunk_status.iter().enumerate() {
            if *status == ChunkStatus::Missing {
                if missing.push(i as u32).is_err() {
                    break; // Vec full
                }
            }
        }
        missing
    }

    /// Check if ready to apply update
    pub fn is_ready_to_apply(&self) -> bool {
        self.state == OtaState::ReadyToApply
    }

    /// Apply the OTA update
    /// In a real implementation, this would:
    /// 1. Verify all chunks
    /// 2. Write to OTA partition
    /// 3. Switch boot partition
    /// 4. Set rollback protection
    pub fn apply_update(&mut self) -> Result<(), ()> {
        if self.state != OtaState::ReadyToApply {
            error!("OTA not ready to apply");
            return Err(());
        }

        info!("Applying OTA update (placeholder - not actually implemented)");
        self.state = OtaState::Applying;

        // TODO: Actual implementation would:
        // - Write chunks to OTA partition
        // - Validate entire firmware
        // - Mark partition as bootable
        // - Schedule reboot

        // For now, just mark as complete
        warn!("OTA apply not fully implemented - would reboot here");
        self.state = OtaState::Idle;

        Ok(())
    }

    /// Reset OTA state
    pub fn reset(&mut self) {
        self.state = OtaState::Idle;
        self.version.clear();
        self.total_chunks = 0;
        self.firmware_size = 0;
        self.chunk_status.clear();
        self.chunks_received = 0;
        info!("OTA state reset");
    }

    /// Get current state
    pub fn state(&self) -> OtaState {
        self.state
    }

    /// Get progress percentage
    pub fn progress_percent(&self) -> u32 {
        if self.total_chunks == 0 {
            return 0;
        }
        (self.chunks_received * 100) / self.total_chunks
    }
}

/// Global OTA manager instance
pub static OTA_MANAGER: Mutex<CriticalSectionRawMutex, Option<OtaManager>> = Mutex::new(None);

/// Initialize OTA manager
pub async fn init_ota_manager() {
    let mut ota = OTA_MANAGER.lock().await;
    *ota = Some(OtaManager::new());
    info!("OTA manager initialized");
}

/// Start OTA update
pub async fn start_update(start_data: &OtaStartData) -> Result<(), ()> {
    let mut ota = OTA_MANAGER.lock().await;
    if let Some(manager) = ota.as_mut() {
        manager.start_update(start_data)
    } else {
        Err(())
    }
}

/// Handle OTA chunk
pub async fn handle_chunk(chunk: &OtaChunkData, node_mac: [u8; 6]) -> Result<bool, ()> {
    let mut ota = OTA_MANAGER.lock().await;
    if let Some(manager) = ota.as_mut() {
        manager.handle_chunk(chunk, node_mac)
    } else {
        Err(())
    }
}

/// Get missing chunks
pub async fn get_missing_chunks() -> Vec<u32, 32> {
    let ota = OTA_MANAGER.lock().await;
    if let Some(manager) = ota.as_ref() {
        manager.get_missing_chunks()
    } else {
        Vec::new()
    }
}

/// Check if ready to apply
pub async fn is_ready_to_apply() -> bool {
    let ota = OTA_MANAGER.lock().await;
    if let Some(manager) = ota.as_ref() {
        manager.is_ready_to_apply()
    } else {
        false
    }
}

/// Apply update
pub async fn apply_update() -> Result<(), ()> {
    let mut ota = OTA_MANAGER.lock().await;
    if let Some(manager) = ota.as_mut() {
        manager.apply_update()
    } else {
        Err(())
    }
}

/// Get progress
pub async fn get_progress() -> (u32, u32) {
    let ota = OTA_MANAGER.lock().await;
    if let Some(manager) = ota.as_ref() {
        (manager.chunks_received, manager.total_chunks)
    } else {
        (0, 0)
    }
}

/// Reset OTA state
pub async fn reset() {
    let mut ota = OTA_MANAGER.lock().await;
    if let Some(manager) = ota.as_mut() {
        manager.reset();
    }
}
