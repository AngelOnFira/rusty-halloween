build-zero:
    cross build \
        --target armv7-unknown-linux-gnueabihf \
        --bin rusty-halloween \
        --features pi \
        --release
        
    mkdir -p exports
    cp target/armv7-unknown-linux-gnueabihf/release/rusty-halloween exports/rusty-halloween-pi-zero
