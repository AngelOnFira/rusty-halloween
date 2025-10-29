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

# Release ESP32 firmware with version bump and tag
# Usage: just release-esp32 [VERSION]
# VERSION can be: patch, minor, major, or explicit version like 0.2.0
release-esp32 VERSION='patch':
    #!/usr/bin/env bash
    set -euo pipefail

    VERSION="{{ VERSION }}"
    CARGO_TOML="esp32-firmware/Cargo.toml"

    # Get current version from Cargo.toml
    CURRENT_VERSION=$(grep '^version = ' "$CARGO_TOML" | head -1 | sed 's/version = "\(.*\)"/\1/')
    echo "Current ESP32 firmware version: $CURRENT_VERSION"

    # Parse current version into components
    IFS='.' read -r MAJOR MINOR PATCH <<< "$CURRENT_VERSION"

    # Determine new version
    if [[ "$VERSION" == "patch" ]]; then
        NEW_VERSION="$MAJOR.$MINOR.$((PATCH + 1))"
    elif [[ "$VERSION" == "minor" ]]; then
        NEW_VERSION="$MAJOR.$((MINOR + 1)).0"
    elif [[ "$VERSION" == "major" ]]; then
        NEW_VERSION="$((MAJOR + 1)).0.0"
    else
        NEW_VERSION="$VERSION"
    fi

    echo "New version: $NEW_VERSION"
    echo ""

    # Update version in Cargo.toml
    sed -i.bak "s/^version = \".*\"/version = \"$NEW_VERSION\"/" "$CARGO_TOML"
    rm "$CARGO_TOML.bak"

    # Update Cargo.lock by building
    echo "Updating Cargo.lock..."
    cd esp32-firmware
    cargo check --quiet 2>/dev/null || true
    cd ..

    # Stage only the files we changed
    git add esp32-firmware/Cargo.toml esp32-firmware/Cargo.lock

    # Commit the version bump
    git commit -m "chore(esp32): bump version to $NEW_VERSION"

    # Create tag
    TAG="esp32-v$NEW_VERSION"
    git tag "$TAG"

    echo ""
    echo "✓ Version bumped to $NEW_VERSION"
    echo "✓ Committed and tagged as: $TAG"
    echo ""
    read -p "Push to GitHub and trigger release? (y/N) " -n 1 -r
    echo ""

    if [[ $REPLY =~ ^[Yy]$ ]]; then
        echo "Pushing to GitHub..."
        git push origin main
        git push origin "$TAG"

        echo ""
        echo "✓ ESP32 firmware release $TAG initiated!"
        echo "✓ GitHub Actions will build and create the release."
        echo ""
        echo "Monitor the release at: https://github.com/fmdunlap/rusty-halloween/actions"
    else
        echo "Aborted. To push manually, run:"
        echo "  git push origin main && git push origin $TAG"
    fi
