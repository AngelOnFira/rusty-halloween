use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::Response,
    routing::{get, post},
    Json, Router,
};
use common::{SerializableShow};
use serde::{Deserialize, Serialize};
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tracing::{info, warn, debug, error};

// Simple ESP32-specific instruction format
#[derive(Debug, Serialize)]
struct Esp32Instruction {
    timestamp: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    r: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    g: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    b: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    off: Option<bool>,
}

#[derive(Debug, Serialize)]
struct Esp32Response {
    device_id: String,
    /// Server's authoritative show start time (Unix timestamp in ms)
    /// ESP32 should use this for timing calculations
    show_start_time: u64,
    instructions: Vec<Esp32Instruction>,
}

#[derive(Clone)]
struct AppState {
    current_show: Arc<RwLock<Option<ShowState>>>,
}

struct ShowState {
    show: SerializableShow,
    upload_time: u64,  // milliseconds since epoch when show was uploaded
    start_time: Option<u64>, // milliseconds since epoch when playback started
    is_playing: bool,
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("show_server=debug,tower_http=debug")
        .init();

    let state = AppState {
        current_show: Arc::new(RwLock::new(None)),
    };

    // Spawn background task to log show progress
    let state_clone = state.clone();
    tokio::spawn(async move {
        log_show_progress(state_clone).await;
    });

    let app = Router::new()
        .route("/show/upload", post(upload_show))
        .route("/show/start", post(start_show))
        .route("/show/status", get(show_status))
        .route("/device/:device_id/instructions", get(device_instructions))
        .route("/firmware/latest", get(get_latest_firmware))
        .route("/firmware/download/:version", get(download_firmware))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .unwrap();

    info!("Show server listening on http://0.0.0.0:3000");

    axum::serve(listener, app).await.unwrap();
}

/// Upload a new show to the server (called during PrepareShow)
async fn upload_show(
    State(state): State<AppState>,
    Json(show): Json<SerializableShow>,
) -> StatusCode {
    info!("📤 Received show upload: {} ({} frames)", show.name, show.frames.len());

    let upload_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    let show_state = ShowState {
        show,
        upload_time,
        start_time: None,
        is_playing: false,
    };

    *state.current_show.write().await = Some(show_state);

    info!("✅ Show uploaded and ready for playback");
    StatusCode::OK
}

#[derive(Deserialize)]
struct StartQuery {
    /// Optional delay in milliseconds before show actually starts
    /// Use this to account for network latency + audio startup time
    #[serde(default)]
    delay_ms: u64,
}

/// Mark the show as started (called when audio playback begins)
async fn start_show(
    State(state): State<AppState>,
    Query(query): Query<StartQuery>,
) -> StatusCode {
    let mut show_lock = state.current_show.write().await;

    if let Some(show_state) = show_lock.as_mut() {
        let current_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        // Apply delay to account for network latency + audio startup
        let start_time = current_time + query.delay_ms;

        show_state.start_time = Some(start_time);
        show_state.is_playing = true;

        if query.delay_ms > 0 {
            info!(
                "🎵 Show '{}' will start in {}ms (at Unix timestamp {})",
                show_state.show.name, query.delay_ms, start_time
            );
        } else {
            info!("🎵 Show '{}' started playing NOW!", show_state.show.name);
        }

        StatusCode::OK
    } else {
        warn!("⚠️  Received start signal but no show is uploaded");
        StatusCode::NOT_FOUND
    }
}

#[derive(Deserialize)]
struct InstructionsQuery {
    from: u64, // timestamp in milliseconds from show start
}

/// Get device-specific instructions for the next 5 seconds
async fn device_instructions(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
    Query(query): Query<InstructionsQuery>,
) -> Result<Json<Esp32Response>, StatusCode> {
    let show_lock = state.current_show.read().await;

    // If no show is loaded, return empty instructions (not an error)
    let show_state = match show_lock.as_ref() {
        Some(state) => state,
        None => {
            debug!("📡 No show loaded, returning empty instructions for device {}", device_id);
            return Ok(Json(Esp32Response {
                device_id,
                show_start_time: 0, // No show = no start time
                instructions: Vec::new(),
            }));
        }
    };

    let from_timestamp = query.from;
    let to_timestamp = from_timestamp + 5000; // 5 seconds buffer

    // Filter frames for the time window and device
    let instructions = extract_device_instructions(
        &show_state.show,
        &device_id,
        from_timestamp,
        to_timestamp,
    );

    // Get show start time (0 if not started yet)
    let show_start_time = show_state.start_time.unwrap_or(0);

    debug!(
        "📡 Serving {} instructions for device {} ({}ms - {}ms), show_start_time: {}",
        instructions.len(),
        device_id,
        from_timestamp,
        to_timestamp,
        show_start_time
    );

    Ok(Json(Esp32Response {
        device_id,
        show_start_time,
        instructions,
    }))
}

