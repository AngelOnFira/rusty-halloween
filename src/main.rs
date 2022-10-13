use anyhow::Error;
use audio::Audio;
use config::Config;
use dashboard::Dashboard;
use interprocess::local_socket::{LocalSocketListener, LocalSocketStream};
use log::{debug, error};
use projector::ProjectorController;
use proto_schema::schema::PicoMessage;
use protobuf::Message;
use rillrate::prime::{LiveTail, LiveTailOpts, Pulse, PulseOpts};
use std::io::{self};
use tokio::sync::mpsc;

mod audio;
mod config;
mod dashboard;
mod projector;
mod proto_schema;

use lights::LightController;
mod lights;

fn handle_error(conn: io::Result<LocalSocketStream>) -> Option<LocalSocketStream> {
    match conn {
        Ok(val) => Some(val),
        Err(error) => {
            eprintln!("Incoming connection failed: {}", error);
            None
        }
    }
}

enum InternalMessage {
    Vision { vision_file: String },
}

enum MessageKind {
    ExternalMessage(PicoMessage),
    InternalMessage(InternalMessage),
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    // Load the config file
    let config = Config::load()?;

    // Make sure the socket is removed if the program exits
    std::fs::remove_file("/tmp/pico.sock").ok();

    let listener = LocalSocketListener::bind("/tmp/pico.sock")?;

    // Message queue
    let (tx, mut rx) = mpsc::channel(32);

    // Start the dashboard
    Dashboard::init(tx.clone()).await?;

    // Initialize the lights
    let tx_clone = tx.clone();
    let mut light_controller = LightController::init(&config, tx_clone).await?;

    // Initialize the projector
    let tx_clone = tx.clone();
    let mut projector_controller = ProjectorController::init(tx_clone)?;

    // Initialize the audio
    let mut audio_manager = Audio::new();

    tokio::spawn(async move {
        // Start a new pulse for the dashboard
        let pulse = Pulse::new(
            "app.dashboard.all.pulse",
            Default::default(),
            PulseOpts::default().min(0).max(10),
        );

        // Start a new log list for the dashboard
        let live_tail = LiveTail::new(
            "app.dashboard.Data.Messages",
            Default::default(),
            LiveTailOpts::default(),
        );

        while let Some(message) = rx.recv().await {
            // TODO: Catch errors to not crash the thread

            // Update the pulse
            pulse.push(1);

            // Handle the message
            match message {
                MessageKind::InternalMessage(internal_message) => match internal_message {
                    InternalMessage::Vision { vision_file } => {
                        live_tail.log_now(module_path!(), "INFO", "Vision command received");
                        if cfg!(feature = "pi") {
                            #[cfg(feature = "pi")]
                            {
                                if let Err(e) = projector_controller.send_file(vision_file) {
                                    error!("Failed to send projector command: {}", e);
                                }
                            }
                        } else {
                            error!("Projectors are not supported on this platform");
                        }
                    }
                },
                MessageKind::ExternalMessage(external_message) => match external_message.payload {
                    Some(proto_schema::schema::pico_message::Payload::Audio(audio_command)) => {
                        live_tail.log_now(module_path!(), "INFO", "Audio command received");
                        match audio_manager {
                            Ok(ref mut audio_manager) => {
                                audio_manager.play_sound(&audio_command.audio_file).unwrap();
                            }
                            Err(_) => {
                                live_tail.log_now(
                                    module_path!(),
                                    "ERROR",
                                    "Audio manager not initialized",
                                );
                            }
                        }
                    }
                    Some(proto_schema::schema::pico_message::Payload::Light(light_command)) => {
                        live_tail.log_now(module_path!(), "INFO", "Light command received");
                        if cfg!(feature = "pi") {
                            #[cfg(feature = "pi")]
                            {
                                // If the light ID is out of range of a u8, print an
                                // error
                                if light_command.light_id >= 256 {
                                    error!("Light ID {} is out of range", light_command.light_id);
                                } else {
                                    light_controller.set_pin(
                                        light_command.light_id as u8,
                                        light_command.enable,
                                    );
                                }
                            }
                        } else {
                            error!("Lights are not supported on this platform");
                        }
                    }
                    Some(proto_schema::schema::pico_message::Payload::Projector(
                        projector_command,
                    )) => {
                        live_tail.log_now(module_path!(), "INFO", "Projector command received");
                        if cfg!(feature = "pi") {
                            #[cfg(feature = "pi")]
                            {
                                if let Err(e) =
                                    projector_controller.projector_to_frames(projector_command)
                                {
                                    error!("Failed to send projector command: {}", e);
                                }
                            }
                        } else {
                            error!("Projectors are not supported on this platform");
                        }
                    }

                    None => {}
                },
            }
        }
    });

    let tx_clone = tx.clone();
    for mut conn in listener.incoming().filter_map(handle_error) {
        // Recieve the data
        // let mut conn = BufReader::new(conn);
        // let mut buffer = String::new();
        // conn.read_line(&mut buffer)?;

        // Try to decode it as protobuf
        // TODO: Reply with an error if this fails
        let proto = PicoMessage::parse_from_reader(&mut conn).unwrap();

        // Debug the message
        debug!("{:#?}", proto);

        // Add the message to the queue
        tx_clone.send(proto).await.unwrap();

        // Translate it to the projector protocol
    }

    Ok(())
}
