# Show Server Deployment Guide

This guide covers deploying the show-server using Docker and Nomad.

## Overview

The show-server provides:
- Show data distribution to ESP32 wireless lights
- Real-time show synchronization
- Firmware distribution proxy (avoids GitHub redirect issues)

## Architecture

```
GitHub Actions → Build Binary → Docker Image → GHCR → Nomad → Cloudflare Tunnel
```

## Prerequisites

1. **Cloudflare Tunnel Token**
   - Go to [Cloudflare Zero Trust Dashboard](https://dash.cloudflare.com/)
   - Navigate to: Zero Trust → Networks → Tunnels
   - Create a new tunnel
   - Copy the tunnel token

2. **Nomad Cluster**
   - Running Nomad cluster with Docker driver
   - Datacenter configured (default: `dc1`)

## Deployment Steps

### 1. Configure Cloudflare Tunnel

Update `nomad/show-server.nomad` with your tunnel token:

```hcl
task "cloudflared" {
  config {
    args = [
      "tunnel",
      "--no-autoupdate",
      "run",
      "--token",
      "YOUR_TOKEN_HERE"  # Replace this
    ]
  }
}
```

Configure your tunnel to route traffic:
- Public hostname: `show-server.rustwood.org` (or your domain)
- Service: `http://localhost:5830`

### 2. Update Environment Variables

Edit `nomad/show-server.nomad` environment variables:

```hcl
env {
  # Logging level
  RUST_LOG = "show_server=info,tower_http=debug"

  # GitHub repo for firmware distribution
  GITHUB_REPO_OWNER = "your-username"
  GITHUB_REPO_NAME = "rusty-halloween"

  # Public URL (must match Cloudflare tunnel)
  SERVER_URL = "https://show-server.your-domain.org"
}
```

### 3. Deploy to Nomad

```bash
# Deploy the job
nomad job run nomad/show-server.nomad

# Check status
nomad job status rusty-halloween-show-server

# View logs
nomad alloc logs -stderr -f <allocation-id> show-server
nomad alloc logs -stderr -f <allocation-id> cloudflared
```

### 4. Verify Deployment

Test the endpoints:

```bash
# Check show status (will 404 if no show uploaded yet)
curl https://show-server.your-domain.org/show/status

# Check firmware endpoint
curl https://show-server.your-domain.org/firmware/latest
```

## CI/CD Pipeline

The GitHub Actions workflow (`.github/workflows/deploy-show-server.yml`) automatically:

1. Builds the binary on Ubuntu (x86_64-unknown-linux-gnu)
2. Creates a Docker image
3. Pushes to GitHub Container Registry
4. Tags with:
   - `latest` (main branch only)
   - Git SHA
   - Branch name

### Triggering Deployment

**Automatic:**
- Push to `main` branch with changes to `show-server/` or `common/`

**Manual:**
- Go to Actions → "Build and Deploy Show Server" → Run workflow

### Deploying Updates

```bash
# Nomad will automatically pull the latest image
nomad job run nomad/show-server.nomad

# Force restart without configuration changes
nomad job restart rusty-halloween-show-server
```

## Configuration

### Port Mapping

- **External (Cloudflare):** HTTPS (443)
- **Nomad static port:** 5830
- **Container port:** 3000

### Resource Limits

```hcl
resources {
  cpu    = 512   # 0.5 CPU cores
  memory = 512   # 512 MB RAM
}
```

Adjust based on load. ESP32 polling is lightweight but firmware downloads may spike.

## API Endpoints

Once deployed:

```bash
# Show management
POST   /show/upload          # Main controller uploads show
POST   /show/start           # Mark show as started
GET    /show/status          # Current show status

# Device instructions
GET    /device/:id/instructions?from=<timestamp>

# Firmware distribution
GET    /firmware/latest      # Latest release info
GET    /firmware/download/:version?asset=<filename>
```

## Main Controller Configuration

Update your Raspberry Pi main controller to use the server:

```bash
# On the Pi, set environment variable
export SHOW_SERVER_URL=https://show-server.your-domain.org

# Run the controller
./rusty-halloween
```

## ESP32 Configuration

Update ESP32 firmware to poll the server:

```rust
// In your ESP32 code
const SERVER_URL: &str = "https://show-server.your-domain.org";

// Poll for instructions
let url = format!("{}/device/light-1/instructions?from=0", SERVER_URL);

// Get firmware updates
let url = format!("{}/firmware/latest", SERVER_URL);
```

## Monitoring

### Logs

```bash
# Show server logs
nomad alloc logs -f <alloc-id> show-server

# Cloudflare tunnel logs
nomad alloc logs -f <alloc-id> cloudflared
```

### Expected Log Output

```
INFO show_server: Show server listening on http://0.0.0.0:3000
📤 Received show upload: test-show (234 frames)
✅ Show uploaded and ready for playback
🎵 Show 'test-show' started playing NOW!
🎬 Show Progress: test-show | 5s elapsed | Frame at 4800ms
📡 Serving 3 instructions for device light-1 (10000ms - 15000ms)
🔍 Fetching latest firmware from GitHub: owner/repo
```

## Troubleshooting

### Image not pulling

```bash
# Check if image exists in GHCR
docker pull ghcr.io/angelonfira/rusty-halloween-show-server:latest

# Force Nomad to re-pull
nomad job run -check-index 0 nomad/show-server.nomad
```

### Cloudflare tunnel issues

```bash
# Check tunnel logs
nomad alloc logs <alloc-id> cloudflared

# Verify tunnel is active in Cloudflare dashboard
# Check public hostname routing
```

### No show data

The server starts empty. It only receives show data when the main controller (Raspberry Pi) uploads it via `POST /show/upload`.

## Security Notes

- Cloudflare tunnel provides TLS/HTTPS automatically
- No authentication on endpoints (relies on network isolation)
- ESP32s and main controller should be on trusted network
- Consider adding API keys if exposing publicly

## Updating

1. Make changes to `show-server/` code
2. Push to `main` branch
3. GitHub Actions builds and pushes new image
4. Redeploy Nomad job: `nomad job run nomad/show-server.nomad`
5. Nomad pulls `latest` tag automatically

## Rollback

```bash
# Deploy specific version
# Edit nomad/show-server.nomad to use SHA tag
image = "ghcr.io/angelonfira/rusty-halloween-show-server:main-abc1234"

nomad job run nomad/show-server.nomad
```
