# Simple Dockerfile that uses a pre-built binary
# Binary is built in GitHub Actions on the Linux host, then copied here
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create a non-root user
RUN useradd -m -u 1000 showserver

# Create directory for the application
WORKDIR /app

# Copy the pre-built binary from GitHub Actions
COPY show-server /app/show-server

# Make it executable
RUN chmod +x /app/show-server

# Switch to non-root user
USER showserver

# Expose port 3000
EXPOSE 3000

# Set default environment variables (can be overridden)
ENV RUST_LOG=info

# Run the server
CMD ["/app/show-server"]
