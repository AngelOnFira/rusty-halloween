use std::{borrow::Cow, collections::HashMap, fmt::format, io::Cursor, path::Path, sync::Arc};

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

use crate::prelude::prelude::Song;

pub struct Audio {
    manager: Option<AudioManager<CpalBackend>>,
}

// #[cfg(feature="embed_audio")]
#[derive(RustEmbed)]
#[folder = "src/audio/assets"]
struct AudioAsset;

impl Audio {
    pub fn new(mut receiver: mpsc::Receiver<Arc<Song>>) -> Result<(), Error> {
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
                if let Some(manager) = audio_manager.manager.as_mut() {
                    if let Some(stream) = &sound.stream {
                        manager.play(stream.clone()).unwrap();
                    }
                }
            }
        });

        Ok(())
    }

    pub fn get_sound(name: &str) -> Result<Song, Box<dyn std::error::Error>> {
        #[allow(unused_variables)]
        let sound_path = format!("src/audio/assets/{}", name);

        // Try to load it from the embedded file
        if let Some(sound_data) = AudioAsset::get(name) {
            let sound_player = StaticSoundData::from_cursor(
                Cursor::new(sound_data.data),
                StaticSoundSettings::default(),
            )?;

            return Ok(Song {
                name: name.to_string(),
                stream: Some(sound_player),
            });
        }

        // Try to load it from the filesystem at
        // shows/<song_name>/<song_name>.mp3
        let sound_path = format!("shows/{}/{}.mp3", name, name);

        let sound_path = Path::new(&sound_path);
        if sound_path.exists() {
            let sound_player =
                StaticSoundData::from_file(sound_path, StaticSoundSettings::default())?;

            return Ok(Song {
                name: name.to_string(),
                stream: Some(sound_player),
            });
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
            if let Some(stream) = sound_data.stream {
                manager.play(stream)?;
            }
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
