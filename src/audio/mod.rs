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
        let manager = AudioManager::<CpalBackend>::new(AudioManagerSettings::default()).unwrap();
        let sound_data = HashMap::new();
        Self {
            manager,
            _sound_data: sound_data,
        }
    }

    pub fn get_sound(&mut self, name: &str) -> Result<StaticSoundData, FromFileError> {
        let _sound_path = format!("src/audio/assets/{}", name);

        let sound_data = Asset::get(&name).unwrap();
        StaticSoundData::from_cursor(Cursor::new(sound_data.data), StaticSoundSettings::default())

        // if !self.sound_data.contains_key(name) {
        //     let sound = StaticSoundData::from_file(sound_path, StaticSoundSettings::default())?;
        //     self.sound_data.insert(name.to_string(), sound);
        // }

        // Ok(self.sound_data[name].clone())
    }

    pub fn play_sound(&mut self, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        let sound_data = self.get_sound(name)?;

        self.manager.play(sound_data.clone())?;

        Ok(())
    }
}
