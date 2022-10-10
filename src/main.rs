use anyhow::Error;
use audio::Audio;
use config::Config;
use dashboard::Dashboard;
use interprocess::local_socket::{LocalSocketListener, LocalSocketStream};
use lights::Lights;
use log::{debug, error};
use proto_schema::schema::PicoMessage;
use protobuf::Message;
use std::io::{self};

mod audio;
mod config;
mod dashboard;
mod lights;
mod pico;
mod projector;
mod proto_schema;

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

    // Start the dashboard
    Dashboard::init().await?;

    // Initialize the lights
    let mut lights = Lights::init(&config)?;

    let mut audio_manager = Audio::new();
    // audio_manager.play_sound("song1.mp3").unwrap();

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

        // Handle the message
        match proto.payload {
            Some(proto_schema::schema::pico_message::Payload::Audio(audio_command)) => {
                let _ = audio_manager.play_sound(&audio_command.audioFile);
            }
            Some(proto_schema::schema::pico_message::Payload::Light(light_command)) => {
                // If the light ID is out of range of a u8, print an
                // error
                if light_command.lightId >= 256 {
                    error!("Light ID {} is out of range", light_command.lightId);
                } else {
                    lights.set_pin(light_command.lightId as u8, light_command.enable);
                }
            }
            Some(proto_schema::schema::pico_message::Payload::Projector(projector_command)) => {
                println!("Projector: {:#?}", projector_command);
            }
            None => {}
        }

        // Translate it to the projector protocol
    }

    Ok(())
}
