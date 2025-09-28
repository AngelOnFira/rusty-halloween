# Rusty Halloween 🎃

A Rust-based Halloween light show controller that orchestrates lights, lasers, projectors, and DMX devices in synchronization with audio.

## Features

- 🎵 Audio synchronization with light shows
- 💡 GPIO-controlled lights
- 🔦 Serial-controlled laser projectors
- 🎭 DMX device control (projectors, turrets)
- 📝 JSON-based show configuration
- 🎮 Real-time show control and monitoring

## Quick Start

### Prerequisites

- Rust toolchain
- Cross-compilation tools for Raspberry Pi (if deploying to Pi)
- `just` command runner
- Hardware setup according to the [2024 hardware spec](https://gist.github.com/AngelOnFira/5fded8e144a2c716e5685398c16081d1)

### Building

For Raspberry Pi Zero:
