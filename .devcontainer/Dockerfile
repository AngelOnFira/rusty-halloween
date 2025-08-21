# See here for image contents:
# https://github.com/microsoft/vscode-dev-containers/tree/v0.245.2/containers/rust/.devcontainer/base.Dockerfile

# [Choice] Debian OS version (use bullseye on local arm64/Apple Silicon): buster, bullseye
ARG VARIANT="bookworm"
FROM mcr.microsoft.com/vscode/devcontainers/rust:${VARIANT}

# Install deps and KiCad 9 from bookworm-backports
RUN set -eux; \
    export DEBIAN_FRONTEND=noninteractive; \
    apt-get update; \
    apt-get -y install --no-install-recommends \
        libasound2-dev \
        build-essential; \
    echo "deb http://deb.debian.org/debian bookworm-backports main contrib non-free non-free-firmware" > /etc/apt/sources.list.d/bookworm-backports.list; \
    apt-get update; \
    apt-get -y -t bookworm-backports install --no-install-recommends \
        kicad; \
        # kicad-footprints \
        # kicad-symbols \
        # kicad-templates \
        # kicad-packages3d; \
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
    && uv tool install atopile==0.11.1;

# Install cargo-binstall + a few handy tools
RUN curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash \
    && cargo binstall cross -y \
    && cargo binstall tokei -y \
    && cargo binstall just -y
