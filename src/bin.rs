use anyhow::Error;
use interprocess::local_socket::LocalSocketListener;
use log::error;
use rillrate::prime::{LiveTail, LiveTailOpts, Pulse, PulseOpts};
use rusty_halloween::prelude::*;
use rusty_halloween::InternalMessage;
use rusty_halloween::MessageKind;

use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), Error> {
    // Load the config file
    let config = Config::load()?;

    // Make sure the socket is removed if the program exits, check if the file
    // exists first
    if std::path::Path::new("/tmp/pico.sock").exists() {
        std::fs::remove_file("/tmp/pico.sock")?;
    }

    let _listener = LocalSocketListener::bind("/tmp/pico.sock")?;

    // Message queue
    let (tx, mut rx) = mpsc::channel(100);

    // Start the dashboard
    Dashboard::init(tx.clone()).await?;

    // Initialize the lights
    let tx_clone = tx.clone();
    #[allow(unused_variables, unused_mut)]
    let mut light_controller = LightController::init(&config, tx_clone).await?;

    // Initialize the projector
    let tx_clone = tx.clone();
    #[allow(unused_variables, unused_mut)]
    let mut projector_controller = ProjectorController::init(tx_clone).await?;

    // Initialize the audio
    let (audio_channel_tx, audio_channel_rx) = mpsc::channel(100);
    let audio_manager = Audio::new(audio_channel_rx);

    let handle = tokio::spawn(async move {
        println!("Starting the reciever thread");
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

            println!("Message: {:?}", message);

            // Handle the message
            match message {
                MessageKind::InternalMessage(internal_message) => match internal_message {
                    #[allow(unused_variables)]
                    InternalMessage::Vision {
                        vision_file_contents,
                    } => {
                        live_tail.log_now(module_path!(), "INFO", "Vision command received");
                        if cfg!(feature = "pi") {
                            #[cfg(feature = "pi")]
                            {
                                if let Err(e) =
                                    projector_controller.send_file(&vision_file_contents)
                                {
                                    error!("Failed to send projector command: {}", e);
                                }
                            }
                        } else {
                            error!("Projectors are not supported on this platform");
                        }
                    }
                    InternalMessage::Audio {
                        audio_file_contents,
                    } => {
                        live_tail.log_now(module_path!(), "INFO", "Audio command received");
                        match audio_manager {
                            Ok(_) => {
                                audio_channel_tx.send(audio_file_contents).await.unwrap();
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
                    InternalMessage::Light { light_id, enable } => {
                        live_tail.log_now(module_path!(), "INFO", "Light command received");
                        light_controller.set_pin(light_id, enable);
                    }
                    #[allow(unused_variables)]
                    InternalMessage::Projector(frame_send_pack) => {
                        live_tail.log_now(module_path!(), "INFO", "Projector command received");
                        if cfg!(feature = "pi") {
                            #[cfg(feature = "pi")]
                            {
                                if let Err(e) = projector_controller.send_projector(frame_send_pack)
                                {
                                    error!("Failed to send projector command: {}", e);
                                }
                            }
                        } else {
                            error!("Projectors are not supported on this platform");
                        }
                    }
                },
            }
        }
    });

    // Initialize the show
    let tx_clone = tx.clone();
    let show = ShowManager::load_show_file("halloween.json".to_string(), tx_clone);
    // tokio::spawn(async move {
    //     show.start_show().await;
    // });

    //     // Join the handle
    //     handle.await?;

    let _ = tokio::join!(show.start_show(), handle,);

    // let _tx_clone = tx.clone();

    // // TODO: Rewrite this to change directly to internal message type first
    // for mut conn in listener.incoming().filter_map(handle_error) {
    //     // Recieve the data
    //     // let mut conn = BufReader::new(conn);
    //     // let mut buffer = String::new();
    //     // conn.read_line(&mut buffer)?;

    //     // Try to decode it as protobuf
    //     // TODO: Reply with an error if this fails
    //     // let proto = PicoMessage::parse_from_reader(&mut conn).unwrap();

    //     // Debug the message
    //     debug!("{:#?}", proto);

    //     // Add the message to the queue
    //     // tx_clone
    //     //     .send(MessageKind::ExternalMessage(proto))
    //     //     .await
    //     //     .unwrap();

    //     // Translate it to the projector protocol
    // }

    Ok(())
}
