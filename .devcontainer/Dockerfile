# Combined Dockerfile for Rust development with ARM (Raspberry Pi) and ESP32 support
# Based on Microsoft VS Code dev container for Rust

ARG VARIANT="bookworm"
FROM mcr.microsoft.com/vscode/devcontainers/rust:${VARIANT}

ENV DEBIAN_FRONTEND=noninteractive
ENV LC_ALL=C.UTF-8
ENV LANG=C.UTF-8

# Arguments for ESP32 support
ARG ESP_BOARD=all
ARG GITHUB_TOKEN

# Install common dependencies, KiCad, and ESP32 development tools
RUN set -eux; \
    apt-get update; \
    apt-get -y install --no-install-recommends \
        # Audio development
        libasound2-dev \
        # Build essentials
        build-essential \
        pkg-config \
        curl \
        gcc \
        clang \
        # ESP32 specific dependencies
        libudev-dev \
        unzip \
        xz-utils \
        git \
        wget \
        flex \
        bison \
        gperf \
        python3 \
        python3-pip \
        python3-venv \
        cmake \
        ninja-build \
        ccache \
        libffi-dev \
        libssl-dev \
        dfu-util \
        libusb-1.0-0 \
        # ARM cross-compilation tools
        gcc-aarch64-linux-gnu \
        g++-aarch64-linux-gnu \
        crossbuild-essential-arm64 \
        # Additional tools
        nodejs \
        npm; \
    # Install Claude Code CLI
    npm install -g @anthropic-ai/claude-code; \
    # Add KiCad 9 from bookworm-backports
    echo "deb http://deb.debian.org/debian bookworm-backports main contrib non-free non-free-firmware" > /etc/apt/sources.list.d/bookworm-backports.list; \
    apt-get update; \
    apt-get -y -t bookworm-backports install --no-install-recommends \
        kicad; \
    apt-get clean; \
    rm -rf /var/lib/apt/lists/*

USER vscode

# Pre-create KiCad user directories for the vscode user
RUN set -eux; \
    kicad-cli --version; \
    mkdir -p \
      "$HOME/.config/kicad/9.0/plugins" \
      "$HOME/.local/share/kicad/9.0/plugins" \
      "$HOME/Documents/KiCad/9.0/plugins" \
    && curl -LsSf https://astral.sh/uv/install.sh | sh \
    && . $HOME/.local/bin/env \
    && uv tool install atopile

# Install cargo-binstall + handy tools including cross for ARM compilation
RUN curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash \
    && cargo binstall cross -y \
    && cargo binstall tokei -y \
    && cargo binstall just -y

# Install ESP32 specific tools
RUN ARCH=$($HOME/.cargo/bin/rustup show | grep "Default host" | sed -e 's/.* //') && \
    # Install espup
    curl -L "https://github.com/esp-rs/espup/releases/latest/download/espup-${ARCH}" -o "${HOME}/.cargo/bin/espup" && \
    chmod u+x "${HOME}/.cargo/bin/espup" && \
    # Install cargo-espflash
    curl -L "https://github.com/esp-rs/espflash/releases/latest/download/cargo-espflash-${ARCH}.zip" -o "${HOME}/.cargo/bin/cargo-espflash.zip" && \
    unzip "${HOME}/.cargo/bin/cargo-espflash.zip" -d "${HOME}/.cargo/bin/" && \
    rm "${HOME}/.cargo/bin/cargo-espflash.zip" && \
    chmod u+x "${HOME}/.cargo/bin/cargo-espflash" && \
    # Install espflash
    curl -L "https://github.com/esp-rs/espflash/releases/latest/download/espflash-${ARCH}.zip" -o "${HOME}/.cargo/bin/espflash.zip" && \
    unzip "${HOME}/.cargo/bin/espflash.zip" -d "${HOME}/.cargo/bin/" && \
    rm "${HOME}/.cargo/bin/espflash.zip" && \
    chmod u+x "${HOME}/.cargo/bin/espflash" && \
    # Install ldproxy
    curl -L "https://github.com/esp-rs/embuild/releases/latest/download/ldproxy-${ARCH}.zip" -o "${HOME}/.cargo/bin/ldproxy.zip" && \
    unzip "${HOME}/.cargo/bin/ldproxy.zip" -d "${HOME}/.cargo/bin/" && \
    rm "${HOME}/.cargo/bin/ldproxy.zip" && \
    chmod u+x "${HOME}/.cargo/bin/ldproxy" && \
    # Install web-flash
    curl -L "https://github.com/esp-rs/esp-web-flash-server/releases/latest/download/web-flash-${ARCH}.zip" -o "${HOME}/.cargo/bin/web-flash.zip" && \
    unzip "${HOME}/.cargo/bin/web-flash.zip" -d "${HOME}/.cargo/bin/" && \
    rm "${HOME}/.cargo/bin/web-flash.zip" && \
    chmod u+x "${HOME}/.cargo/bin/web-flash"

# Install Xtensa Rust toolchain for ESP32
RUN if [ -n "${GITHUB_TOKEN}" ]; then export GITHUB_TOKEN=${GITHUB_TOKEN}; fi \
    && ${HOME}/.cargo/bin/espup install \
    --targets "${ESP_BOARD}" \
    --log-level debug \
    --export-file /home/vscode/export-esp.sh

# Add ARM target for Raspberry Pi cross-compilation
RUN rustup target add aarch64-unknown-linux-gnu

# Setup environment for ESP32 development
RUN echo "source /home/vscode/export-esp.sh" >> ~/.bashrc

# Create cargo config for ARM cross-compilation
RUN mkdir -p $HOME/.cargo && \
    echo '[target.aarch64-unknown-linux-gnu]' >> $HOME/.cargo/config.toml && \
    echo 'linker = "aarch64-linux-gnu-gcc"' >> $HOME/.cargo/config.toml

CMD [ "/bin/bash" ]