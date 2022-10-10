use anyhow::Error;
use audio::Audio;
use config::Config;
use dashboard::Dashboard;
use interprocess::local_socket::{LocalSocketListener, LocalSocketStream};
use log::{debug, error};
use proto_schema::schema::PicoMessage;
use protobuf::Message;
use rillrate::prime::{Pulse, PulseOpts, LiveTail, LiveTailOpts};
use std::io::{self};
use tokio::sync::mpsc;

mod audio;
mod config;
mod dashboard;
mod pico;
mod projector;
mod proto_schema;

#[cfg(feature = "pi")]
use lights::Lights;
#[cfg(feature = "pi")]
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
    #[cfg(feature = "pi")]
    let mut lights = Lights::init(&config)?;

    let mut audio_manager = Audio::new();
    // audio_manager.play_sound("song1.mp3").unwrap();

    tokio::spawn(async move {
        // Start a new pulse for the dashboard
        let pulse = Pulse::new(
            "messages.dashboard.all.pulse",
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
            // Update the pulse
            pulse.push(1);

            // Handle the message
            match message.payload {
                Some(proto_schema::schema::pico_message::Payload::Audio(audio_command)) => {
                    live_tail.log_now(module_path!(), "INFO", "Audio command received");
                    let _ = audio_manager.play_sound(&audio_command.audioFile);
                }
                Some(proto_schema::schema::pico_message::Payload::Light(light_command)) => {
                    live_tail.log_now(module_path!(), "INFO", "Light command received");
                    if cfg!(feature = "pi") {
                        #[cfg(feature = "pi")]
                        {
                            // If the light ID is out of range of a u8, print an
                            // error
                            if light_command.lightId >= 256 {
                                error!("Light ID {} is out of range", light_command.lightId);
                            } else {
                                lights.set_pin(light_command.lightId as u8, light_command.enable);
                            }
                        }
                    } else {
                        error!("Lights are not supported on this platform");
                    }
                }
                Some(proto_schema::schema::pico_message::Payload::Projector(projector_command)) => {
                    live_tail.log_now(module_path!(), "INFO", "Projector command received");
                    println!("Projector: {:#?}", projector_command);
                }
                None => {}
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
