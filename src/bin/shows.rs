use rand::Rng;
use rusty_halloween::{
    prelude::*,
    show::prelude::{Frame, Show, ShowManager, Song},
};
use std::char::MAX;

const BPM: f32 = 166.0;

fn main() {
    println!("Hello, world!");

    // Thread random
    let mut rng = rand::thread_rng();

    let frames = Show::row_flashing();

    // Create a show
    let show = Show {
        song: Song {
            name: "song3.mp3".to_string(),
            stream: None,
        },
        frames: frames,
    };

    // Write the show to a json file
    let data = ShowManager::new().save_show(show);

    // Save the show to a file
    std::fs::write("src/show/assets/halloween.json", data).unwrap();
}
