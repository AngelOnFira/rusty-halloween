use self::pack::{DrawPack, HeaderPack};
use crate::proto_schema::schema::pico_message::Payload;
use crate::{projector::pack::CheckSum, proto_schema::schema::PicoMessage};
use crate::proto_schema::schema::Projector;
use packed_struct::PackedStruct;
use rillrate::prime::{Click, ClickOpts};
use rppal::spi::{Bus, SlaveSelect, Spi};
use rust_embed::RustEmbed;
use tokio::sync::mpsc;

mod pack;

pub struct ProjectorController {
    pub spi: Spi,
}

type Frame = [u8; 4];

pub struct MessageSendPack {
    pub header: Frame,
    pub draw_instructions: Vec<Frame>,
}

#[derive(RustEmbed)]
#[folder = "src/projector/visions/assets"]
struct VisionAsset;

impl ProjectorController {
    pub fn init( message_queue: mpsc::Sender<MessageKind>,) -> Result<Self, anyhow::Error> {
        // Set up SPI
        let spi = Spi::new(
            Bus::Spi0,
            SlaveSelect::Ss0,
            115_200,
            rppal::spi::Mode::Mode0,
        )?;

        for vision in VisionAsset::iter() {
            // Load each vision
            let click = Click::new(
                format!("app.dashboard.Visions.Vision-{}", vision),
                ClickOpts::default().label("Click Me!"),
            );

            let this = click.clone();

            click.sync_callback(move |envelope| {
                if let Some(action) = envelope.action {
                    log::warn!("ACTION: {:?}", action);
                    this.apply();
                    
                    let mut light_message = PicoMessage::new();
                    light_message.payload = Some(Payload::Light(Vision {
                        vision
                        ..Default::default()
                    }));

                    message_queue_clone.blocking_send(light_message)?;
                }
                Ok(())
            });
        }

        Ok(ProjectorController { spi })
    }

    pub fn projector_to_frames(
        &mut self,
        projector_command: Projector,
    ) -> Result<(), anyhow::Error> {
        // Create the header from the message
        let header_command = projector_command.header;
        let header = HeaderPack {
            projector_id: (header_command.projector_id as u8).into(),
            point_count: (header_command.point_count as u8).into(),
            home: header_command.home,
            enable: header_command.enable,
            configuration_mode: header_command.configuration_mode,
            draw_boundary: header_command.draw_boundary,
            oneshot: header_command.oneshot,
            speed_profile: (header_command.speed_profile as u8).into(),
            checksum: header_command.checksum,
            ..HeaderPack::default()
        }
        .checksum_pack();

        let mut draw_instructions = Vec::new();
        for draw_command in projector_command.draw_instructions {
            let draw_pack = DrawPack {
                x: (draw_command.xCoOrd as u16).into(),
                y: (draw_command.yCoOrd as u16).into(),
                red: (draw_command.red as u8).into(),
                green: (draw_command.green as u8).into(),
                blue: (draw_command.blue as u8).into(),
                checksum: draw_command.checksum,
                ..DrawPack::default()
            }
            .checksum_pack();
            draw_instructions.push(draw_pack);
        }

        // Create a message pack
        let message_pack = MessageSendPack {
            header,
            draw_instructions,
        };

        self.send_projector(message_pack)?;

        Ok(())
    }

    pub fn send_projector(
        &mut self,
        projector_command: MessageSendPack,
    ) -> Result<(), anyhow::Error> {
        // Send the header
        self.spi.write(&projector_command.header)?;

        // Send each draw instruction
        for draw_pack in &projector_command.draw_instructions {
            self.spi.write(draw_pack)?;
        }

        // Send extra frames to get to 51 total frames
        for _ in 0..(51 - projector_command.draw_instructions.len() - 1) {
            self.spi.write(&[0, 0, 0, 0])?;
        }

        Ok(())
    }

    pub fn send_file(&mut self, file_path: &str) -> Result<(), anyhow::Error> {
        let file_string = std::fs::read_to_string(file_path)?;

        let frames = file_string
            .lines()
            .map(|s| u32::to_be_bytes(u32::from_str_radix(s, 16).unwrap()))
            .collect::<Vec<[u8; 4]>>();

        // Create frames in the proto format

        // Send the header
        self.spi.write(&frames[0])?;

        // Send each draw instruction
        for frame in frames[1..].iter() {
            self.spi.write(frame)?;
        }

        // Send any extra frames required to get to 51 total frames
        for _ in 0..(51 - frames.len()) {
            self.spi.write(&[0, 0, 0, 0])?;
        }

        Ok(())
    }
}
