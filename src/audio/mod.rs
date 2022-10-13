use std::{collections::HashMap, io::Cursor};

use anyhow::Error;
use kira::{
    manager::{backend::cpal::CpalBackend, AudioManager, AudioManagerSettings},
    sound::{
        static_sound::{StaticSoundData, StaticSoundSettings},
        FromFileError,
    },
};
use rust_embed::RustEmbed;

pub struct Audio {
    manager: AudioManager<CpalBackend>,
    _sound_data: HashMap<String, StaticSoundData>,
}

#[derive(RustEmbed)]
#[folder = "src/audio/assets"]
struct AudioAsset;

impl Audio {
    pub fn new() -> Result<Self, Error> {
        // TODO: Gracefully handle audio not being available
        let manager = AudioManager::<CpalBackend>::new(AudioManagerSettings::default())?;

        let sound_data = HashMap::new();
        Ok(Self {
            manager,
            _sound_data: sound_data,
        })
    }

    pub fn get_sound(&mut self, name: &str) -> Result<StaticSoundData, Box<dyn std::error::Error>> {
        let _sound_path = format!("src/audio/assets/{}", name);

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

    pub fn play_sound(&mut self, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        let sound_data = self.get_sound(name)?;

        self.manager.play(sound_data)?;

        Ok(())
    }
}
