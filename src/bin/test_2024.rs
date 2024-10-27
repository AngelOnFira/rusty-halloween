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

    // Try to load the show using UnloadedShow
    println!("\nAttempting to load show...");
    let show = UnloadedShow::load_show_file(Path::new("src/show/assets/2024/song.json"));

    // Debug show
    dbg!(&show);

    // Print show details
    println!("\nShow details:");
    println!("Name: {}", show.name);
    println!("Number of frames: {}", show.frames.len());
    
    // Print details about the first few frames
    println!("\nFirst frame details:");
    if let Some(first_frame) = show.frames.first() {
        println!("Timestamp: {}ms", first_frame.timestamp);
        println!("Active lights: {}", first_frame.lights.iter()
            .enumerate()
            .filter(|(_, &light)| light == Some(true))
            .map(|(i, _)| format!("light-{}", i + 1))
            .collect::<Vec<_>>()
            .join(", "));
        
        println!("Active lasers: {}", first_frame.lasers.iter()
            .enumerate()
            .filter(|(_, laser)| laser.is_some())
            .map(|(i, _)| format!("laser-{}", i + 1))
            .collect::<Vec<_>>()
            .join(", "));
        
        println!("DMX states:");
        for dmx_state in &first_frame.dmx_states {
            println!("  {}: Channel {} - {:?}", 
                dmx_state.device_name, 
                dmx_state.channel_id, 
                dmx_state.values);
        }
    }

    // Print show duration
    if let Some(last_frame) = show.frames.last() {
        println!("\nShow duration: {} seconds", last_frame.timestamp as f64 / 1000.0);
    }
}
