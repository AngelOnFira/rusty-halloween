use rusty_halloween::show::prelude::{ShowManager, UnloadedShow};

const BPM: f32 = 166.0;

fn main() {
    println!("Hello, world!");

    // Thread random
    let _rng = rand::thread_rng();

    let frames = UnloadedShow::row_flashing();

    // Create a show
    let show = UnloadedShow {
        name: "song3.mp3".to_string(),
        frames: frames,
    };

    // Write the show to a json file
    let data = ShowManager::save_show(show);

    // Save the show to a file
    std::fs::write("src/show/assets/halloween.json", data).unwrap();
}
