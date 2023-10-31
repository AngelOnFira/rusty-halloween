use anyhow::Error;
use chrono::Local;
use env_logger::Builder;
use interprocess::local_socket::LocalSocketListener;
use log::debug;
use log::error;
use log::info;
use log::warn;
use log::LevelFilter;
use std::io::Write;
use tokio::signal;
// use rillrate::prime::{LiveTail, LiveTailOpts, Pulse, PulseOpts};
use rusty_halloween::prelude::*;
use rusty_halloween::InternalMessage;
use rusty_halloween::MessageKind;

use rusty_halloween::projector::uart::UARTProjectorController;
use rusty_halloween::show::prelude::ShowChoice;
use rusty_halloween::show::prelude::ShowElement;
use rusty_halloween::show::prelude::ShowManager;

use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), Error> {
    // Init the dashboard
    // dashboard_core::init();

    // Start logging
    Builder::new()
        .format(|buf, record| {
            writeln!(
                buf,
                "{} [{}] ({}:{}) - {}",
                Local::now().format("%Y-%m-%dT%H:%M:%S"),
                record.level(),
                record.file().unwrap_or("unknown"),
                record.line().unwrap_or(0),
                // record.module_path().unwrap_or("unknown"),
                record.args()
            )
        })
        .filter(None, LevelFilter::Info)
        .filter(Some("symphonia_core::probe"), LevelFilter::Off)
        .filter(Some("symphonia_bundle_mp3::demuxer"), LevelFilter::Off)
        .init();

    info!("Starting UART controller...");

    // Send the data test to uart
    // UARTProjectorController::init().await?;

    // return Ok(());

    info!("Starting Tokio console...");
    #[cfg(not(feature = "pi"))]
    console_subscriber::init();

    // Load the config file
    info!("Starting config...");
    #[cfg(feature = "pi")]
    let config = Config::load()?;

    // Make sure the socket is removed if the program exits, check if the file
    // exists first
    info!("Starting socket...");
    if std::path::Path::new("/tmp/pico.sock").exists() {
        std::fs::remove_file("/tmp/pico.sock")?;
    }

    // // Set up the local audio storage
    // info!("Starting audio system...");
    // FileStructure::verify();

    let _listener = LocalSocketListener::bind("/tmp/pico.sock")?;

    // Message queue
    let (message_queue_tx, mut message_queue_rx) = mpsc::channel(100);

    // Start the dashboard
    info!("Starting dashboard...");
    Dashboard::init(message_queue_tx.clone()).await?;

    // Initialize the lights
    #[cfg(feature = "pi")]
    let mut light_controller = {
        info!("Starting lights...");
        let tx_clone = message_queue_tx.clone();
        #[allow(unused_variables, unused_mut)]
        LightController::init(&config, tx_clone).await?
    };

    if !cfg!(feature = "pi") {
        warn!("Lights are not supported on this platform");
    }

    #[cfg(feature = "spi")]
    let mut projector_controller = {
        // Initialize the projector
        info!("Starting projector...");
        let tx_clone = message_queue_tx.clone();
        #[allow(unused_variables, unused_mut)]
        SPIProjectorController::init(tx_clone).await?
    };

    if !cfg!(feature = "pi") {
        warn!("Projectors are not supported on this platform");
    }

    // Initialize the projector
    info!("Starting projector...");
    let tx_clone = message_queue_tx.clone();
    #[allow(unused_variables, unused_mut)]
    let mut projector_controller = UARTProjectorController::init(tx_clone).await?;

    // Initialize the audio
    info!("Starting audio...");
    #[cfg(feature = "audio")]
    let (audio_channel_tx, audio_manager) = {
        let (audio_channel_tx, audio_channel_rx) = mpsc::channel(100);
        let audio_manager = Audio::new(audio_channel_rx);

        (audio_channel_tx, audio_manager)
    };

    let handle = tokio::spawn(async move {
        info!("Starting the reciever thread");
        // // Start a new pulse for the dashboard
        // let pulse = Pulse::new(
        //     "app.dashboard.all.pulse",
        //     Default::default(),
        //     PulseOpts::default().min(0).max(10),
        // );

        // // Start a new log list for the dashboard
        // let live_tail = LiveTail::new(
        //     "app.dashboard.Data.Messages",
        //     Default::default(),
        //     LiveTailOpts::default(),
        // );

        while let Some(message) = message_queue_rx.recv().await {
            // TODO: Catch errors to not crash the thread

            // // Update the pulse
            // pulse.push(1);

            // Handle the message
            match message {
                MessageKind::InternalMessage(internal_message) => match internal_message {
                    #[allow(unused_variables)]
                    InternalMessage::Vision {
                        vision_file_contents,
                    } => {
                        // live_tail.log_now(module_path!(), "INFO", "Vision command received");
                        if cfg!(feature = "pi") {
                            {
                                #[cfg(feature = "spi")]
                                if let Err(e) =
                                    projector_controller.uart_send_file(&vision_file_contents)
                                {
                                    error!("Failed to send projector command: {}", e);
                                }
                            }
                        } else {
                            error!("Projectors are not supported on this platform");
                        }
                    }
                    InternalMessage::Audio {
                        audio_file_contents: _audio_file_contents,
                    } => {
                        // live_tail.log_now(module_path!(), "INFO", "Audio
                        // command received");
                        if cfg!(feature = "audio") {
                            #[cfg(feature = "audio")]
                            match audio_manager {
                                Ok(_) => {
                                    audio_channel_tx.send(_audio_file_contents).await.unwrap();
                                }
                                Err(_) => {
                                    // live_tail.log_now(
                                    //     module_path!(),
                                    //     "ERROR",
                                    //     "Audio manager not initialized",
                                    // );
                                }
                            }
                        }
                    }
                    InternalMessage::Light {
                        light_id: _light_id,
                        enable: _enable,
                    } => {
                        // live_tail.log_now(module_path!(), "INFO", "Light
                        // command received");
                        #[cfg(feature = "pi")]
                        light_controller.set_pin(_light_id, _enable);
                    }
                    #[allow(unused_variables)]
                    InternalMessage::Projector(frame_send_pack) => {
                        // live_tail.log_now(module_path!(), "INFO", "Projector command received");
                        if cfg!(feature = "pi") {
                            {
                                if let Err(e) =
                                    projector_controller.uart_send_projector(frame_send_pack)
                                {
                                    error!("Failed to send projector command: {}", e);
                                }
                                // Sleep for half a second for the projector.
                                // This prevents them from sending information
                                // too fast, causing them to overlap data and
                                // freeze.
                                //
                                // The calulation for time is 51 frames * 32
                                // bits per frame / 57600 baud = 0.028 seconds =
                                // 28 milliseconds per frame, so we sleep for 50
                                // milliseconds to be safe.
                                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                            }
                        } else {
                            debug!("Projectors are not supported on this platform");
                        }
                    }
                },
            }
        }
    });

    // Get the shows on disk
    info!("Starting shows...");
    let tx_clone = message_queue_tx.clone();
    let shows = ShowManager::load_shows(tx_clone);

    // Start playing the first show
    let tx_clone = message_queue_tx.clone();
    let manager = ShowManager::new(shows, tx_clone);

    let (show_worker_channel_tx, show_worker_channel_rx) = mpsc::channel(100);

    info!("Starting show worker...");

    let worker_handle = tokio::spawn(async move {
        manager.start_show_worker(show_worker_channel_rx).await;
    });

    info!("Starting queue worker...");

    let queue_handle = tokio::spawn(async move {
        // Send startup command
        show_worker_channel_tx
            .send(vec![ShowElement::Idle { time: 5 }])
            .await
            .unwrap();

        // Send first show
        show_worker_channel_tx
            .send(vec![
                // ShowElement::LightTest,
                ShowElement::RunInit,
                ShowElement::Home,
                ShowElement::PrepareShow(ShowChoice::Random { last_song: None }),
            ])
            .await
            .unwrap();
    });

    info!("Joining...");

    // let _ = tokio::join!(handle, worker_handle, queue_handle);

    match signal::ctrl_c().await {
        Ok(()) => {}
        Err(err) => {
            eprintln!("Unable to listen for shutdown signal: {}", err);
            // we also shut down in case of error
        }
    }

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