/// Get current show status
async fn show_status(State(state): State<AppState>) -> Result<Json<ShowStatus>, StatusCode> {
    let show_lock = state.current_show.read().await;
    let show_state = show_lock.as_ref().ok_or(StatusCode::NOT_FOUND)?;

    let current_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    let elapsed = show_state.start_time.map(|start| current_time - start);

    Ok(Json(ShowStatus {
        show_name: show_state.show.name.clone(),
        upload_time: show_state.upload_time,
        start_time: show_state.start_time,
        elapsed_ms: elapsed,
        is_playing: show_state.is_playing,
        frame_count: show_state.show.frames.len(),
    }))
}

#[derive(serde::Serialize)]
struct ShowStatus {
    show_name: String,
    upload_time: u64,
    start_time: Option<u64>,
    elapsed_ms: Option<u64>,
    is_playing: bool,
    frame_count: usize,
}

/// Background task that logs show progress every 5 seconds
async fn log_show_progress(state: AppState) {
    use tokio::time::{interval, Duration};

    let mut tick = interval(Duration::from_secs(5));

    loop {
        tick.tick().await;

        let show_lock = state.current_show.read().await;
        if let Some(show_state) = show_lock.as_ref() {
            if show_state.is_playing {
                if let Some(start_time) = show_state.start_time {
                    let current_time = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as u64;

                    let elapsed_ms = current_time - start_time;
                    let elapsed_sec = elapsed_ms / 1000;

                    // Find the last frame that should have executed
                    let current_frame = show_state
                        .show
                        .frames
                        .iter()
                        .rev()
                        .find(|f| f.timestamp <= elapsed_ms);

                    if let Some(frame) = current_frame {
                        info!(
                            "🎬 Show Progress: {} | {}s elapsed | Frame at {}ms | {} frames total",
                            show_state.show.name,
                            elapsed_sec,
                            frame.timestamp,
                            show_state.show.frames.len()
                        );
                    } else {
                        info!(
                            "🎬 Show Progress: {} | {}s elapsed | Before first frame",
                            show_state.show.name, elapsed_sec
                        );
                    }
                }
            }
        }
    }
}

/// Extract device-specific instructions from the show for a given time window
/// Returns simple RGB instructions for ESP32 devices
fn extract_device_instructions(
    show: &SerializableShow,
    device_id: &str,
    from: u64,
    to: u64,
) -> Vec<Esp32Instruction> {
    let mut instructions = Vec::new();

    // Handle ESP32 RGB devices (format: "esp32-light-1", "esp32-rgb-2", etc.)
    // These devices get RGB commands aggregated from all lasers in the show
    if device_id.starts_with("esp32-") {
        debug!("Extracting instructions for ESP32 RGB device: {}", device_id);

        // For ESP32 RGB devices, aggregate all laser colors and average them
        for frame in &show.frames {
            if frame.timestamp < from || frame.timestamp >= to {
                continue;
            }

            // Collect all enabled laser colors
            let mut r_sum = 0u32;
            let mut g_sum = 0u32;
            let mut b_sum = 0u32;
            let mut count = 0u32;

            for laser_opt in &frame.lasers {
                if let Some(laser) = laser_opt {
                    if laser.enable {
                        r_sum += laser.hex[0] as u32;
                        g_sum += laser.hex[1] as u32;
                        b_sum += laser.hex[2] as u32;
                        count += 1;
                    }
                }
            }

            // Create simple ESP32 instruction
            let instruction = if count > 0 {
                // Average the colors if any lasers are enabled
                Esp32Instruction {
                    timestamp: frame.timestamp,
                    r: Some((r_sum / count) as u8),
                    g: Some((g_sum / count) as u8),
                    b: Some((b_sum / count) as u8),
                    off: None,
                }
            } else {
                // No lasers enabled, turn off
                Esp32Instruction {
                    timestamp: frame.timestamp,
                    r: None,
                    g: None,
                    b: None,
                    off: Some(true),
                }
            };

            instructions.push(instruction);
        }

        return instructions;
    }

    // For non-ESP32 devices, return empty for now
    // (This can be extended later if needed for other device types)
    warn!("Non-ESP32 device requested: {}, returning empty instructions", device_id);
    instructions
}

// ===== Firmware Distribution Endpoints =====

#[derive(Debug, Serialize, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    name: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

#[derive(Debug, Serialize)]
struct FirmwareInfo {
    version: String,
    name: String,
    assets: Vec<FirmwareAsset>,
}

#[derive(Debug, Serialize)]
struct FirmwareAsset {
    name: String,
    size: u64,
    download_url: String,
}

