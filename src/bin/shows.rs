use rand::Rng;
use rusty_halloween::prelude::*;

const BPM: f32 = 168.0;

fn main() {
    println!("Hello, world!");

    // Thread random
    let mut rng = rand::thread_rng();

    let frames = (0..1_000)
        .into_iter()
        .map(|i| {
            let frame = Frame {
                timestamp: i * (60.0 / BPM * 1000.0) as u64,
                lights: (0..MAX_LIGHTS)
                    .into_iter()
                    .map(|_| match rng.gen_range(0..=3) {
                        0 => Some(false),
                        1 => Some(true),
                        _ => None,
                    })
                    .collect(),
                lasers: (0..MAX_PROJECTORS).into_iter().map(|_| None).collect(),
            };
            frame
        })
        .collect::<Vec<Frame>>();

    // Create a show
    let show = Show {
        song: "song3.mp3".to_string(),
        frames: frames,
    };

    // Write the show to a json file
    let data = ShowManager::new().save_show(show);

    // Save the show to a file
    std::fs::write("src/show/assets/halloween.json", data).unwrap();
}
