use anyhow::Error;
use interprocess::local_socket::LocalSocketListener;
use log::error;
use rillrate::prime::{LiveTail, LiveTailOpts, Pulse, PulseOpts};
use rusty_halloween::prelude::*;
use rusty_halloween::InternalMessage;
use rusty_halloween::MessageKind;

use rusty_halloween::show::prelude::ShowChoice;
use rusty_halloween::show::prelude::ShowElement;
use rusty_halloween::show::prelude::ShowManager;

use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), Error> {
    // Send the data test to uart
    UARTProjectorController::init().await?;

    // return Ok(());

    println!("Starting Tokio console...");
    #[cfg(not(feature = "pi"))]
    console_subscriber::init();

    // Load the config file
    println!("Starting config...");
    let config = Config::load()?;

    // Make sure the socket is removed if the program exits, check if the file
    // exists first
    println!("Starting socket...");
    if std::path::Path::new("/tmp/pico.sock").exists() {
        std::fs::remove_file("/tmp/pico.sock")?;
    }

    // // Set up the local audio storage
    // println!("Starting audio system...");
    // FileStructure::verify();

    let _listener = LocalSocketListener::bind("/tmp/pico.sock")?;

    // Message queue
    let (message_queue_tx, mut message_queue_rx) = mpsc::channel(100);

    // Start the dashboard
    println!("Starting dashboard...");
    Dashboard::init(message_queue_tx.clone()).await?;

    // Initialize the lights
    println!("Starting lights...");
    let tx_clone = message_queue_tx.clone();
    #[allow(unused_variables, unused_mut)]
    let mut light_controller = LightController::init(&config, tx_clone).await?;

    #[cfg(feature = "spi")]
    let mut projector_controller = {
        // Initialize the projector
        println!("Starting projector...");
        let tx_clone = message_queue_tx.clone();
        #[allow(unused_variables, unused_mut)]
        let mut projector_controller = SPIProjectorController::init(tx_clone).await?;
    };

    // Initialize the audio
    println!("Starting audio...");
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

        while let Some(message) = message_queue_rx.recv().await {
            // TODO: Catch errors to not crash the thread

            // Update the pulse
            pulse.push(1);

            // Handle the message
            match message {
                MessageKind::InternalMessage(internal_message) => match internal_message {
                    #[allow(unused_variables)]
                    InternalMessage::Vision {
                        vision_file_contents,
                    } => {
                        live_tail.log_now(module_path!(), "INFO", "Vision command received");
                        if cfg!(feature = "pi") {
                            {
                                #[cfg(feature = "spi")]
                                if let Err(e) =
                                    projector_controller.spi_send_file(&vision_file_contents)
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
                            {
                                #[cfg(feature = "spi")]
                                if let Err(e) =
                                    projector_controller.spi_send_projector(frame_send_pack)
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

    // Get the shows on disk
    println!("Starting shows...");
    let tx_clone = message_queue_tx.clone();
    let shows = ShowManager::load_shows(tx_clone);

    // Start playing the first show
    let tx_clone = message_queue_tx.clone();
    let manager = ShowManager::new(shows, tx_clone);

    let (show_worker_channel_tx, show_worker_channel_rx) = mpsc::channel(100);

    println!("Starting show worker...");

    let worker_handle = tokio::spawn(async move {
        manager.start_show_worker(show_worker_channel_rx).await;
    });

    println!("Starting queue worker...");

    let queue_handle = tokio::spawn(async move {
        // Send startup command
        show_worker_channel_tx
            .send(vec![ShowElement::Idle { time: 5 }])
            .await
            .unwrap();

        // Send first show
        show_worker_channel_tx
            .send(vec![
                ShowElement::Home,
                ShowElement::PrepareShow(ShowChoice::Random),
            ])
            .await
            .unwrap();
    });

    println!("Joining...");

    let _ = tokio::join!(handle, worker_handle, queue_handle);

    // let _tx_clone = message_queue_tx.clone();

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
