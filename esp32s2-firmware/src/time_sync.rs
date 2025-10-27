use defmt::{debug, info, warn};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, signal::Signal};
use embassy_time::{Duration, Instant, Ticker};

use crate::protocol::{MeshPacket, PacketType, TimeSyncBeacon};

/// Time synchronization state
pub struct TimeSync {
    /// Local time offset from root (microseconds)
    /// Positive means local clock is ahead of root
    offset_us: i64,
    /// Last time we received a sync beacon
    last_sync_instant: Option<Instant>,
    /// Sequence number of last sync beacon received
    last_sync_sequence: u32,
    /// Whether this node is the root
    is_root: bool,
}

impl TimeSync {
    pub fn new(is_root: bool) -> Self {
        Self {
            offset_us: 0,
            last_sync_instant: None,
            last_sync_sequence: 0,
            is_root,
        }
    }

    /// Get current synchronized time in microseconds
    pub fn now_us(&self) -> u64 {
        let local_us = Instant::now().as_micros();
        if self.is_root {
            local_us
        } else {
            // Apply offset to sync with root
            (local_us as i64 - self.offset_us) as u64
        }
    }

    /// Handle received time sync beacon (child nodes only)
    pub fn handle_sync_beacon(&mut self, beacon: &TimeSyncBeacon, rx_instant: Instant) {
        if self.is_root {
            // Root doesn't sync with anyone
            return;
        }

        // Check if this is a new beacon
        if beacon.sequence <= self.last_sync_sequence {
            debug!("Ignoring old sync beacon: {} <= {}", beacon.sequence, self.last_sync_sequence);
            return;
        }

        // Calculate offset
        // root_timestamp is when root sent the beacon
        // rx_instant is when we received it (local time)
        let local_rx_us = rx_instant.as_micros() as i64;
        let root_tx_us = beacon.root_timestamp as i64;

        // Estimate one-way delay (assume symmetric path)
        // For now, we'll use a simple estimate of 5ms
        const ESTIMATED_ONE_WAY_DELAY_US: i64 = 5000;

        // Calculate offset: local_time - (root_time + delay)
        let new_offset = local_rx_us - (root_tx_us + ESTIMATED_ONE_WAY_DELAY_US);

        // Apply smoothing to avoid sudden jumps
        if self.last_sync_instant.is_some() {
            // Weighted average: 70% old, 30% new
            self.offset_us = (self.offset_us * 7 + new_offset * 3) / 10;
        } else {
            // First sync, accept new offset immediately
            self.offset_us = new_offset;
        }

        self.last_sync_instant = Some(rx_instant);
        self.last_sync_sequence = beacon.sequence;

        info!(
            "Time sync: offset={}us, beacon_seq={}",
            self.offset_us,
            beacon.sequence
        );
    }

    /// Check if time sync is valid (received beacon recently)
    pub fn is_synced(&self) -> bool {
        if self.is_root {
            return true;
        }

        if let Some(last_sync) = self.last_sync_instant {
            // Consider synced if we got a beacon in the last 10 seconds
            let elapsed = Instant::now().duration_since(last_sync);
            elapsed < Duration::from_secs(10)
        } else {
            false
        }
    }

    /// Set this node as root
    pub fn set_root(&mut self, is_root: bool) {
        self.is_root = is_root;
        if is_root {
            self.offset_us = 0;
            info!("Node is now root - time offset reset to 0");
        }
    }
}

/// Time synchronization beacon task (root node only)
pub async fn time_sync_beacon_task(
    tx_queue: &'static Channel<CriticalSectionRawMutex, MeshPacket, 8>,
    is_root_signal: &'static Signal<CriticalSectionRawMutex, bool>,
) {
    info!("Time sync beacon task started");

    let mut ticker = Ticker::every(Duration::from_secs(1));
    let mut sequence = 0u32;
    let mut is_root = false;

    loop {
        // Check if we're root
        if is_root_signal.signaled() {
            is_root = is_root_signal.wait().await;
        }

        ticker.next().await;

        if !is_root {
            continue;
        }

        // Create time sync beacon
        let beacon = TimeSyncBeacon {
            root_timestamp: Instant::now().as_micros(),
            sequence,
        };

        // Serialize beacon to packet payload
        let mut packet = MeshPacket::new(
            PacketType::TimeSync,
            beacon.root_timestamp,
            [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF], // Will be set by sender
        );

        // Add beacon data to payload
        if let Ok(serialized) = postcard::to_vec::<_, 64>(&beacon) {
            for byte in serialized {
                let _ = packet.payload.push(byte);
            }

            // Send beacon
            tx_queue.send(packet).await;
            debug!("Sent time sync beacon seq={}", sequence);
        } else {
            warn!("Failed to serialize time sync beacon");
        }

        sequence = sequence.wrapping_add(1);
    }
}

/// Time synchronization state shared across tasks
pub static TIME_SYNC: embassy_sync::mutex::Mutex<CriticalSectionRawMutex, Option<TimeSync>> =
    embassy_sync::mutex::Mutex::new(None);

/// Initialize time sync
pub async fn init_time_sync(is_root: bool) {
    let mut time_sync = TIME_SYNC.lock().await;
    *time_sync = Some(TimeSync::new(is_root));
    info!("Time sync initialized (is_root={})", is_root);
}

/// Get current synchronized time
pub async fn now_us() -> u64 {
    let time_sync = TIME_SYNC.lock().await;
    if let Some(ts) = time_sync.as_ref() {
        ts.now_us()
    } else {
        // Fallback to local time if not initialized
        Instant::now().as_micros()
    }
}

/// Handle time sync beacon (should be called when TimeSync packet is received)
pub async fn handle_sync_beacon(beacon: &TimeSyncBeacon) {
    let mut time_sync = TIME_SYNC.lock().await;
    if let Some(ts) = time_sync.as_mut() {
        ts.handle_sync_beacon(beacon, Instant::now());
    }
}

/// Check if time is synchronized
pub async fn is_synced() -> bool {
    let time_sync = TIME_SYNC.lock().await;
    if let Some(ts) = time_sync.as_ref() {
        ts.is_synced()
    } else {
        false
    }
}

/// Set root status
pub async fn set_root(is_root: bool) {
    let mut time_sync = TIME_SYNC.lock().await;
    if let Some(ts) = time_sync.as_mut() {
        ts.set_root(is_root);
    }
}
