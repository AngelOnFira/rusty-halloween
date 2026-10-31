use anyhow::Error;
use chrono::Local;
use env_logger::Builder;
use log::{error, info, LevelFilter};
use rusty_halloween::{
    audio::Audio,
    config::Config,
    dmx::{DmxMessage, DmxState},
    laser::{LaserController, LaserMessage},
    lights::LightController,
    show::prelude::{ShowChoice, ShowElement, ShowManager},
    uart::UartController,
    AudioMessage, InternalMessage, MessageKind,
};
use std::io::Write;
use tokio::{signal, sync::mpsc};

#[tokio::main]
async fn main() -> Result<(), Error> {
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

    info!("Starting Tokio console...");
    #[cfg(not(feature = "pi"))]
    console_subscriber::init();

    // Load the config file
    info!("Loading config...");
    let config = Config::load_from_json("config/hardware.json")?;

    // // Set up the local audio storage
    // info!("Starting audio system...");
    // FileStructure::verify();

    // Message queue
    let (message_queue_tx, mut message_queue_rx) = mpsc::channel(100);

    // Initialize the lights
    let mut light_controller = {
        info!("Starting lights...");
        let tx_clone = message_queue_tx.clone();
        #[allow(unused_variables, unused_mut)]
        LightController::init(&config, tx_clone).await?
    };

    // Initialize UART controller
    let uart_tx = {
        let (uart_tx, uart_rx) = mpsc::channel(100);
        let uart_controller = UartController::init().unwrap();
        tokio::spawn(async move {
            uart_controller.start(uart_rx).await;
        });

        uart_tx
    };

    // Initialize the projector
    info!("Starting laser...");
    let tx_clone = message_queue_tx.clone();
    let (laser_tx, laser_rx) = mpsc::channel(100);
    let mut laser_controller = LaserController::init();
    let uart_tx_clone = uart_tx.clone();
    tokio::spawn(async move {
        laser_controller.start(laser_rx, uart_tx_clone).await;
    });

    // Initialize the audio
    info!("Starting audio...");
    #[cfg(feature = "audio")]
    let audio_tx = {
        let (audio_tx, audio_rx) = mpsc::channel(100);
        let audio_controller = Audio::new()?;
        tokio::spawn(async move {
            audio_controller.start(audio_rx).await;
        });
        audio_tx
    };

    // Initialize DMX
    info!("Starting DMX...");
    let (dmx_tx, dmx_rx) = mpsc::channel(100);
    let dmx_state = DmxState::init(config.clone());
    let uart_tx_clone = uart_tx.clone();
    tokio::spawn(async move {
        dmx_state.start(dmx_rx, uart_tx_clone).await;
    });

    let handle = tokio::spawn(async move {
        info!("Starting the reciever thread");

        while let Some(message) = message_queue_rx.recv().await {
            // TODO: Catch errors to not crash the thread

            // Handle the message
            match message {
                MessageKind::InternalMessage(internal_message) => match internal_message {
                    InternalMessage::Audio {
                        audio_file_contents,
                    } => {
                        if cfg!(feature = "audio") {
                            audio_tx
                                .send(AudioMessage::Play(audio_file_contents))
                                .await
                                .unwrap();
                        }
                    }
                    InternalMessage::AudioStop => {
                        if cfg!(feature = "audio") {
                            audio_tx.send(AudioMessage::Stop).await.unwrap();
                        }
                    }
                    InternalMessage::Light {
                        light_id: _light_id,
                        enable: _enable,
                    } => {
                        info!("Light command received");
                        #[cfg(feature = "pi")]
                        light_controller.set_pin(_light_id, _enable);
                    }
                    #[allow(unused_variables)]
                    InternalMessage::Laser(frame_send_pack) => {
                        info!("Projector command received");
                        {
                            laser_tx
                                .send(LaserMessage::Frame(frame_send_pack))
                                .await
                                .unwrap();
                        }
                    }
                    InternalMessage::DmxUpdateState(dmx_state_var_positions) => {
                        info!("DMX data received");
                        dmx_tx
                            .send(DmxMessage::UpdateState(dmx_state_var_positions))
                            .await
                            .unwrap();
                    }
                    InternalMessage::DmxSendRequest => {
                        info!("DMX request received");
                        dmx_tx.send(DmxMessage::Send).await.unwrap();
                    }
                    InternalMessage::DmxZeroOut => {
                        info!("DMX zero out received");
                        dmx_tx.send(DmxMessage::ZeroOut).await.unwrap();
                    }
                    InternalMessage::DmxShutdown => {
                        info!("DMX shutdown received");
                        dmx_tx.send(DmxMessage::Shutdown).await.unwrap();
                    }
                },
            }
        }
    });

    // Get the shows on disk
    info!("Starting shows...");
    let tx_clone = message_queue_tx.clone();
    let shows = ShowManager::load_shows(tx_clone, &config);

    // Start playing the first show
    let tx_clone = message_queue_tx.clone();
    let manager = ShowManager::new(shows, tx_clone);

    let (show_worker_channel_tx, show_worker_channel_rx) = mpsc::channel(100);

    info!("Starting show worker...");

    let worker_handle = tokio::spawn(async move {
        manager.start_show_worker(show_worker_channel_rx).await;
    });

    info!("Starting queue worker...");

    // Play a specific show first with SHOW=<show folder name>, otherwise pick
    // one at random
    let first_show = match std::env::var("SHOW") {
        Ok(show_name) => ShowChoice::Name(format!("{}-instructions-exported.json", show_name)),
        Err(_) => ShowChoice::Random { last_song: None },
    };

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
                ShowElement::PrepareShow(first_show),
            ])
            .await
            .unwrap();
    });

    info!("Joining...");

    // let _ = tokio::join!(handle, worker_handle, queue_handle);

    // Wait for Ctrl-C or a terminate signal
    let mut terminate = signal::unix::signal(signal::unix::SignalKind::terminate())?;
    tokio::select! {
        result = signal::ctrl_c() => {
            if let Err(err) = result {
                eprintln!("Unable to listen for shutdown signal: {}", err);
                // we also shut down in case of error
            }
        }
        _ = terminate.recv() => {}
    }

    // Black out DMX before exiting. DMX-Now keeps repeating the last frame it
    // got, so otherwise the fog machines and lights would stay on
    info!("Shutting down, blacking out DMX...");
    if let Err(e) = message_queue_tx
        .send(MessageKind::InternalMessage(InternalMessage::DmxShutdown))
        .await
    {
        error!("Failed to black out DMX: {}", e);
    }

    // Give the DMX and UART actors time to write the blackout
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

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
