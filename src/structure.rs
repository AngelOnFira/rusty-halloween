use std::{
    fs::{self, File},
    io::Write,
    path::Path,
};

use crate::prelude::Audio;

pub struct FileStructure {}

impl FileStructure {
    pub fn verify() {
        // Make sure there is a folder in this directory called shows. If there
        // isn't, create it.
        if !Path::new("shows").exists() {
            fs::create_dir("shows").unwrap();
        }

        // Make sure every embedded song is in its own folder
        Audio::get_embedded_sounds().iter().for_each(|sound| {
            if !Path::new(&format!("shows/{}", sound)).exists() {
                fs::create_dir(&format!("shows/{}", sound)).unwrap();
            }
        });

        // Save each embedded song to its own folder
        Audio::get_embedded_sounds().iter().for_each(|sound| {
            let name = format!("shows/{}/{}.mp3", sound, sound);
            if !Path::new(&name).exists() {
                File::create(&name)
                    .unwrap()
                    .write_all(&Audio::get_sound_file(sound))
                    .unwrap();
            }
        });
    }
}
