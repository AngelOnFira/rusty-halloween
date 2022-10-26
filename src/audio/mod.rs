use std::{borrow::Cow, collections::HashMap, io::Cursor, path::Path};

use anyhow::Error;
use kira::{
    manager::{backend::cpal::CpalBackend, AudioManager, AudioManagerSettings},
    sound::{
        static_sound::{StaticSoundData, StaticSoundSettings},
        FromFileError,
    },
};
use rust_embed::RustEmbed;
use tokio::sync::mpsc;

pub struct Audio {
    manager: Option<AudioManager<CpalBackend>>,
}

// #[cfg(feature="embed_audio")]
#[derive(RustEmbed)]
#[folder = "src/audio/assets"]
struct AudioAsset;

impl Audio {
    pub fn new(mut receiver: mpsc::Receiver<String>) -> Result<(), Error> {
        // TODO: Gracefully handle audio not being available
        let mut audio_manager =
            match AudioManager::<CpalBackend>::new(AudioManagerSettings::default()) {
                Ok(manager) => Self {
                    manager: Some(manager),
                },
                Err(e) => {
                    println!("Error initializing audio: {}", e);
                    Self { manager: None }
                }
            };

        // Start the audio manager thread
        tokio::spawn(async move {
            while let Some(sound) = receiver.recv().await {
                audio_manager.play_sound(&sound).unwrap();
            }
        });

        Ok(())
    }

    pub fn get_sound(name: &str) -> Result<StaticSoundData, Box<dyn std::error::Error>> {
        #[allow(unused_variables)]
        let sound_path = format!("src/audio/assets/{}", name);

        if let Some(sound_data) = AudioAsset::get(name) {
            let sound_player = StaticSoundData::from_cursor(
                Cursor::new(sound_data.data),
                StaticSoundSettings::default(),
            )?;

            return Ok(sound_player);
        }

        // Return "Sound not found" error
        Err(Box::new(FromFileError::IoError(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Sound not found",
        ))))
    }

    pub fn get_sound_file(name: &str) -> Cow<[u8]> {
        let sound_data = AudioAsset::get(&format!("{}.mp3", name)).unwrap();
        sound_data.data
    }

    pub fn play_sound(&mut self, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        let sound_data = Audio::get_sound(name)?;

        if let Some(manager) = self.manager.as_mut() {
            manager.play(sound_data)?;
        }

        Ok(())
    }

    pub fn get_embedded_sounds() -> Vec<String> {
        AudioAsset::iter()
            .map(|s| {
                Path::new(&s.to_string())
                    .file_stem()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_string()
            })
            .collect()
    }
}
