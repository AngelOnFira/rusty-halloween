use packed_struct::{prelude::*, types::bits::Bits};

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
#[derive(PackedStruct, Default, Debug, PartialEq)]
#[packed_struct(bit_numbering = "msb0")]
pub struct HeaderPack {
    #[packed_field(bits = "0..=3")]
    projector_id: Integer<u8, Bits<4>>,
    #[packed_field(bits = "4..=11")]
    point_count: Integer<u8, Bits<8>>,
    #[packed_field(bits = "12")]
    home: bool,
    #[packed_field(bits = "13")]
    enable: bool,
    #[packed_field(bits = "14")]
    configuration_mode: bool,
    #[packed_field(bits = "15")]
    draw_boundary: bool,
    #[packed_field(bits = "16")]
    oneshot: bool,
    #[packed_field(bits = "17..=19")]
    speed_profile: Integer<u8, Bits<3>>,
    #[packed_field(bits = "20..=31")]
    _reserved: ReservedZero<packed_bits::Bits<11>>,
    #[packed_field(bits = "32")]
    checksum: bool,
}

// frame # | Bits & Definition
// Draw Mode:
// 1 -> n  | 0xFF800000 = X coordinate
//         | 0x007FC000 = Y coordinate
//         | 0x00003800 = Red 3-bit colour
//         | 0x00000700 = Green 3-bit colour
//         | 0x000000E0 = Blue 3-bit colour
//         | 0x00000001 = Checksum
#[derive(PackedStruct, Default, Debug, PartialEq)]
#[packed_struct(bit_numbering = "msb0")]
pub struct DrawPack {
    #[packed_field(bits = "0..=8", endian = "msb")]
    x: Integer<u16, Bits<9>>,
    #[packed_field(bits = "9..=17", endian = "msb")]
    y: Integer<u16, Bits<9>>,
    #[packed_field(bits = "18..=20")]
    red: Integer<u8, Bits<3>>,
    #[packed_field(bits = "21..=23")]
    green: Integer<u8, Bits<3>>,
    #[packed_field(bits = "24..=26")]
    blue: Integer<u8, Bits<3>>,
    #[packed_field(bits = "27..=31")]
    _reserved: ReservedZero<packed_bits::Bits<5>>,
    #[packed_field(bits = "32")]
    checksum: bool,
}

// frame # | Bits & Definition
// Configure Mode:
// 1       | 0xFFFFF000 = Acceleration (0 - 1048574)
//         | 0x00000FFE = Transfer Size (0 - 2046)
//         | 0x00000001 = Checksum
// 2       | 0xFFFFC000 = Max Speed (0 - 262142)
//         | 0x00003FFE = Min Speed (0 - 16382)
//         | 0x00000001 = Checksum
// 3       | 0xFFF00000 = X Home Pos (0 - 4094)
//         | 0x000FFF00 = Y Home Pos (0 - 4094)
//         | 0x000000F0 = Projector ID (0 - 14)
//         | 0x00000001 = Checksum
// 4 -> n  | 0x00000000

// Add tests here
#[cfg(test)]
mod tests {
    use super::*;
    use packed_struct::PackedStruct;

    #[test]
    fn test_header_pack_empty() -> Result<(), PackingError> {
        assert_eq!(
            [0x00, 0x00, 0x00, 0x00, 0x00],
            HeaderPack::default().pack()?
        );

        assert_eq!(
            [0x00, 0x00, 0x00, 0x00, 0x00],
            HeaderPack {
                projector_id: 0.into(),
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
            .pack()?
        );

        Ok(())
    }

    #[test]
    fn test_header_pack() -> Result<(), PackingError> {
        let header1 = HeaderPack {
            projector_id: 1.into(),
            point_count: 32.into(),
            home: true,
            enable: true,
            ..HeaderPack::default()
        };

        assert_eq!([0x12, 0x0c, 0x00, 0x00, 0x00], header1.pack()?);

        let header2 = HeaderPack {
            projector_id: 1.into(),
            point_count: 32.into(),
            home: true,
            enable: true,
            configuration_mode: true,
            draw_boundary: true,
            oneshot: true,
            speed_profile: 7.into(),
            ..HeaderPack::default()
        };

        assert_eq!([0x12, 0x0f, 0xF0, 0x00, 0x00], header2.pack()?);

        Ok(())
    }

    #[test]
    fn test_draw_pack_empty() -> Result<(), PackingError> {
        assert_eq!([0x00, 0x00, 0x00, 0x00, 0x00], DrawPack::default().pack()?);

        assert_eq!(
            [0x00, 0x00, 0x00, 0x00, 0x00],
            DrawPack {
                x: 0.into(),
                y: 0.into(),
                red: 0.into(),
                green: 0.into(),
                blue: 0.into(),
                checksum: false,
                ..Default::default()
            }
            .pack()?
        );

        Ok(())
    }

    #[test]
    fn test_draw_pack() -> Result<(), PackingError> {
        let draw1 = DrawPack {
            x: 122.into(),
            y: 92.into(),
            red: 7.into(),
            green: 0.into(),
            blue: 0.into(),
            ..DrawPack::default()
        };
        
        assert_eq!([0x3d, 0x17, 0x38, 0x00, 0x00], draw1.pack()?);

        Ok(())
    }
}
