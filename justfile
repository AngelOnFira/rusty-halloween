build-zero:
    cross build \
        --target armv7-unknown-linux-gnueabihf \
        --bin rusty-halloween \
        --features pi \
        --release
        
    mkdir -p exports
    cp target/armv7-unknown-linux-gnueabihf/release/rusty-halloween exports/rusty-halloween-pi-zero

    # Try to upload it to the pi
    scp exports/rusty-halloween-pi-zero 192.168.0.141:/home/pi/Halloween/rust-test/rusty-halloween-pi-zero

build-zero-debug:
    cross build \
        --target armv7-unknown-linux-gnueabihf \
        --bin rusty-halloween \
        --features pi
        
    mkdir -p exports
    cp target/armv7-unknown-linux-gnueabihf/debug/rusty-halloween exports/rusty-halloween-pi-zero-debug

    # Try to upload it to the pi
    scp exports/rusty-halloween-pi-zero-debug 192.168.0.141:/home/pi/Halloween/rust-test/rusty-halloween-pi-zero-debug