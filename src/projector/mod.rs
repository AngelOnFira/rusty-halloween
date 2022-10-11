use self::pack::{DrawPack, HeaderPack};
use crate::projector::pack::CheckSum;
use crate::proto_schema::schema::Projector;
use packed_struct::PackedStruct;
use rppal::spi::{Bus, SlaveSelect, Spi};

mod pack;

pub struct ProjectorController {
    pub spi: Spi,
}

impl ProjectorController {
    pub fn init() -> Result<Self, anyhow::Error> {
        // Set up SPI
        let spi = Spi::new(
            Bus::Spi0,
            SlaveSelect::Ss0,
            115_200,
            rppal::spi::Mode::Mode0,
        )?;

        Ok(ProjectorController { spi })
    }

    pub fn send(&mut self, projector_command: Projector) -> Result<(), anyhow::Error> {
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

        // Send the header
        self.spi.write(&header)?;

        // Send each draw instruction
        for draw_pack in draw_instructions {
            self.spi.write(&draw_pack)?;
        }

        Ok(())
    }
}
