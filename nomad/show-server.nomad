job "rusty-halloween-show-server" {
  datacenters = ["dc1"]
  type = "service"

  group "backend" {
    count = 1

    # Network configuration with static port
    network {
      port "server" {
        static = 5838
        to = 3000
      }
    }

    # Show Server service
    task "show-server" {
      driver = "docker"

      config {
        image = "ghcr.io/angelonfira/rusty-halloween-show-server:latest"
        ports = ["server"]

        # Force pull latest image on updates
        force_pull = true
      }

      # Environment variables for production
      env {
        RUST_LOG = "show_server=info,tower_http=debug"

        # GitHub configuration for firmware distribution
        GITHUB_REPO_OWNER = "angelonfira"
        GITHUB_REPO_NAME = "rusty-halloween"

        # Server URL for firmware download links
        SERVER_URL = "https://rusty-halloween-show-server.rustwood.org"
      }

      resources {
        cpu    = 512
        memory = 512
      }

      # Health check (optional - adjust if you add a health endpoint)
      # service {
      #   name = "show-server"
      #   port = "server"
      #
      #   check {
      #     type     = "http"
      #     path     = "/show/status"
      #     interval = "10s"
      #     timeout  = "2s"
      #   }
      # }

      # Restart policy
      restart {
        attempts = 4
        interval = "2m"
        delay    = "25s"
        mode     = "delay"
      }
    }

    # # Cloudflare tunnel for secure internet access
    # task "cloudflared" {
    #   driver = "docker"

    #   config {
    #     image = "cloudflare/cloudflared:latest"
    #     args = [
    #       "tunnel",
    #       "--no-autoupdate",
    #       "run",
    #       "--token",
    #       # TODO: Replace with your Cloudflare tunnel token
    #       # Get one from: https://dash.cloudflare.com/ -> Zero Trust -> Networks -> Tunnels
    #       "YOUR_CLOUDFLARE_TUNNEL_TOKEN_HERE"
    #     ]
    #   }

    #   resources {
    #     cpu    = 128
    #     memory = 256
    #   }

    #   # Restart policy for tunnel connectivity
    #   restart {
    #     attempts = 10
    #     interval = "5m"
    #     delay    = "15s"
    #     mode     = "delay"
    #   }
    # }

    # Restart policy for the entire group
    restart {
      attempts = 2
      interval = "30m"
      delay    = "15s"
      mode     = "fail"
    }

    # Update strategy
    update {
      max_parallel      = 1
      min_healthy_time  = "10s"
      healthy_deadline  = "3m"
      progress_deadline = "10m"
      auto_revert       = true
      canary            = 0
    }
  }

  # Job metadata
  meta {
    version = "1.0.0"
    description = "Rusty Halloween Show Server - Controls light shows and firmware distribution"
  }
}
