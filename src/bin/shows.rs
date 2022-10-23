use std::time::Instant;

use rand::Rng;
use rusty_halloween::prelude::*;

fn main() {
    println!("Hello, world!");

    // Thread random
    let mut rng = rand::thread_rng();

    let frames = (0..100).into_iter().map(|i| {
        let mut frame = Frame {
            timestamp: i * 500,
            lights: (0..MAX_LIGHTS).into_iter().map(|_| match rng.gen_range(0..=3) {
                0 => Some(false),
                1 => Some(true),
                _ => None,
            }).collect(),
            lasers: (0..MAX_PROJECTORS).into_iter().map(|_| None).collect(),
        };
        frame
    }).collect::<Vec<Frame>>();

    // Create a show
    let show = Show {
        song: "test".to_string(),
        frames: frames,
    };

    // Write the show to a json file
    let data = ShowManager::new().save_show(show);

    // Save the show to a file
    std::fs::write("test.json", data).unwrap();
    
}
