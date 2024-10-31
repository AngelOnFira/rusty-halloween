use std::{
    borrow::Cow,
    io::Cursor,
    path::Path,
    sync::{Arc, Mutex},
};

use anyhow::Error;
use kira::{
    manager::{backend::cpal::CpalBackend, AudioManager, AudioManagerSettings},
    sound::{
        static_sound::{StaticSoundData, StaticSoundSettings},
        FromFileError,
    }, tween::Tween,
};
use log::{error, info};
use rust_embed::RustEmbed;
use tokio::sync::mpsc;

use crate::AudioMessage;

pub struct Audio {
    manager: Option<AudioManager<CpalBackend>>,
}

#[derive(Clone, Debug)]
pub struct LoadingSong {
    pub name: String,
    pub stream: Arc<Mutex<Option<StaticSoundData>>>,
}

#[derive(Debug, Clone)]
pub struct LoadedSong {
    pub name: String,
    // We can clone this since inside it uses an Arc to share the data
    pub stream: StaticSoundData,
}

// #[cfg(feature="embed_audio")]
#[derive(RustEmbed)]
#[folder = "src/audio/assets"]
struct AudioAsset;

impl Audio {
    pub fn new() -> Result<Self, Error> {
        // TODO: Gracefully handle audio not being available
        if cfg!(feature = "audio") {
            // Adding the cfg feature here for audio allows us to go through
            // the rest of audio testing, but not actually play sound
            match AudioManager::<CpalBackend>::new(AudioManagerSettings::default()) {
                Ok(manager) => Ok(Self {
                    manager: Some(manager),
                }),
                Err(e) => {
                    error!("Error initializing audio: {}", e);
                    Ok(Self { manager: None })
                }
            }
        } else {
            Ok(Audio { manager: None })
        }
    }

    pub async fn start(mut self, mut receiver: mpsc::Receiver<AudioMessage>) {
        while let Some(message) = receiver.recv().await {
            match message {
                AudioMessage::Play(sound) => {
                    info!("Playing sound: {}", sound.name);
                    // Create a new audio manager instance for each play
                    if let Ok(mut manager) = AudioManager::<CpalBackend>::new(AudioManagerSettings::default()) {
                        if let Err(e) = manager.play(sound.stream) {
                            error!("Failed to play audio: {}", e);
                        }
                        // Store the new manager
                        self.manager = Some(manager);
                    } else {
                        error!("Failed to create new audio manager");
                    }
                }
                AudioMessage::Stop => {
                    info!("Stopping audio playback");
                    if let Some(manager) = self.manager.as_mut() {
                        manager.pause(Tween::default()).unwrap();
                    }
                }
            }
        }
    }

    pub fn get_sound(name: &str) -> Result<LoadingSong, Box<dyn std::error::Error>> {
        #[allow(unused_variables)]
        let sound_path = format!("src/audio/assets/{}", name);

        // Print that this song is loading
        info!("Loading song: {}", name);

        // Try to load it from the embedded file
        if let Some(sound_data) = AudioAsset::get(name) {
            let sound_player = StaticSoundData::from_cursor(
                Cursor::new(sound_data.data),
                StaticSoundSettings::default(),
            )?;

            return Ok(LoadingSong {
                name: name.to_string(),
                stream: Arc::new(Mutex::new(Some(sound_player))),
            });
        }

        // Try to load it from the filesystem at
        // shows/<song_name>/<song_name>.mp3
        let sound_path_local = format!("shows/{}/{}.mp3", name, name);

        if Path::new(&sound_path_local).exists() {
            let song_stream = Arc::new(Mutex::new(None));

            let song_future = LoadingSong {
                name: name.to_string(),
                stream: song_stream.clone(),
            };

            // Start loading it in a new thread
            tokio::spawn(async move {
                // Load the song
                let sound_player = StaticSoundData::from_file(
                    Path::new(&sound_path_local),
                    StaticSoundSettings::default(),
                )
                .unwrap();

                // Save the song to the stream
                *song_stream.lock().unwrap() = Some(sound_player);

                info!("Finished loading song");
            });

            // Return an empty song for now, this will be filled in later once
            // it's loaded on the other thread
            return Ok(song_future);
        } else {
            error!("Sound not found: {}", sound_path_local);
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
