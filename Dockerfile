FROM ghcr.io/cross-rs/armv7-unknown-linux-gnueabihf:edge

# Add ALSA
RUN apt-get update && apt-get install -y libasound2-dev
