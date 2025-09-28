FROM ghcr.io/cross-rs/armv7-unknown-linux-gnueabihf:0.2.5

# Install pkg-config and ALSA dev headers for the *target* arch
RUN apt-get update \
 && apt-get install -y --no-install-recommends pkg-config \
 && dpkg --add-architecture armhf \
 && apt-get update \
 && apt-get install -y --no-install-recommends libasound2-dev:armhf \
 && rm -rf /var/lib/apt/lists/*

# Make pkg-config use the ARMHF sysroot
# (These paths are where Debian/Ubuntu put the .pc files for armhf)
ENV PKG_CONFIG_ALLOW_CROSS=1 \
    PKG_CONFIG_SYSROOT_DIR=/ \
    PKG_CONFIG_DIR= \
    PKG_CONFIG_LIBDIR=/usr/lib/arm-linux-gnueabihf/pkgconfig:/usr/lib/pkgconfig:/usr/share/pkgconfig
