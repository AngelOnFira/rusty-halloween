use rusty_halloween::show::prelude::UnloadedShow;
use std::path::Path;
use serde_json::Value;

fn main() {
    // Load the hardware configuration
    let hardware_config = std::fs::read_to_string("src/show/assets/2024/hardware.json")
        .expect("Failed to read hardware config");
    let hardware: Value = serde_json::from_str(&hardware_config)
        .expect("Failed to parse hardware config");

    // Load the song instructions
    let song_instructions = std::fs::read_to_string("src/show/assets/2024/song.json")
        .expect("Failed to read song instructions");
    let song: Value = serde_json::from_str(&song_instructions)
        .expect("Failed to parse song instructions");

    println!("Loaded configurations successfully!");
    println!("Validating hardware mappings...");

    // Validate that all devices in song instructions exist in hardware config
    for (timestamp, frame) in song.as_object().unwrap() {
        if timestamp == "song" { continue; } // Skip the song field
        
        for (device, _) in frame.as_object().unwrap() {
            if !hardware.as_object().unwrap().contains_key(device) {
                println!("Warning: Device '{}' in song instructions not found in hardware config", device);
            }
        }
    }

    println!("Validation complete!");

    // Print some stats about the show
    let frame_count = song.as_object().unwrap().len() - 1; // Subtract 1 for "song" field
    println!("\nShow statistics:");
    println!("Number of frames: {}", frame_count);
    println!("Show duration: {} seconds", 
        song.as_object().unwrap()
            .keys()
            .filter(|k| *k != "song")
            .map(|k| k.parse::<u64>().unwrap_or(0))
            .max()
            .unwrap_or(0) / 1000
    );
}
