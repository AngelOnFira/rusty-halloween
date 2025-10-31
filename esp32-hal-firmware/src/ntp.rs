extern crate alloc;

use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_net::{IpEndpoint, Stack};
use embassy_time::{Duration, Instant, Timer};
use smoltcp::wire::{IpAddress, Ipv4Address};

const NTP_PORT: u16 = 123;
const NTP_PACKET_SIZE: usize = 48;
const NTP_EPOCH_OFFSET: u64 = 2_208_988_800; // Seconds between 1900 and 1970

/// NTP time synchronization client
pub struct NtpClient {
    server: &'static str,
}

impl NtpClient {
    /// Create a new NTP client with the default pool.ntp.org server
    pub fn new() -> Self {
        Self {
            server: "pool.ntp.org",
        }
    }

    /// Create a new NTP client with a custom NTP server
    pub fn with_server(server: &'static str) -> Self {
        Self { server }
    }

    /// Synchronize time with the NTP server
    ///
    /// Returns the current Unix timestamp in milliseconds
    pub async fn sync_time(&self, stack: &Stack<'_>) -> Result<u64, NtpError> {
        defmt::info!("Synchronizing time with NTP server: {}", self.server);

        // Create UDP socket
        let mut rx_meta = [PacketMetadata::EMPTY; 2];
        let mut rx_buffer = [0; NTP_PACKET_SIZE];
        let mut tx_meta = [PacketMetadata::EMPTY; 2];
        let mut tx_buffer = [0; NTP_PACKET_SIZE];

        let mut socket = UdpSocket::new(
            *stack,
            &mut rx_meta,
            &mut rx_buffer,
            &mut tx_meta,
            &mut tx_buffer,
        );

        // Bind to any local port
        socket.bind(0).map_err(|_| NtpError::BindFailed)?;

        // TODO: Implement DNS resolution using embassy-net
        // For now, use a hardcoded NTP server IP (time.google.com: 216.239.35.0)
        let server_addr = IpAddress::Ipv4(Ipv4Address::new(216, 239, 35, 0));

        defmt::debug!("Using NTP server: {} (hardcoded, DNS not yet implemented)", server_addr);

        let endpoint = IpEndpoint::new(server_addr, NTP_PORT);

        // Create NTP request packet
        let mut ntp_packet = [0u8; NTP_PACKET_SIZE];

        // Set LI (0), Version (4), Mode (3 - client)
        ntp_packet[0] = 0b00_100_011;

        // Stratum, Poll, Precision all 0
        // Root delay and Root dispersion all 0
        // Reference ID all 0

        // Record transmit timestamp (for round-trip calculation)
        let send_instant = Instant::now();

        // Send NTP request
        socket
            .send_to(&ntp_packet, endpoint)
            .await
            .map_err(|_| NtpError::SendFailed)?;

        defmt::debug!("NTP request sent to {:?}", endpoint);

        // Receive NTP response with timeout
        let mut response = [0u8; NTP_PACKET_SIZE];

        let recv_result = embassy_time::with_timeout(
            Duration::from_secs(5),
            socket.recv_from(&mut response),
        )
        .await;

        let (len, _from) = recv_result
            .map_err(|_| NtpError::Timeout)?
            .map_err(|_| NtpError::ReceiveFailed)?;

        if len != NTP_PACKET_SIZE {
            return Err(NtpError::InvalidResponse);
        }

        let recv_instant = Instant::now();
        let rtt = recv_instant - send_instant;

        defmt::debug!("NTP response received, RTT: {:?}", rtt);

        // Parse transmit timestamp from response (bytes 40-47)
        let tx_timestamp = Self::parse_ntp_timestamp(&response[40..48]);

        // Convert NTP timestamp to Unix timestamp
        let unix_timestamp = if tx_timestamp >= NTP_EPOCH_OFFSET {
            tx_timestamp - NTP_EPOCH_OFFSET
        } else {
            return Err(NtpError::InvalidTimestamp);
        };

        // Account for half of the round-trip time
        let rtt_ms = rtt.as_millis();
        let adjusted_timestamp_ms = unix_timestamp * 1000 + (rtt_ms / 2);

        defmt::info!(
            "Time synchronized: {} (Unix ms), RTT: {}ms",
            adjusted_timestamp_ms,
            rtt_ms
        );

        Ok(adjusted_timestamp_ms)
    }

    /// Parse an NTP timestamp (8 bytes) into seconds since 1900
    fn parse_ntp_timestamp(bytes: &[u8]) -> u64 {
        let seconds = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as u64;
        let _fraction = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as u64;

        // Convert fraction to milliseconds (fraction is in 1/2^32 units)
        // We don't need sub-second precision for show sync, so just return seconds
        seconds
    }
}

impl Default for NtpClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Time synchronization manager that periodically syncs with NTP
pub struct TimeSync {
    ntp_client: NtpClient,
    last_sync_ms: u64,
    sync_offset_ms: i64, // Offset between local monotonic time and NTP time
    last_sync_instant: Option<Instant>,
}

impl TimeSync {
    pub fn new() -> Self {
        Self {
            ntp_client: NtpClient::new(),
            last_sync_ms: 0,
            sync_offset_ms: 0,
            last_sync_instant: None,
        }
    }

    /// Perform initial time synchronization
    pub async fn initial_sync(&mut self, stack: &Stack<'_>) -> Result<(), NtpError> {
        defmt::info!("Performing initial NTP sync...");

        let ntp_time_ms = self.ntp_client.sync_time(stack).await?;
        let now_instant = Instant::now();

        self.last_sync_ms = ntp_time_ms;
        self.last_sync_instant = Some(now_instant);
        self.sync_offset_ms = ntp_time_ms as i64;

        defmt::info!("Initial sync complete, offset: {}ms", self.sync_offset_ms);
        Ok(())
    }

    /// Get current time in milliseconds (Unix timestamp)
    pub fn current_time_ms(&self) -> u64 {
        if let Some(last_instant) = self.last_sync_instant {
            let elapsed = Instant::now() - last_instant;
            let elapsed_ms = elapsed.as_millis();
            (self.last_sync_ms + elapsed_ms) as u64
        } else {
            0 // Not synchronized yet
        }
    }

    /// Get current time relative to show start (milliseconds from show start)
    pub fn show_time_ms(&self, show_start_ms: u64) -> u64 {
        let current = self.current_time_ms();
        if current >= show_start_ms {
            current - show_start_ms
        } else {
            0
        }
    }

    /// Background task that periodically re-syncs time
    pub async fn periodic_sync_task(&mut self, stack: &Stack<'_>, interval_secs: u64) -> ! {
        loop {
            Timer::after(Duration::from_secs(interval_secs)).await;

            defmt::info!("Performing periodic NTP sync...");
            match self.ntp_client.sync_time(stack).await {
                Ok(ntp_time_ms) => {
                    let now_instant = Instant::now();
                    self.last_sync_ms = ntp_time_ms;
                    self.last_sync_instant = Some(now_instant);
                    defmt::info!("Periodic sync successful: {}ms", ntp_time_ms);
                }
                Err(e) => {
                    defmt::error!("Periodic NTP sync failed: {:?}", e);
                }
            }
        }
    }
}

impl Default for TimeSync {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, defmt::Format)]
pub enum NtpError {
    BindFailed,
    DnsResolveFailed,
    SendFailed,
    ReceiveFailed,
    Timeout,
    InvalidResponse,
    InvalidTimestamp,
}
