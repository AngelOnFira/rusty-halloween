use crate::{
    prelude::{prelude::Song, Audio},
    MessageKind,
};
use rillrate::prime::{Click, ClickOpts, Gauge, GaugeOpts};
use rust_embed::RustEmbed;
use std::{cmp::max, time::Duration};
use tokio::{
    sync::mpsc,
    time::{sleep, Instant},
};

use super::{show::Show, ShowAsset};

pub struct ShowManager {
    pub current_show: Option<Show>,
    pub start_time: Option<Instant>,
    pub shows: Option<Vec<Show>>,
    pub message_queue: Option<mpsc::Sender<MessageKind>>,
    pub progress_bar: Gauge,
}

impl ShowManager {
    pub fn new() -> Self {
        let progress_bar = Gauge::new(
            "app.dashboard.Show.Progress",
            Default::default(),
            GaugeOpts::default().min(0.0).max(0.0),
        );

        Self {
            current_show: None,
            start_time: None,
            message_queue: None,
            shows: None,
            progress_bar,
        }
    }

    pub fn set_show(
        &mut self,
        show_file_contents: String,
        message_queue: mpsc::Sender<MessageKind>,
    ) {
        self.current_show = Some(Show::load_show(show_file_contents));
    }

    pub async fn start_show(mut self) {
        // Set the timer
        self.start_time = Some(Instant::now());

        let mut current_show = self.current_show.unwrap();

        // Start the song
        self.message_queue
            .as_ref()
            .unwrap()
            .try_send(MessageKind::InternalMessage(
                crate::InternalMessage::Audio {
                    audio_file_contents: current_show.song.name.clone(),
                },
            ))
            .unwrap();

        let progress_bar = Gauge::new(
            "app.dashboard.Show.Progress",
            Default::default(),
            GaugeOpts::default()
                .min(0.0)
                .max(current_show.frames.last().unwrap().timestamp as f64 / 1000.0),
        );

        self.progress_bar = progress_bar;

        // Spawn a progress bar thread
        tokio::spawn(async move {
            loop {
                sleep(Duration::from_millis(100)).await;
                let current_time = self.start_time.unwrap().elapsed().as_millis() as f64;
                self.progress_bar.set(current_time / 1000.0);
            }
        });

        // Start the show thread
        tokio::spawn(async move {
            loop {
                // Get the next frame
                let curr_frame = current_show.frames.remove(0);

                // Sleep until the current frame is ready
                let curr_time = self.start_time.unwrap().elapsed().as_millis() as i64;
                let sleep_time = max(curr_frame.timestamp as i64 - curr_time, 0);
                sleep(Duration::from_millis(sleep_time as u64)).await;

                // Execute the current frame

                // Send all the lights data
                for (i, light) in curr_frame.lights.iter().enumerate() {
                    if let Some(light) = light {
                        self.message_queue
                            .as_mut()
                            .unwrap()
                            .try_send(MessageKind::InternalMessage(
                                crate::InternalMessage::Light {
                                    light_id: i as u8 + 1,
                                    enable: *light,
                                },
                            ))
                            .unwrap();
                    }
                }

                // Print the message queue size
                println!(
                    "Message queue size: {}",
                    self.message_queue.as_ref().unwrap().capacity()
                );

                // Send all the lasers data
                // ...

                if current_show.frames.len() == 0 {
                    break;
                }
            }
        });
    }

    pub fn load_shows(message_queue: mpsc::Sender<MessageKind>) -> Vec<Show> {
        // Find all folders in the shows folder
        let shows = std::fs::read_dir("shows").unwrap();

        let names = shows
            .into_iter()
            .map(|show| {
                show.unwrap()
                    .path()
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_string()
            })
            .collect::<Vec<String>>();

        println!("Found shows: {:?}", names);

        // For each one, load the show and song
        let shows = names
            .iter()
            .map(|name| {
                // Print the show name
                println!("Loading show: {}", name);

                // Load the show file
                let show_file =
                    std::fs::read_to_string(format!("shows/{}/instructions.json", name)).unwrap();

                // Load the show

                // Set up the buttons on the dashboard
                let click = Click::new(
                    format!("app.dashboard.Shows.{}", name),
                    ClickOpts::default().label("Start"),
                );
                let this = click.clone();

                let _message_queue_clone = message_queue.clone();
                click.sync_callback(move |_envelope| {
                    // Start loading that song
                    this.apply();
                    Ok(())
                });

                Show {
                    song: Audio::get_sound(name).unwrap(),
                    frames: Vec::new(),
                }
            })
            .collect::<Vec<Show>>();

        shows
    }
}
