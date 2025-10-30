//! OTA state transitions and business logic

use super::{types::*, GLOBAL_STATE};
use anyhow::{Context, Result};
use crc::{Crc, CRC_32_ISO_HDLC};
use esp_idf_svc::sys as sys;
use esp_idf_svc::ota::{EspOta, EspOtaUpdate};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    marker::PhantomData,
    sync::{Arc, Mutex},
};

use crate::version::{GitHubRelease, Version};

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
        info!("state::ota: Beginning OTA download: v{} ({} bytes) from {}", version, firmware_size, firmware_url);

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
        })
    }

    /// Begin OTA reception (child node only)
    /// Transitions from Idle → Receiving
    pub fn begin_ota_reception(self, total_chunks: u32, firmware_size: u32)
        -> anyhow::Result<WifiMeshState<W, M, S, OtaReceiving>>
    {
        info!("state::ota: Beginning OTA reception: {} chunks ({} bytes)", total_chunks, firmware_size);

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
        info!("state::ota: Completing OTA download, fragmenting firmware...");

        // Fragment firmware into chunks
        let total_chunks = (firmware_data.len() + CHUNK_SIZE - 1) / CHUNK_SIZE;
        let version = GLOBAL_STATE.lock().unwrap()
            .as_ref()
            .and_then(|s| s.ota_data.target_version.clone())
            .unwrap_or_else(|| "unknown".to_string());

        let mut chunks = Vec::new();
        for (i, chunk_data) in firmware_data.chunks(CHUNK_SIZE).enumerate() {
            let chunk = FirmwareChunk::new(
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

        info!("state::ota: Firmware fragmented into {} chunks, ready to distribute", total_chunks);

        Ok(WifiMeshState {
            _wifi_mode: PhantomData,
            _mesh_state: PhantomData,
            _scan_state: PhantomData,
            _ota_state: PhantomData,
        })
    }

    /// Cancel OTA and return to idle
    pub fn cancel_ota(self) -> WifiMeshState<W, M, S, OtaIdle> {
        info!("state::ota: Cancelling OTA download");

        if let Some(state) = GLOBAL_STATE.lock().unwrap().as_mut() {
            state.ota_state = OtaStateRuntime::Idle;
            *state.ota_data_mut() = OtaRuntimeData::new();
        }

        WifiMeshState {
            _wifi_mode: PhantomData,
            _mesh_state: PhantomData,
            _scan_state: PhantomData,
            _ota_state: PhantomData,
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
        info!("state::ota: All nodes ready, transitioning to ready to reboot");

        if let Some(state) = GLOBAL_STATE.lock().unwrap().as_mut() {
            state.ota_state = OtaStateRuntime::ReadyToReboot;
        }

        Ok(WifiMeshState {
            _wifi_mode: PhantomData,
            _mesh_state: PhantomData,
            _scan_state: PhantomData,
            _ota_state: PhantomData,
        })
    }

    /// Cancel OTA and return to idle
    pub fn cancel_ota(self) -> WifiMeshState<W, M, S, OtaIdle> {
        info!("state::ota: Cancelling OTA distribution");

        if let Some(state) = GLOBAL_STATE.lock().unwrap().as_mut() {
            state.ota_state = OtaStateRuntime::Idle;
            *state.ota_data_mut() = OtaRuntimeData::new();
        }

        WifiMeshState {
            _wifi_mode: PhantomData,
            _mesh_state: PhantomData,
            _scan_state: PhantomData,
            _ota_state: PhantomData,
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
        info!("state::ota: All chunks received and validated, transitioning to ready to reboot");

        if let Some(state) = GLOBAL_STATE.lock().unwrap().as_mut() {
            state.ota_state = OtaStateRuntime::ReadyToReboot;
        }

        Ok(WifiMeshState {
            _wifi_mode: PhantomData,
            _mesh_state: PhantomData,
            _scan_state: PhantomData,
            _ota_state: PhantomData,
        })
    }

    /// Cancel OTA and return to idle
    pub fn cancel_ota(self) -> WifiMeshState<W, M, S, OtaIdle> {
        info!("state::ota: Cancelling OTA reception");

        if let Some(state) = GLOBAL_STATE.lock().unwrap().as_mut() {
            state.ota_state = OtaStateRuntime::Idle;
            *state.ota_data_mut() = OtaRuntimeData::new();
        }

        WifiMeshState {
            _wifi_mode: PhantomData,
            _mesh_state: PhantomData,
            _scan_state: PhantomData,
            _ota_state: PhantomData,
        }
    }
}

/// OTA Ready to Reboot state
impl<W, M, S> WifiMeshState<W, M, S, OtaReadyToReboot> {
    /// Reboot the device (never returns)
    pub fn reboot(self) -> ! {
        info!("state::ota: Rebooting device to apply OTA update...");
        unsafe {
            sys::esp_restart();
        }
    }

    /// Cancel OTA and return to idle (in case of abort before reboot)
    pub fn cancel_ota(self) -> WifiMeshState<W, M, S, OtaIdle> {
        info!("state::ota: Cancelling OTA before reboot");

        if let Some(state) = GLOBAL_STATE.lock().unwrap().as_mut() {
            state.ota_state = OtaStateRuntime::Idle;
            *state.ota_data_mut() = OtaRuntimeData::new();
        }

        WifiMeshState {
            _wifi_mode: PhantomData,
            _mesh_state: PhantomData,
            _scan_state: PhantomData,
            _ota_state: PhantomData,
        }
    }
}

// Keep OtaActive for backwards compatibility
impl<W, M, S> WifiMeshState<W, M, S, OtaActive> {
    /// Complete OTA operation and return to idle state.
    /// Call this whether OTA succeeded or failed.
    pub fn finish_ota(self) -> WifiMeshState<W, M, S, OtaIdle> {
        info!("state::ota: Finishing OTA operation");

        // Update global state
        if let Some(state) = GLOBAL_STATE.lock().unwrap().as_mut() {
            state.ota_state = OtaStateRuntime::Idle;
        }

        WifiMeshState {
            _wifi_mode: PhantomData,
            _mesh_state: PhantomData,
            _scan_state: PhantomData,
            _ota_state: PhantomData,
        }
    }

    /// Query if OTA is currently active
    /// (Always returns true for this state, provided for API consistency)
    pub fn is_ota_active(&self) -> bool {
        true
    }
}

// =============================================================================
// OTA Business Logic (Manager, Messages, Chunks)
// =============================================================================

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
                "state::ota: Chunk {} CRC mismatch: expected {}, got {}",
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
    Updating(EspOtaUpdate<'static>),
}

/// OTA Manager - handles entire OTA process
pub struct OtaManager {
    /// Current OTA state
    state: Arc<Mutex<OtaState>>,
    /// EspOta instance for managing OTA operations (raw pointer to leaked Box)
    /// Safety: This points to a leaked Box that lives for 'static, only accessed through &mut
    esp_ota: *mut EspOta,
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
    /// Firmware size for root node (used for chunk generation from partition)
    firmware_size: u32,
    /// Total chunks for root node distribution
    firmware_total_chunks: u32,
    /// Firmware version for root node distribution
    firmware_version: String,
}

// Safety: OtaManager can be safely sent between threads because:
// - esp_ota points to a leaked Box that lives for 'static and is thread-safe
// - All other fields are Send
unsafe impl Send for OtaManager {}

impl OtaManager {
    /// Create a new OTA manager
    pub fn new() -> Result<Self> {
        let esp_ota = Box::new(EspOta::new().context("Failed to create EspOta instance")?);
        // Leak the EspOta instance and store as raw pointer
        // This is fine since OtaManager is a singleton that lives for the program lifetime
        let esp_ota: *mut EspOta = Box::leak(esp_ota) as *mut EspOta;

        Ok(Self {
            state: Arc::new(Mutex::new(OtaState::Idle)),
            esp_ota,
            ota_handle: OtaHandle::None,
            chunks: Vec::new(),
            node_progress: Arc::new(Mutex::new(HashMap::new())),
            received_chunks_buffer: Arc::new(Mutex::new(HashMap::new())),
            next_expected_sequence: 0,
            total_chunks: 0,
            target_version: None,
            firmware_size: 0,
            firmware_total_chunks: 0,
            firmware_version: String::new(),
        })
    }

    /// Get current OTA state
    pub fn get_state(&self) -> OtaState {
        self.state.lock().unwrap().clone()
    }

    /// Check GitHub for updates (root node only)
    pub fn check_for_updates(&mut self) -> Result<Option<GitHubRelease>> {
        use crate::version::{is_update_available, SHOW_SERVER_URL};
        use embedded_svc::http::client::Client;
        use esp_idf_svc::http::client::{Configuration, EspHttpConnection};

        // Log available heap before TLS operations
        let free_heap = unsafe { esp_idf_sys::esp_get_free_heap_size() };
        info!("state::ota: Free heap before API call: {} bytes ({} KB)",
              free_heap, free_heap / 1024);

        info!("state::ota: Checking show server for firmware updates...");

        let url = format!("{}/firmware/latest", SHOW_SERVER_URL);

        info!("state::ota: Querying show server: {}", url);

        let connection = EspHttpConnection::new(&Configuration {
            buffer_size: Some(4096),
            buffer_size_tx: Some(512),  // Reduced from 4096 - no more long GitHub redirect URLs!
            timeout: Some(std::time::Duration::from_secs(30)),
            crt_bundle_attach: Some(esp_idf_sys::esp_crt_bundle_attach),
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
        info!("state::ota: GitHub API response status: {}", status);

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

        debug!("state::ota: GitHub API response: {} bytes", json_data.len());

        // Parse JSON response
        let release: GitHubRelease = serde_json::from_str(&json_data)
            .context("Failed to parse GitHub release JSON")?;

        info!(
            "state::ota: Latest release: {} (version: {})",
            release.name, release.version
        );

        // Parse version from tag and check if update is available
        let latest_version = release.version()?;

        if is_update_available(&latest_version)? {
            info!(
                "state::ota: Update available! Current: v{}, Latest: v{}",
                crate::version::FIRMWARE_VERSION,
                latest_version
            );
            Ok(Some(release))
        } else {
            info!(
                "state::ota: Already running latest version: v{}",
                crate::version::FIRMWARE_VERSION
            );
            Ok(None)
        }
    }

    /// Download firmware from server and write directly to OTA partition (root node only)
    /// Uses chunked downloads with HTTP Range requests for reliability
    pub fn download_firmware(&mut self, url: &str, expected_size: u32) -> Result<()> {
        // Log available heap before TLS operations
        let free_heap = unsafe { esp_idf_sys::esp_get_free_heap_size() };
        info!("state::ota: Free heap before firmware download: {} bytes ({} KB)",
              free_heap, free_heap / 1024);

        info!("state::ota: Starting chunked firmware download from: {}", url);
        info!("state::ota: Total size: {} KB, using 50KB chunks with retry", expected_size / 1024);

        // Reset watchdog before long erase operation
        unsafe {
            sys::esp_task_wdt_reset();
        }

        // Initiate OTA update - erases sectors incrementally as we write
        let mut update = unsafe { (*self.esp_ota).initiate_update() }
            .context("Failed to initiate OTA update")?;

        *self.state.lock().unwrap() = OtaState::Downloading {
            progress: 0,
            total: expected_size,
        };

        // Download in 50KB chunks with retry
        const CHUNK_SIZE: u32 = 50 * 1024; // 50KB chunks (conservative for TLS stability)
        const MAX_RETRIES: u32 = 3;

        let mut offset = 0u32;
        let mut total_downloaded = 0u32;

        while offset < expected_size {
            let chunk_end = core::cmp::min(offset + CHUNK_SIZE, expected_size);
            let chunk_size = chunk_end - offset;

            // Try downloading this chunk with retries
            let mut retry_count = 0;
            loop {
                match self.download_chunk(url, offset, chunk_end - 1, &mut update) {
                    Ok(bytes_written) => {
                        offset += bytes_written;
                        total_downloaded += bytes_written;

                        // Update progress
                        *self.state.lock().unwrap() = OtaState::Downloading {
                            progress: total_downloaded,
                            total: expected_size,
                        };

                        info!(
                            "state::ota: Progress: {} KB / {} KB ({:.1}%)",
                            total_downloaded / 1024,
                            expected_size / 1024,
                            (total_downloaded as f32 / expected_size as f32) * 100.0
                        );
                        break; // Chunk succeeded, move to next
                    }
                    Err(e) => {
                        retry_count += 1;
                        if retry_count >= MAX_RETRIES {
                            anyhow::bail!("Failed to download chunk after {} retries: {:?}", MAX_RETRIES, e);
                        }
                        warn!(
                            "state::ota: Chunk download failed (retry {}/{}): {:?}",
                            retry_count, MAX_RETRIES, e
                        );
                        // Wait a bit before retry
                        std::thread::sleep(std::time::Duration::from_millis(500));
                    }
                }
            }
        }

        info!(
            "state::ota: Firmware download complete: {} bytes written to OTA partition",
            total_downloaded
        );

        if total_downloaded != expected_size {
            warn!(
                "state::ota: Downloaded size ({}) doesn't match expected size ({})",
                total_downloaded,
                expected_size
            );
        }

        // Store the OTA handle and firmware metadata for later use
        self.ota_handle = OtaHandle::Updating(update);
        self.firmware_size = total_downloaded;

        Ok(())
    }

    /// Download a single chunk using HTTP Range header
    fn download_chunk(
        &mut self,
        url: &str,
        start: u32,
        end: u32,
        update: &mut esp_idf_svc::ota::EspOtaUpdate,
    ) -> Result<u32> {
        use embedded_svc::http::client::Client;
        use esp_idf_svc::http::client::{Configuration, EspHttpConnection};

        debug!(
            "state::ota: Downloading chunk: bytes {}-{} ({} KB)",
            start,
            end,
            (end - start + 1) / 1024
        );

        let connection = EspHttpConnection::new(&Configuration {
            buffer_size: Some(4096),
            buffer_size_tx: Some(512),
            crt_bundle_attach: Some(esp_idf_sys::esp_crt_bundle_attach),
            timeout: Some(std::time::Duration::from_secs(30)),
            ..Default::default()
        })?;

        let mut client = Client::wrap(connection);

        // Create HTTP GET request with Range header
        let range_header = format!("bytes={}-{}", start, end);
        let headers = [("Range", range_header.as_str())];

        let request = client
            .request(embedded_svc::http::Method::Get, url, &headers)
            .context("Failed to create GET request with Range header")?;

        // Submit request and get response
        let mut response = request.submit().context("Failed to submit request")?;

        let status = response.status();

        // Accept 200 (full response) or 206 (partial content)
        if status != 200 && status != 206 {
            anyhow::bail!("HTTP request failed with status: {}", status);
        }

        // Read response body and write to OTA partition
        let mut buffer = [0u8; 4096];
        let mut bytes_written = 0u32;
        let expected_chunk_size = end - start + 1;

        loop {
            match response.read(&mut buffer) {
                Ok(0) => break, // EOF
                Ok(n) => {
                    // Write to OTA partition
                    update
                        .write(&buffer[..n])
                        .map_err(|e| anyhow::anyhow!("Failed to write to OTA partition: {:?}", e))?;

                    bytes_written += n as u32;

                    // Safety check: don't write more than expected
                    if bytes_written > expected_chunk_size {
                        anyhow::bail!("Received more data than expected for chunk");
                    }
                }
                Err(e) => {
                    anyhow::bail!("Failed to read chunk response: {:?}", e);
                }
            }
        }

        if bytes_written != expected_chunk_size {
            anyhow::bail!(
                "Chunk incomplete: got {} bytes, expected {}",
                bytes_written,
                expected_chunk_size
            );
        }

        Ok(bytes_written)
    }

    /// Set up firmware metadata for chunk distribution (root node only)
    /// Instead of loading all chunks into RAM, we store metadata and generate chunks on-demand
    pub fn prepare_distribution(&mut self, version: String) -> Result<()> {
        let total_chunks = (self.firmware_size as usize + CHUNK_SIZE - 1) / CHUNK_SIZE;

        info!(
            "state::ota: Preparing distribution: {} bytes = {} chunks of {} bytes",
            self.firmware_size,
            total_chunks,
            CHUNK_SIZE
        );

        // Store metadata for on-demand chunk generation
        self.firmware_total_chunks = total_chunks as u32;
        self.firmware_version = version;

        // Clear any old chunks (we won't use the chunks vec anymore)
        self.chunks.clear();

        info!(
            "state::ota: Ready to distribute {} chunks (on-demand generation)",
            self.firmware_total_chunks
        );
        Ok(())
    }

    /// Read a chunk from the OTA partition (root node only)
    /// This allows on-demand chunk generation without storing all chunks in RAM
    fn read_chunk_from_partition(&self, sequence: u32) -> Result<FirmwareChunk> {
        use esp_idf_sys as sys;

        if sequence >= self.firmware_total_chunks {
            anyhow::bail!("Chunk sequence {} out of range (total: {})", sequence, self.firmware_total_chunks);
        }

        // Calculate offset and size for this chunk
        let offset = sequence as usize * CHUNK_SIZE;
        let remaining = self.firmware_size as usize - offset;
        let chunk_size = remaining.min(CHUNK_SIZE);

        // Allocate buffer for chunk data
        let mut chunk_data = vec![0u8; chunk_size];

        // Get the running partition (where we wrote the firmware)
        let update_partition = unsafe {
            let running_partition = sys::esp_ota_get_running_partition();
            if running_partition.is_null() {
                anyhow::bail!("Failed to get running partition");
            }

            // Get the next update partition (where our downloaded firmware is)
            let next_partition = sys::esp_ota_get_next_update_partition(std::ptr::null());
            if next_partition.is_null() {
                anyhow::bail!("Failed to get next update partition");
            }
            next_partition
        };

        // Read data from partition
        let ret = unsafe {
            sys::esp_partition_read(
                update_partition,
                offset,
                chunk_data.as_mut_ptr() as *mut std::ffi::c_void,
                chunk_size,
            )
        };

        if ret != sys::ESP_OK {
            anyhow::bail!("Failed to read from partition: error code {}", ret);
        }

        // Create chunk with CRC
        let chunk = FirmwareChunk::new(
            sequence,
            self.firmware_total_chunks,
            self.firmware_version.clone(),
            chunk_data,
        );

        Ok(chunk)
    }

    /// Get chunk by sequence number (root node only)
    /// This now generates chunks on-demand from the OTA partition
    pub fn get_chunk(&self, sequence: u32) -> Result<FirmwareChunk> {
        self.read_chunk_from_partition(sequence)
    }

    /// Get total number of chunks (root node only)
    pub fn get_total_chunks(&self) -> u32 {
        self.firmware_total_chunks
    }

    /// Get all chunks (root node only) - DEPRECATED
    /// WARNING: This loads all chunks into memory and may cause OOM!
    /// Use get_chunk() in a loop instead for large firmware.
    #[deprecated(note = "Use get_chunk() in a loop instead to avoid OOM")]
    pub fn get_all_chunks(&self) -> &[FirmwareChunk] {
        &self.chunks
    }

    /// Start OTA update (child node only)
    pub fn start_ota_reception(&mut self, total_chunks: u32, firmware_size: u32) -> Result<()> {
        info!(
            "state::ota: Starting OTA reception: {} chunks, {} bytes",
            total_chunks, firmware_size
        );

        self.total_chunks = total_chunks;
        self.next_expected_sequence = 0;
        self.received_chunks_buffer.lock().unwrap().clear();

        // Initiate OTA update - erases sectors incrementally as we write
        // Safety: esp_ota points to a leaked Box that lives for 'static
        let update = unsafe { (*self.esp_ota).initiate_update() }
            .context("Failed to initiate OTA update")?;

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
            warn!("state::ota: Chunk {} failed CRC validation", chunk.sequence);
            return Ok(false);
        }

        // Store chunk in buffer if out of order
        if chunk.sequence != self.next_expected_sequence {
            debug!(
                "state::ota: Buffering out-of-order chunk {} (expecting {})",
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
            "state::ota: Received chunk {}/{} ({}%)",
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
            info!("state::ota: All chunks received! Finalizing OTA update...");
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
        info!("state::ota: Finalizing OTA update...");

        // Take ownership of the Updating variant and transition to None
        let ota_handle = std::mem::replace(&mut self.ota_handle, OtaHandle::None);
        if let OtaHandle::Updating(update) = ota_handle {
            // Complete the update (validates image and sets as boot partition)
            update
                .complete()
                .context("Failed to complete OTA update")?;

            info!("state::ota: OTA update finalized successfully - will boot into new firmware on restart");
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
            debug!("state::ota: Node {} acknowledged chunk {}", mac, sequence);
        } else {
            warn!("state::ota: Node {} failed to receive chunk {}", mac, sequence);
        }
    }

    /// Handle node completion (root node only)
    pub fn handle_node_complete(&mut self, mac: String) {
        let mut progress = self.node_progress.lock().unwrap();
        if let Some(node) = progress.get_mut(&mac) {
            node.ready = true;
            info!("state::ota: Node {} is ready to reboot", mac);
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
    pub fn mark_valid(&mut self) -> Result<()> {
        info!("state::ota: Marking current firmware as valid...");
        // Safety: esp_ota points to a leaked Box that lives for 'static
        unsafe { (*self.esp_ota).mark_running_slot_valid() }
            .context("Failed to mark running slot as valid")?;
        info!("state::ota: Firmware marked as valid - rollback cancelled");
        Ok(())
    }

    /// Get current running firmware version
    pub fn get_running_version(&self) -> Result<String> {
        // Safety: esp_ota points to a leaked Box that lives for 'static
        let running_slot = unsafe { (*self.esp_ota).get_running_slot() }
            .context("Failed to get running slot")?;

        // If firmware info is available, use it. Otherwise fall back to compile-time version.
        if let Some(firmware_info) = running_slot.firmware {
            Ok(firmware_info.version.to_string())
        } else {
            use crate::version::FIRMWARE_VERSION;
            Ok(FIRMWARE_VERSION.to_string())
        }
    }

    /// Trigger complete OTA update workflow (root node only)
    /// Downloads firmware from URL and prepares for distribution
    /// Firmware is streamed directly to OTA partition to avoid RAM exhaustion
    pub fn trigger_ota_update(&mut self, firmware_url: &str, version: String, firmware_size: u32) -> Result<()> {
        info!("state::ota: Triggering OTA update to version: {}", version);

        // Download firmware directly to OTA partition (streaming)
        self.download_firmware(firmware_url, firmware_size)?;

        // Finalize the OTA update for the root node
        // This validates the image and marks it as the boot partition
        info!("state::ota: Download complete, finalizing OTA update for root node...");
        self.finalize_ota()?;

        // Prepare distribution metadata (no chunk loading)
        self.prepare_distribution(version)?;

        // Update state to distributing
        *self.state.lock().unwrap() = OtaState::Distributing {
            total_chunks: self.firmware_total_chunks,
            nodes_complete: 0,
            total_nodes: 0, // Will be updated as nodes respond
        };

        info!(
            "state::ota: OTA update prepared: {} chunks ready for on-demand distribution",
            self.firmware_total_chunks
        );

        Ok(())
    }

    /// Trigger reboot
    pub fn reboot(&self) -> ! {
        info!("state::ota: Rebooting device...");
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
