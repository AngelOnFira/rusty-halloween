FROM ghcr.io/cross-rs/aarch64-unknown-linux-gnu:0.2.5

# Install pkg-config and ALSA dev headers for the *target* arch
RUN apt-get update \
 && apt-get install -y --no-install-recommends pkg-config \
 && dpkg --add-architecture arm64 \
 && apt-get update \
 && apt-get install -y --no-install-recommends libasound2-dev:arm64 \
 && rm -rf /var/lib/apt/lists/*

# Make pkg-config use the ARM64 sysroot
# (These paths are where Debian/Ubuntu put the .pc files for arm64)
ENV PKG_CONFIG_ALLOW_CROSS=1 \
    PKG_CONFIG_SYSROOT_DIR=/ \
    PKG_CONFIG_DIR= \
    PKG_CONFIG_LIBDIR=/usr/lib/aarch64-linux-gnu/pkgconfig:/usr/lib/pkgconfig:/usr/share/pkgconfig
