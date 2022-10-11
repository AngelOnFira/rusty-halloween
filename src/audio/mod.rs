use std::{collections::HashMap, io::Cursor};

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
struct Asset;

impl Audio {
    pub fn new() -> Self {
        // TODO: Gracefully handle audio not being available
        let manager = AudioManager::<CpalBackend>::new(AudioManagerSettings::default()).expect(
            "Could not load the audio driver! Likely you need to start the code with sudo.",
        );

        let sound_data = HashMap::new();
        Self {
            manager,
            _sound_data: sound_data,
        }
    }

    pub fn get_sound(&mut self, name: &str) -> Result<StaticSoundData, Box<dyn std::error::Error>> {
        let _sound_path = format!("src/audio/assets/{}", name);

        if let Some(sound_data) = Asset::get(name) {
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
