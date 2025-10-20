deploy-zero:
    cross build \
        --target aarch64-unknown-linux-gnu \
        --bin rusty-halloween \
        --features pi,audio \
        --release
        
    mkdir -p exports
    cp target/aarch64-unknown-linux-gnu/release/rusty-halloween exports/rusty-halloween-pi-zero

    # Try to upload it to the pi
    scp exports/rusty-halloween-pi-zero pi@192.168.0.141:/home/pi/2025-rust/rusty-halloween-pi-zero

deploy-zero-debug:
    cross build \
        --target aarch64-unknown-linux-gnu \
        --bin rusty-halloween \
        --features pi,audio
        
    mkdir -p exports
    cp target/aarch64-unknown-linux-gnu/debug/rusty-halloween exports/rusty-halloween-pi-zero-debug

    # Try to upload it to the pi
    scp exports/rusty-halloween-pi-zero-debug pi@192.168.0.141:/home/pi/2025-rust/rusty-halloween-pi-zero-debug
    
run-remote-test:
    cross build \
        --target aarch64-unknown-linux-gnu \
        --bin test \
        --features pi,audio \
        --release

    mkdir -p exports
    cp target/aarch64-unknown-linux-gnu/release/test exports/rust-test

    # Try to upload it to the pi
    scp exports/rust-test pi@192.168.0.141:/home/pi/2025-rust/rust-test
    
    # Run the test
    ssh pi@192.168.0.141 "cd /home/pi/2025-rust && ./rust-test"
    
build-zero:
    cross build \
        --target aarch64-unknown-linux-gnu \
        --bin rusty-halloween \
        --features pi,audio \
        --release

    mkdir -p exports
    cp target/aarch64-unknown-linux-gnu/release/rusty-halloween exports/rusty-halloween-pi-zero

build-zero-debug:
    cross build \
        --target aarch64-unknown-linux-gnu \
        --bin rusty-halloween \
        --features pi,audio
        
    mkdir -p exports
    cp target/aarch64-unknown-linux-gnu/debug/rusty-halloween exports/rusty-halloween-pi-zero-debug
    