use defmt::{debug, info, warn};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use embassy_time::{Duration, Instant, Ticker};
use heapless::FnvIndexMap;

use crate::protocol::{MeshPacket, PacketType, RoutingMessage};

/// Maximum number of neighbors to track
const MAX_NEIGHBORS: usize = 10;

/// Maximum hop count before dropping packet
const MAX_HOP_COUNT: u8 = 10;

/// Neighbor expiry time (if no beacon received)
const NEIGHBOR_EXPIRY_SECS: u64 = 30;

/// Neighbor information
#[derive(Debug, Clone)]
pub struct Neighbor {
    /// MAC address
    pub mac: [u8; 6],
    /// Distance from root (hops)
    pub distance_from_root: u8,
    /// Last time we heard from this neighbor
    pub last_seen: Instant,
    /// RSSI (signal strength)
    pub rssi: i8,
}

/// Routing table
pub struct RoutingTable {
    /// List of known neighbors
    neighbors: FnvIndexMap<[u8; 6], Neighbor, MAX_NEIGHBORS>,
    /// Our distance from root (0 = root, 255 = unknown)
    our_distance: u8,
    /// MAC address of our preferred parent (closest to root)
    parent_mac: Option<[u8; 6]>,
    /// Our own MAC address
    our_mac: [u8; 6],
}

impl RoutingTable {
    pub fn new(our_mac: [u8; 6], is_root: bool) -> Self {
        Self {
            neighbors: FnvIndexMap::new(),
            our_distance: if is_root { 0 } else { 255 },
            parent_mac: None,
            our_mac,
        }
    }

    /// Update neighbor information
    pub fn update_neighbor(&mut self, mac: [u8; 6], distance_from_root: u8, rssi: i8) -> bool {
        let now = Instant::now();
        let mut route_changed = false;

        // Update or insert neighbor
        if let Some(neighbor) = self.neighbors.get_mut(&mac) {
            // Update existing neighbor
            if neighbor.distance_from_root != distance_from_root {
                neighbor.distance_from_root = distance_from_root;
                route_changed = true;
            }
            neighbor.last_seen = now;
            neighbor.rssi = rssi;
        } else {
            // New neighbor discovered
            if self.neighbors.len() < MAX_NEIGHBORS {
                let _ = self.neighbors.insert(
                    mac,
                    Neighbor {
                        mac,
                        distance_from_root,
                        last_seen: now,
                        rssi,
                    },
                );
                route_changed = true;
                info!(
                    "New neighbor: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}, distance={}",
                    mac[0], mac[1], mac[2], mac[3], mac[4], mac[5], distance_from_root
                );
            } else {
                warn!("Neighbor table full, cannot add new neighbor");
            }
        }

        // Update our distance and parent
        if route_changed {
            self.update_our_distance();
        }

        route_changed
    }

    /// Update our distance from root based on neighbors
    fn update_our_distance(&mut self) {
        if self.our_distance == 0 {
            // We're root, don't change
            return;
        }

        // Find neighbor with minimum distance
        let mut best_neighbor: Option<([u8; 6], u8)> = None;

        for (mac, neighbor) in &self.neighbors {
            if neighbor.distance_from_root < 255 {
                match best_neighbor {
                    None => best_neighbor = Some((*mac, neighbor.distance_from_root)),
                    Some((_, best_dist)) => {
                        if neighbor.distance_from_root < best_dist {
                            best_neighbor = Some((*mac, neighbor.distance_from_root));
                        }
                    }
                }
            }
        }

        if let Some((parent_mac, parent_dist)) = best_neighbor {
            let new_distance = parent_dist.saturating_add(1);
            if new_distance != self.our_distance {
                info!(
                    "Route updated: distance {} -> {}, parent: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                    self.our_distance,
                    new_distance,
                    parent_mac[0], parent_mac[1], parent_mac[2],
                    parent_mac[3], parent_mac[4], parent_mac[5]
                );
                self.our_distance = new_distance;
                self.parent_mac = Some(parent_mac);
            }
        } else {
            // No valid path to root
            if self.our_distance != 255 {
                warn!("Lost connection to root");
                self.our_distance = 255;
                self.parent_mac = None;
            }
        }
    }

    /// Remove expired neighbors
    pub fn remove_expired_neighbors(&mut self) {
        let now = Instant::now();
        let mut expired_macs = heapless::Vec::<[u8; 6], MAX_NEIGHBORS>::new();

        for (mac, neighbor) in &self.neighbors {
            if now.duration_since(neighbor.last_seen).as_secs() > NEIGHBOR_EXPIRY_SECS {
                let _ = expired_macs.push(*mac);
            }
        }

        for mac in &expired_macs {
            self.neighbors.remove(mac);
            info!(
                "Removed expired neighbor: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
            );
        }

        if !expired_macs.is_empty() {
            self.update_our_distance();
        }
    }

    /// Get our current distance from root
    pub fn our_distance(&self) -> u8 {
        self.our_distance
    }

