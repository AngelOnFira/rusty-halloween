extern crate alloc;

use embassy_net::Stack;
use embassy_net::tcp::client::{TcpClient, TcpClientState};
use embassy_net::dns::DnsSocket;
use embassy_time::{Duration, Timer};
use crate::esp32_types::Esp32Response;
use heapless::String as HeaplessString;
use static_cell::StaticCell;

use reqwless::client::HttpClient;
use reqwless::request::Method;

const SERVER_HOST: &str = "rusty-halloween-show-server.rustwood.org";
const SERVER_PORT: u16 = 443; // HTTPS
const MAX_RESPONSE_SIZE: usize = 4096;

/// HTTP request retry configuration
const HTTP_RETRY_COUNT: u32 = 3;
const HTTP_RETRY_DELAY_MS: u64 = 1000;

// Static state for TCP client (required by embassy-net)
// Supports 2 concurrent connections with 1024-byte buffers
// Must be initialized once before any HTTP requests are made
static TCP_CLIENT_STATE: StaticCell<TcpClientState<2, 1024, 1024>> = StaticCell::new();

pub struct ShowClient<'a> {
    device_id: &'a str,
    server_host: &'a str,
    server_port: u16,
    tcp_state: &'a TcpClientState<2, 1024, 1024>,
}

impl<'a> ShowClient<'a> {
    /// Initialize the global TCP client state and return a ShowClient.
    /// This initializes the TCP state once and creates a client that uses it.
    pub fn new_with_init(device_id: &'a str) -> Self {
        let tcp_state = TCP_CLIENT_STATE.init_with(|| TcpClientState::new());
        Self {
            device_id,
            server_host: SERVER_HOST,
            server_port: SERVER_PORT,
            tcp_state,
        }
    }

    pub fn with_server(device_id: &'a str, host: &'a str, port: u16, tcp_state: &'a TcpClientState<2, 1024, 1024>) -> Self {
        Self {
            device_id,
            server_host: host,
            server_port: port,
            tcp_state,
        }
    }

    /// Fetch test pattern instructions (10 seconds of blinking white LEDs)
    /// This is useful for testing LED connectivity without needing a full show
    pub async fn fetch_test_instructions(
        &self,
        stack: &Stack<'_>,
    ) -> Result<Esp32Response, FetchError> {
        // Build URL path for test endpoint
        use core::fmt::Write;
        let mut url = HeaplessString::<256>::new();
        write!(
            &mut url,
            "https://{}/device/{}/test",
            self.server_host, self.device_id
        )
        .map_err(|_| FetchError::UrlTooLong)?;

        defmt::info!("Fetching test pattern from: {}", url.as_str());

        // Create TCP client and DNS socket from stack
        let tcp_client = TcpClient::new(stack.clone(), self.tcp_state);
        let dns = DnsSocket::new(stack.clone());

        // Create HTTP client
        let mut client = HttpClient::new(&tcp_client, &dns);

        // Create and send request
        let mut request = client
            .request(Method::GET, url.as_str())
            .await
            .map_err(|_| FetchError::RequestFailed)?;

        let mut rx_buf = [0u8; MAX_RESPONSE_SIZE];
        let response = request
            .send(&mut rx_buf)
            .await
            .map_err(|_| FetchError::RequestFailed)?;

        // Check status code
        if response.status.0 != 200 {
            defmt::error!("Server returned status: {}", response.status.0);
            return Err(FetchError::InvalidStatus(response.status.0));
        }

        // Read and parse response
        let body = response
            .body()
            .read_to_end()
            .await
            .map_err(|_| FetchError::ReadFailed)?;

        defmt::info!("Received {} bytes of test pattern", body.len());

        let response: Esp32Response = serde_json_core::from_slice(body)
            .map_err(|_| FetchError::ParseFailed)?
            .0;

        defmt::info!(
            "Parsed {} test instructions, show_start_time: {}",
            response.instructions.len(),
            response.show_start_time
        );

        Ok(response)
    }

