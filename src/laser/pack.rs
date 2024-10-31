use packed_struct::{prelude::*, types::bits::Bits};

use crate::show::LaserDataFrame;

/// Trait to calculate checksum before packing the struct
pub trait CheckSum {
    fn calculate_checksum(&self, message: [u8; 4]) -> bool {
        let message = u32::from_le_bytes(message);

        let mut sum = message ^ (message >> 1);
        sum = sum ^ (sum >> 2);
        sum = sum ^ (sum >> 4);
        sum = sum ^ (sum >> 8);
        sum = sum ^ (sum >> 16);

        (sum & 1) == 1
    }

    fn checksum_pack(&mut self) -> [u8; 4];
}

// frame # | Bits & Definition
// Header Mode:
// 0       | 0xF0000000 = Projector ID
//         | 0x0FF00000 = Point Count
//         | 0x00080000 = Home
//         | 0x00040000 = Enable
//         | 0x00020000 = Configuration Mode
//         | 0x00010000 = Draw Boundary
//         | 0x00008000 = Oneshot
//         | 0x00007000 = Speed Profile
//         | 0x00000001 = Checksum
#[derive(PackedStruct, Default, Debug, PartialEq, Clone)]
#[packed_struct(bit_numbering = "msb0")]
pub struct HeaderPack {
    #[packed_field(bits = "0..=3")]
    pub laser_id: Integer<u8, Bits<4>>,
    #[packed_field(bits = "4..=11")]
    pub point_count: Integer<u8, Bits<8>>,
    #[packed_field(bits = "12")]
    pub home: bool,
    #[packed_field(bits = "13")]
    pub enable: bool,
    // Always false
    #[packed_field(bits = "14")]
    pub configuration_mode: bool,
    // Always false
    #[packed_field(bits = "15")]
    pub draw_boundary: bool,
    // Always false
    #[packed_field(bits = "16")]
    pub oneshot: bool,
    // Should be in
    #[packed_field(bits = "17..=19")]
    pub speed_profile: Integer<u8, Bits<3>>,
    #[packed_field(bits = "20..=30")]
    pub _reserved: ReservedZero<packed_bits::Bits<11>>,
    #[packed_field(bits = "31")]
    pub checksum: bool,
}

impl CheckSum for HeaderPack {
    fn checksum_pack(&mut self) -> [u8; 4] {
        self.checksum = self.calculate_checksum(self.pack().unwrap());
        let pack = self.pack().unwrap();

        pack
    }
}

// Pattern Selection:
// 1       | 0xFF000000 = Pattern ID - Selected from pattern lookup array
//         | 0x00FF8000 = Color Mask - 3-bit Red, 3-bit Green, 3-bit Blue (9-bit total)
//         | 0x00000001 = Checksum
#[derive(PackedStruct, Default, Debug, PartialEq, Clone)]
#[packed_struct(bit_numbering = "msb0")]
pub struct PatternPack {
    // TODO: Check this
    #[packed_field(bits = "0..=7")]
    pub pattern_id: Integer<u8, Bits<8>>,
    #[packed_field(bits = "8..=10")]
    pub red: Integer<u8, Bits<3>>,
    #[packed_field(bits = "11..=13")]
    pub green: Integer<u8, Bits<3>>,
    #[packed_field(bits = "14..=16")]
    pub blue: Integer<u8, Bits<3>>,
    #[packed_field(bits = "17..=30")]
    pub _reserved: ReservedZero<packed_bits::Bits<14>>,
    #[packed_field(bits = "31")]
    pub checksum: bool,
}

impl CheckSum for PatternPack {
    fn checksum_pack(&mut self) -> [u8; 4] {
        self.checksum = self.calculate_checksum(self.pack().unwrap());
        self.pack().unwrap()
    }
}

