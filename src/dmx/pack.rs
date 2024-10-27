use packed_struct::{prelude::*, types::bits::Bits};

// DMX Header Mode (8-bit Packet):
// Byte # | Bits      | Definition
// 0      | 0xF0      | Controller ID (Reserved addresses 0xA-0xE)
//        | 0x0F      | Universe Selector
#[derive(PackedStruct, Default, Debug, PartialEq, Clone)]
#[packed_struct(bit_numbering = "msb0", size = "1")]
pub struct DmxHeaderPack {
    #[packed_field(bits = "0..=3")]
    pub controller_id: Integer<u8, Bits<4>>,
    #[packed_field(bits = "4..=7")]
    pub universe: Integer<u8, Bits<4>>,
}

impl DmxHeaderPack {
    pub fn pack_header(&self) -> Result<[u8; 1], PackingError> {
        self.pack()
    }
}

// DMX Data (8-bit Packet):
// Byte #    | Bits      | Definition
// 1 -> 255  | 0xFF      | DMX Channel Data - Forward the DMX data as required by the channel
//
// Note: Writing again to the DMX controller requires addressing it again
// and sending another 255 bytes
#[derive(PackedStruct, Default, Debug, PartialEq, Clone)]
#[packed_struct(bit_numbering = "msb0", size = "1")]
pub struct DmxDataPack {
    #[packed_field(bits = "0..=7")]
    pub channel_data: Integer<u8, Bits<8>>,
}

impl DmxDataPack {
    pub fn pack_data(&self) -> Result<[u8; 1], PackingError> {
        self.pack()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dmx_header_pack() -> Result<(), PackingError> {
        // Test controller ID A (0xA), universe 0
        let header = DmxHeaderPack {
            controller_id: 0xA.into(),
            universe: 0.into(),
        };
        assert_eq!([0xA0], header.pack_header()?);

        // Test controller ID E (0xE), universe F (0xF)
        let header = DmxHeaderPack {
            controller_id: 0xE.into(),
            universe: 0xF.into(),
        };
        assert_eq!([0xEF], header.pack_header()?);

        Ok(())
    }

    #[test]
    fn test_dmx_data_pack() -> Result<(), PackingError> {
        // Test full off
        let data = DmxDataPack {
            channel_data: 0x00.into(),
        };
        assert_eq!([0x00], data.pack_data()?);

        // Test full on
        let data = DmxDataPack {
            channel_data: 0xFF.into(),
        };
        assert_eq!([0xFF], data.pack_data()?);

        // Test mid value
        let data = DmxDataPack {
            channel_data: 0x7F.into(),
        };
        assert_eq!([0x7F], data.pack_data()?);

        Ok(())
    }
}