/// Get latest firmware release information from GitHub
async fn get_latest_firmware() -> Result<Json<FirmwareInfo>, StatusCode> {
    let owner = std::env::var("GITHUB_REPO_OWNER")
        .unwrap_or_else(|_| "your-username".to_string());
    let repo = std::env::var("GITHUB_REPO_NAME")
        .unwrap_or_else(|_| "rusty-halloween".to_string());

    let url = format!(
        "https://api.github.com/repos/{}/{}/releases/latest",
        owner, repo
    );

    info!("🔍 Fetching latest firmware from GitHub: {}/{}", owner, repo);

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("User-Agent", "show-server")
        .send()
        .await
        .map_err(|e| {
            error!("Failed to fetch from GitHub: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if !response.status().is_success() {
        error!("GitHub API returned status: {}", response.status());
        return Err(StatusCode::BAD_GATEWAY);
    }

    let release: GitHubRelease = response.json().await.map_err(|e| {
        error!("Failed to parse GitHub response: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    info!("✅ Found firmware version: {}", release.tag_name);

    let server_url = std::env::var("SERVER_URL")
        .unwrap_or_else(|_| "http://localhost:3000".to_string());

    let firmware_info = FirmwareInfo {
        version: release.tag_name.clone(),
        name: release.name,
        assets: release
            .assets
            .iter()
            .map(|asset| FirmwareAsset {
                name: asset.name.clone(),
                size: asset.size,
                download_url: format!(
                    "{}/firmware/download/{}?asset={}",
                    server_url,
                    release.tag_name,
                    asset.name
                ),
            })
            .collect(),
    };

    Ok(Json(firmware_info))
}

#[derive(Deserialize)]
struct DownloadQuery {
    asset: String,
}

/// Download firmware binary by version (proxies from GitHub with Range request support)
async fn download_firmware(
    Path(version): Path<String>,
    Query(query): Query<DownloadQuery>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let owner = std::env::var("GITHUB_REPO_OWNER")
        .unwrap_or_else(|_| "your-username".to_string());
    let repo = std::env::var("GITHUB_REPO_NAME")
        .unwrap_or_else(|_| "rusty-halloween".to_string());

    info!(
        "📥 Downloading firmware: {} / {} / {}",
        owner, repo, version
    );

    // Get release info for this version
    let url = format!(
        "https://api.github.com/repos/{}/{}/releases/tags/{}",
        owner, repo, version
    );

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("User-Agent", "show-server")
        .send()
        .await
        .map_err(|e| {
            error!("Failed to fetch release info: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if !response.status().is_success() {
        error!("GitHub API returned status: {}", response.status());
        return Err(StatusCode::NOT_FOUND);
    }

    let release: GitHubRelease = response.json().await.map_err(|e| {
        error!("Failed to parse GitHub response: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Find the requested asset
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == query.asset)
        .ok_or_else(|| {
            error!("Asset '{}' not found in release", query.asset);
            StatusCode::NOT_FOUND
        })?;

    info!("📦 Proxying download for: {} ({} bytes)", asset.name, asset.size);

    // Build request to GitHub, forwarding Range header if present
    let mut github_request = client
        .get(&asset.browser_download_url)
        .header("User-Agent", "show-server");

    // Forward Range header to GitHub for chunked downloads
    if let Some(range) = headers.get(header::RANGE) {
        info!("📍 Range request: {:?}", range);
        // Convert axum header value to string for reqwest
        if let Ok(range_str) = range.to_str() {
            github_request = github_request.header("Range", range_str);
        }
    }

    // Stream the binary from GitHub
    let download_response = github_request
        .send()
        .await
        .map_err(|e| {
            error!("Failed to download from GitHub: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let github_status = download_response.status();

    // Accept both 200 (full content) and 206 (partial content)
    if !github_status.is_success() && github_status.as_u16() != 206 {
        error!("Download failed with status: {}", github_status);
        return Err(StatusCode::BAD_GATEWAY);
    }

    // Extract headers before consuming the response
    let content_length = download_response.headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let content_range = download_response.headers()
        .get("content-range")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    if let Some(ref cr) = content_range {
        info!("📍 Content-Range: {}", cr);
    }

    // Stream the response body
    let stream = download_response.bytes_stream();
    let body = Body::from_stream(stream);

    // Build response, forwarding status code and headers from GitHub
    let mut response_builder = Response::builder()
        .status(github_status.as_u16())  // Use GitHub's status (200 or 206)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", asset.name),
        )
        .header(header::ACCEPT_RANGES, "bytes");  // Advertise Range support

    // Forward Content-Length from GitHub (will be chunk size if Range request)
    if let Some(cl) = content_length {
        response_builder = response_builder.header(header::CONTENT_LENGTH, cl);
    }

    // Forward Content-Range from GitHub if present (for 206 responses)
    if let Some(cr) = content_range {
        response_builder = response_builder.header(header::CONTENT_RANGE, cr);
    }

    let response = response_builder.body(body).unwrap();

    info!("✅ Streaming firmware to device (status: {})", github_status);

    Ok(response)
}