impl From<LaserDataFrame> for PatternPack {
    fn from(laser: LaserDataFrame) -> Self {
        PatternPack {
            pattern_id: laser.pattern_id.into(),
            red: laser.r.into(),
            green: laser.g.into(),
            blue: laser.b.into(),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_pack_empty() -> Result<(), PackingError> {
        assert_eq!(
            [0x00, 0x00, 0x00, 0x00],
            HeaderPack::default().checksum_pack()
        );

        assert_eq!(
            [0x00, 0x00, 0x00, 0x00],
            HeaderPack {
                laser_id: 0.into(),
                point_count: 0.into(),
                home: false,
                enable: false,
                configuration_mode: false,
                draw_boundary: false,
                oneshot: false,
                speed_profile: 0.into(),
                checksum: false,
                ..Default::default()
            }
            .checksum_pack()
        );

        Ok(())
    }

    #[test]
    fn test_header_pack() -> Result<(), PackingError> {
        assert_eq!(
            [0x12, 0x0c, 0x00, 0x00],
            HeaderPack {
                laser_id: 1.into(),
                point_count: 32.into(),
                home: true,
                enable: true,
                ..HeaderPack::default()
            }
            .checksum_pack()
        );

        assert_eq!(
            [0x12, 0x0f, 0xF0, 0x00],
            HeaderPack {
                laser_id: 1.into(),
                point_count: 32.into(),
                home: true,
                enable: true,
                configuration_mode: true,
                draw_boundary: true,
                oneshot: true,
                speed_profile: 7.into(),
                ..HeaderPack::default()
            }
            .checksum_pack()
        );

        Ok(())
    }

    #[test]
    fn test_header_pack_full_config() -> Result<(), PackingError> {
        assert_eq!(
            [0xf0, 0x00, 0x10, 0x01],
            HeaderPack {
                laser_id: 0xF.into(),
                speed_profile: 1.into(),
                ..HeaderPack::default()
            }
            .checksum_pack()
        );

        assert_eq!(
            [81, 143, 144, 0],
            HeaderPack {
                laser_id: 5.into(),
                point_count: 24.into(),
                home: true,
                enable: true,
                configuration_mode: true,
                draw_boundary: true,
                oneshot: true,
                speed_profile: 1.into(),
                ..HeaderPack::default()
            }
            .checksum_pack()
        );

        assert_eq!(
            [0x20, 0xd6, 0x10, 0x01],
            HeaderPack {
                laser_id: 2.into(),
                point_count: 13.into(),
                home: false,
                enable: true,
                configuration_mode: true,
                draw_boundary: false,
                oneshot: false,
                speed_profile: 1.into(),
                ..HeaderPack::default()
            }
            .checksum_pack()
        );

        assert_eq!(
            [0x22, 0xb6, 0x10, 0x00],
            HeaderPack {
                laser_id: 2.into(),
                point_count: 43.into(),
                home: false,
                enable: true,
                configuration_mode: true,
                draw_boundary: false,
                oneshot: false,
                speed_profile: 1.into(),
                ..HeaderPack::default()
            }
            .checksum_pack()
        );

        Ok(())
    }

    #[test]
    fn test_draw_pack_empty() -> Result<(), PackingError> {
        assert_eq!(
            [0x00, 0x00, 0x00, 0x00],
            PatternPack::default().checksum_pack()
        );

        assert_eq!(
            [0x00, 0x00, 0x00, 0x00],
            PatternPack {
                pattern_id: 0.into(),
                red: 0.into(),
                green: 0.into(),
                blue: 0.into(),
                checksum: false,
                ..Default::default()
            }
            .checksum_pack()
        );

        Ok(())
    }

    #[test]
    fn test_draw_pack() -> Result<(), PackingError> {
        assert_eq!(
            [0x3d, 0x17, 0x38, 0x00],
            PatternPack {
                pattern_id: 122.into(),
                red: 7.into(),
                green: 0.into(),
                blue: 0.into(),
                ..PatternPack::default()
            }
            .checksum_pack()
        );

        Ok(())
    }
}