    /// Check if packet should be forwarded
    /// Returns true if we should forward this packet
    pub fn should_forward(&self, packet: &MeshPacket) -> bool {
        // Don't forward if hop count exceeded
        if packet.hop_count >= MAX_HOP_COUNT {
            debug!("Dropping packet: hop count exceeded");
            return false;
        }

        // Don't forward our own packets
        if packet.source_mac == self.our_mac {
            return false;
        }

        // Forward broadcast packets
        // In a real implementation, you'd track seen packet IDs to avoid loops
        true
    }

    /// Set as root node
    pub fn set_root(&mut self, is_root: bool) {
        if is_root {
            self.our_distance = 0;
            self.parent_mac = None;
            info!("Node is now root");
        } else if self.our_distance == 0 {
            self.our_distance = 255;
            info!("Node is no longer root");
        }
    }
}

/// Routing state shared across tasks
pub static ROUTING_TABLE: embassy_sync::mutex::Mutex<
    CriticalSectionRawMutex,
    Option<RoutingTable>,
> = embassy_sync::mutex::Mutex::new(None);

/// Initialize routing
pub async fn init_routing(our_mac: [u8; 6], is_root: bool) {
    let mut routing = ROUTING_TABLE.lock().await;
    *routing = Some(RoutingTable::new(our_mac, is_root));
    info!("Routing initialized (is_root={})", is_root);
}

/// Handle routing message
pub async fn handle_routing_message(sender_mac: [u8; 6], message: &RoutingMessage, rssi: i8) {
    let mut routing = ROUTING_TABLE.lock().await;
    if let Some(table) = routing.as_mut() {
        match message {
            RoutingMessage::NeighborBeacon { distance_from_root } => {
                table.update_neighbor(sender_mac, *distance_from_root, rssi);
            }
            RoutingMessage::RouteUpdate { distance_from_root } => {
                table.update_neighbor(sender_mac, *distance_from_root, rssi);
            }
        }
    }
}

/// Get our distance from root
pub async fn our_distance() -> u8 {
    let routing = ROUTING_TABLE.lock().await;
    if let Some(table) = routing.as_ref() {
        table.our_distance()
    } else {
        255
    }
}

/// Check if packet should be forwarded
pub async fn should_forward(packet: &MeshPacket) -> bool {
    let routing = ROUTING_TABLE.lock().await;
    if let Some(table) = routing.as_ref() {
        table.should_forward(packet)
    } else {
        false
    }
}

/// Neighbor beacon task - broadcasts our distance periodically
pub async fn neighbor_beacon_task(
    tx_queue: &'static Channel<CriticalSectionRawMutex, MeshPacket, 8>,
    our_mac: [u8; 6],
) {
    info!("Neighbor beacon task started");

    let mut ticker = Ticker::every(Duration::from_secs(5));

    loop {
        ticker.next().await;

        // Get our distance
        let distance = our_distance().await;

        // Create neighbor beacon
        let message = RoutingMessage::NeighborBeacon {
            distance_from_root: distance,
        };

        // Create packet
        let mut packet = MeshPacket::new(
            PacketType::Routing,
            embassy_time::Instant::now().as_micros(),
            our_mac,
        );

        // Serialize routing message to payload
        if let Ok(serialized) = postcard::to_vec::<_, 64>(&message) {
            for byte in serialized {
                let _ = packet.payload.push(byte);
            }

            // Send beacon
            tx_queue.send(packet).await;
            debug!("Sent neighbor beacon (distance={})", distance);
        } else {
            warn!("Failed to serialize neighbor beacon");
        }
    }
}

/// Neighbor cleanup task - removes expired neighbors
pub async fn neighbor_cleanup_task() {
    info!("Neighbor cleanup task started");

    let mut ticker = Ticker::every(Duration::from_secs(10));

    loop {
        ticker.next().await;

        let mut routing = ROUTING_TABLE.lock().await;
        if let Some(table) = routing.as_mut() {
            table.remove_expired_neighbors();
        }
    }
}

/// Packet forwarding task - forwards packets to extend mesh range
pub async fn packet_forward_task(
    rx_queue: &'static Channel<CriticalSectionRawMutex, crate::espnow::ReceivedPacket, 8>,
    tx_queue: &'static Channel<CriticalSectionRawMutex, MeshPacket, 8>,
) {
    info!("Packet forwarding task started");

    loop {
        let received = rx_queue.receive().await;

        // Check if we should forward this packet
        if should_forward(&received.packet).await {
            // Increment hop count
            let mut forwarded = received.packet.clone();
            forwarded.increment_hop();

            // Forward packet
            tx_queue.send(forwarded).await;
            debug!("Forwarded packet from {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                received.sender_mac[0], received.sender_mac[1], received.sender_mac[2],
                received.sender_mac[3], received.sender_mac[4], received.sender_mac[5]
            );
        }
    }
}