    /// Fetch LED instructions from the server with retry logic
    ///
    /// # Arguments
    /// * `stack` - Embassy network stack reference
    /// * `from_ms` - Timestamp in milliseconds from show start
    ///
    /// # Returns
    /// An Esp32Response containing show_start_time and instructions if successful
    pub async fn fetch_instructions(
        &self,
        stack: &Stack<'_>,
        from_ms: u64,
    ) -> Result<Esp32Response, FetchError> {
        for attempt in 1..=HTTP_RETRY_COUNT {
            match self.fetch_instructions_inner(stack, from_ms).await {
                Ok(response) => {
                    if attempt > 1 {
                        defmt::info!("HTTP request succeeded on attempt {}", attempt);
                    }
                    return Ok(response);
                }
                Err(e) => {
                    if attempt < HTTP_RETRY_COUNT {
                        defmt::warn!(
                            "HTTP request failed (attempt {}/{}): {:?}, retrying in {}ms",
                            attempt, HTTP_RETRY_COUNT, e, HTTP_RETRY_DELAY_MS
                        );
                        Timer::after(Duration::from_millis(HTTP_RETRY_DELAY_MS)).await;
                    } else {
                        defmt::error!(
                            "HTTP request failed after {} attempts: {:?}",
                            HTTP_RETRY_COUNT, e
                        );
                        return Err(e);
                    }
                }
            }
        }

        // This should never be reached due to the loop logic above
        Err(FetchError::RequestFailed)
    }

    /// Internal fetch implementation without retries
    async fn fetch_instructions_inner(
        &self,
        stack: &Stack<'_>,
        from_ms: u64,
    ) -> Result<Esp32Response, FetchError> {
        // Build URL path
        use core::fmt::Write;
        let mut url = HeaplessString::<256>::new();
        write!(
            &mut url,
            "https://{}/device/{}/instructions?from={}",
            self.server_host, self.device_id, from_ms
        )
        .map_err(|_| FetchError::UrlTooLong)?;

        defmt::info!("Fetching from: {}", url.as_str());

        // Create TCP client and DNS socket from stack using the pre-initialized state
        let tcp_client = TcpClient::new(stack.clone(), self.tcp_state);
        let dns = DnsSocket::new(stack.clone());

        // Create HTTP client (no TLS for now - plain HTTP)
        let mut client = HttpClient::new(&tcp_client, &dns);

        // Create request
        let mut request = client
            .request(Method::GET, url.as_str())
            .await
            .map_err(|_| FetchError::RequestFailed)?;

        // Send request
        let mut rx_buf = [0u8; MAX_RESPONSE_SIZE];
        let response = request
            .send(&mut rx_buf)
            .await
            .map_err(|_| FetchError::RequestFailed)?;

        // Check status code
        if response.status.0 != 200 {
            defmt::error!("Server returned status: {}", response.status.0);
            return Err(FetchError::InvalidStatus(response.status.0));
        }

        // Read response body
        let body = response
            .body()
            .read_to_end()
            .await
            .map_err(|_| FetchError::ReadFailed)?;

        defmt::info!("Received {} bytes", body.len());

        // Parse JSON using simple ESP32 format
        let response: Esp32Response = serde_json_core::from_slice(body)
            .map_err(|_| FetchError::ParseFailed)?
            .0;

        defmt::info!(
            "Parsed {} instructions for device {}, show_start_time: {}",
            response.instructions.len(),
            response.device_id.as_str(),
            response.show_start_time
        );

        Ok(response)
    }

    /// Poll the server repeatedly at the given interval
    ///
    /// This is useful for continuously fetching new instructions as the show progresses
    pub async fn poll_loop<F>(
        &self,
        stack: &Stack<'_>,
        interval_ms: u64,
        mut callback: F,
    ) -> !
    where
        F: FnMut(Esp32Response),
    {
        let mut last_fetch_ms = 0u64;

        loop {
            match self.fetch_instructions(stack, last_fetch_ms).await {
                Ok(response) => {
                    if !response.instructions.is_empty() {
                        // Update last fetch timestamp to the latest instruction
                        if let Some(last_instr) = response.instructions.last() {
                            last_fetch_ms = last_instr.timestamp;
                        }
                        callback(response);
                    }
                }
                Err(e) => {
                    defmt::error!("Failed to fetch instructions: {:?}", e);
                }
            }

            Timer::after(Duration::from_millis(interval_ms)).await;
        }
    }
}

#[derive(Debug, defmt::Format)]
pub enum FetchError {
    UrlTooLong,
    RequestFailed,
    InvalidStatus(u16),
    ReadFailed,
    ParseFailed,
    NotImplemented,
}
