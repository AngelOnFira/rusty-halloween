use serde::{Deserialize, Deserializer, Serialize, Serializer};
use smart_leds::RGB8;

/// Serializable wrapper for RGB8
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SerializableRGB8(pub RGB8);

impl Serialize for SerializableRGB8 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (self.0.r, self.0.g, self.0.b).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SerializableRGB8 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (r, g, b) = <(u8, u8, u8)>::deserialize(deserializer)?;
        Ok(SerializableRGB8(RGB8 { r, g, b }))
    }
}

/// Maximum payload size for ESP-NOW packets
/// ESP-NOW supports up to 250 bytes, we reserve 14 bytes for header
pub const MAX_PAYLOAD_SIZE: usize = 236;

/// Packet types for mesh communication
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, defmt::Format)]
pub enum PacketType {
    /// Time synchronization beacon from root
    TimeSync,
    /// LED instruction buffer (list of timed color changes)
    Instructions,
    /// Routing protocol messages (neighbor discovery, route updates)
    Routing,
    /// OTA firmware update chunk
    OtaChunk,
    /// OTA acknowledgment
    OtaAck,
    /// OTA start notification
    OtaStart,
    /// OTA reboot command
    OtaReboot,
}

/// Main packet structure for ESP-NOW communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshPacket {
    /// Hop count (incremented at each relay, used for loop prevention)
    pub hop_count: u8,
    /// Packet type
    pub packet_type: PacketType,
    /// Timestamp from sender (microseconds)
    pub timestamp: u64,
    /// Source node MAC address
    pub source_mac: [u8; 6],
    /// Payload data
    pub payload: heapless::Vec<u8, MAX_PAYLOAD_SIZE>,
}

impl MeshPacket {
    /// Create a new packet
    pub fn new(packet_type: PacketType, timestamp: u64, source_mac: [u8; 6]) -> Self {
        Self {
            hop_count: 0,
            packet_type,
            timestamp,
            source_mac,
            payload: heapless::Vec::new(),
        }
    }

    /// Serialize packet to bytes using postcard
    pub fn serialize(&self) -> Result<heapless::Vec<u8, 256>, postcard::Error> {
        let crc_engine = crc::Crc::<u32>::new(&crc::CRC_32_ISCSI);
        let digest = crc_engine.digest();
        postcard::to_vec_crc32(self, digest)
    }

    /// Deserialize packet from bytes using postcard
    pub fn deserialize(data: &[u8]) -> Result<Self, postcard::Error> {
        let crc_engine = crc::Crc::<u32>::new(&crc::CRC_32_ISCSI);
        let digest = crc_engine.digest();
        postcard::from_bytes_crc32(data, digest)
    }

    /// Increment hop count for packet forwarding
    pub fn increment_hop(&mut self) {
        self.hop_count = self.hop_count.saturating_add(1);
    }
}

/// Time synchronization beacon payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeSyncBeacon {
    /// Root node's current timestamp (microseconds)
    pub root_timestamp: u64,
    /// Beacon sequence number
    pub sequence: u32,
}

/// LED instruction with timestamp and color
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Instruction {
    /// Timestamp when this instruction should execute (microseconds)
    pub timestamp: u64,
    /// RGB color to display
    pub color: SerializableRGB8,
}

/// Instruction buffer payload (multiple instructions)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstructionBuffer {
    /// List of instructions (sorted by timestamp)
    pub instructions: heapless::Vec<Instruction, 32>,
}

/// Routing protocol message types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RoutingMessage {
    /// Neighbor discovery beacon (broadcast periodically)
    NeighborBeacon {
        /// Distance from root (0 = root, 255 = unknown)
        distance_from_root: u8,
    },
    /// Route update notification
    RouteUpdate {
        /// New distance from root
        distance_from_root: u8,
    },
}

/// OTA firmware chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtaChunkData {
    /// Chunk sequence number (0-based)
    pub sequence: u32,
    /// Total number of chunks
    pub total_chunks: u32,
    /// Firmware version string
    pub version: heapless::String<16>,
    /// Chunk data (up to 200 bytes)
    pub data: heapless::Vec<u8, 200>,
    /// CRC32 of this chunk data
    pub crc32: u32,
}

/// OTA acknowledgment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtaAckData {
    /// Chunk sequence being acknowledged
    pub sequence: u32,
    /// Success flag
    pub success: bool,
    /// Node's MAC address
    pub node_mac: [u8; 6],
}

/// OTA start notification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtaStartData {
    /// Firmware version
    pub version: heapless::String<16>,
    /// Total number of chunks
    pub total_chunks: u32,
    /// Total firmware size in bytes
    pub firmware_size: u32,
}
