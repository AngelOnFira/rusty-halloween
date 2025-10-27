use defmt::{error, info, warn};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use esp_radio::esp_now::{EspNow, EspNowWifiInterface, PeerInfo, BROADCAST_ADDRESS};

use crate::protocol::MeshPacket;

/// ESP-NOW manager errors
#[derive(Debug, defmt::Format)]
pub enum EspNowError {
    /// Serialization error
    SerializationFailed,
    /// Deserialization error
    DeserializationFailed,
    /// Send failed
    SendFailed,
    /// Receive failed
    ReceiveFailed,
    /// Peer add failed
    PeerAddFailed,
}

/// Maximum number of packets in RX queue
const RX_QUEUE_SIZE: usize = 8;

/// Maximum number of packets in TX queue
const TX_QUEUE_SIZE: usize = 8;

/// Received packet with sender MAC address
#[derive(Debug, Clone)]
pub struct ReceivedPacket {
    pub packet: MeshPacket,
    pub rssi: i8,
    pub sender_mac: [u8; 6],
}

/// Add broadcast peer for sending to all nodes
pub fn add_broadcast_peer(esp_now: &mut EspNow) -> Result<(), EspNowError> {
    let peer_info = PeerInfo {
        peer_address: BROADCAST_ADDRESS,
        lmk: None,
        channel: None,
        encrypt: false,
        interface: EspNowWifiInterface::Sta,
    };

    match esp_now.add_peer(peer_info) {
        Ok(_) => {
            info!("Added broadcast peer");
            Ok(())
        }
        Err(_) => {
            error!("Failed to add broadcast peer");
            Err(EspNowError::PeerAddFailed)
        }
    }
}

/// Send a packet via ESP-NOW
pub async fn send_packet(esp_now: &mut EspNow<'_>, packet: &MeshPacket) -> Result<(), EspNowError> {
    // Serialize packet
    let serialized = match packet.serialize() {
        Ok(data) => data,
        Err(_) => {
            error!("Failed to serialize packet");
            return Err(EspNowError::SerializationFailed);
        }
    };

    // Send to broadcast address
    match esp_now.send_async(&BROADCAST_ADDRESS, &serialized).await {
        Ok(_) => {
            info!("Sent packet type {:?}, {} bytes", packet.packet_type, serialized.len());
            Ok(())
        }
        Err(_) => {
            error!("Failed to send packet");
            Err(EspNowError::SendFailed)
        }
    }
}

/// Receive a packet via ESP-NOW
pub async fn receive_packet(esp_now: &mut EspNow<'_>) -> Result<ReceivedPacket, EspNowError> {
    // Receive from ESP-NOW
    let received_data = esp_now.receive_async().await;
    let data = received_data.data();
    let sender_mac = received_data.info.src_address;

    // Deserialize packet
    match MeshPacket::deserialize(data) {
        Ok(packet) => {
            info!("Received packet type {:?} from {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                packet.packet_type,
                sender_mac[0], sender_mac[1], sender_mac[2],
                sender_mac[3], sender_mac[4], sender_mac[5]
            );

            Ok(ReceivedPacket {
                packet,
                rssi: 0, // ESP-NOW doesn't provide RSSI directly
                sender_mac,
            })
        }
        Err(_) => {
            warn!("Failed to deserialize packet");
            Err(EspNowError::DeserializationFailed)
        }
    }
}

/// ESP-NOW receive task
pub async fn espnow_rx_task(
    esp_now: &'static mut EspNow<'static>,
    rx_queue: &'static Channel<CriticalSectionRawMutex, ReceivedPacket, RX_QUEUE_SIZE>,
) {
    info!("ESP-NOW RX task started");

    loop {
        match receive_packet(esp_now).await {
            Ok(received) => {
                // Send to RX queue for processing
                rx_queue.send(received).await;
            }
            Err(e) => {
                error!("ESP-NOW receive error: {:?}", e);
                embassy_time::Timer::after_millis(100).await;
            }
        }
    }
}

/// ESP-NOW transmit task
pub async fn espnow_tx_task(
    esp_now: &'static mut EspNow<'static>,
    tx_queue: &'static Channel<CriticalSectionRawMutex, MeshPacket, TX_QUEUE_SIZE>,
) {
    info!("ESP-NOW TX task started");

    loop {
        // Wait for packet from TX queue
        let packet = tx_queue.receive().await;

        // Send packet
        if let Err(e) = send_packet(esp_now, &packet).await {
            error!("Failed to send packet: {:?}", e);
        }

        // Small delay between transmissions
        embassy_time::Timer::after_millis(10).await;
    }
}
