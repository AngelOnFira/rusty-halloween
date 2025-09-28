deploy-zero:
    cross build \
        --target armv7-unknown-linux-gnueabihf \
        --bin rusty-halloween \
        --features pi,audio \
        --release
        
    mkdir -p exports
    cp target/armv7-unknown-linux-gnueabihf/release/rusty-halloween exports/rusty-halloween-pi-zero

    # Try to upload it to the pi
    # scp exports/rusty-halloween-pi-zero aidan-pi:/home/pi/Halloween/rust-test/rusty-halloween-pi-zero

deploy-zero-debug:
    cross build \
        --target armv7-unknown-linux-gnueabihf \
        --bin rusty-halloween \
        --features pi,audio
        
    mkdir -p exports
    cp target/armv7-unknown-linux-gnueabihf/debug/rusty-halloween exports/rusty-halloween-pi-zero-debug

    # Try to upload it to the pi
    scp exports/rusty-halloween-pi-zero-debug 192.168.0.141:/home/pi/Halloween/rust-test/rusty-halloween-pi-zero-debug
    
build-zero:
    cross build \
        --target armv7-unknown-linux-gnueabihf \
        --bin rusty-halloween \
        --features pi,audio \
        --release

    mkdir -p exports
    cp target/armv7-unknown-linux-gnueabihf/release/rusty-halloween exports/rusty-halloween-pi-zero

build-zero-debug:
    cross build \
        --target armv7-unknown-linux-gnueabihf \
        --bin rusty-halloween \
        --features pi,audio
        
    mkdir -p exports
    cp target/armv7-unknown-linux-gnueabihf/debug/rusty-halloween exports/rusty-halloween-pi-zero-debug
    