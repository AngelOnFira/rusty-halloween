use std::collections::HashMap;

use kira::{
    manager::{backend::cpal::CpalBackend, AudioManager, AudioManagerSettings},
    sound::{
        static_sound::{StaticSoundData, StaticSoundSettings},
        FromFileError,
    },
};

pub struct Audio {
    manager: AudioManager<CpalBackend>,
    sound_data: HashMap<String, StaticSoundData>,
}

pub enum AudioError {
    FromFil,
}

impl Audio {
    pub fn new() -> Self {
        let manager = AudioManager::<CpalBackend>::new(AudioManagerSettings::default()).unwrap();
        let sound_data = HashMap::new();
        Self {
            manager,
            sound_data,
        }
    }

    pub fn get_sound(&mut self, name: &str) -> Result<StaticSoundData, FromFileError> {
        let sound_path = format!("src/audio/assets/{}", name);

        if !self.sound_data.contains_key(name) {
            let sound = StaticSoundData::from_file(sound_path, StaticSoundSettings::default())?;
            self.sound_data.insert(name.to_string(), sound);
        }

        Ok(self.sound_data[name].clone())
    }

    pub fn play_sound(&mut self, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        let sound_data = self.get_sound(name)?;

        self.manager.play(sound_data.clone())?;

        Ok(())
    }
}
