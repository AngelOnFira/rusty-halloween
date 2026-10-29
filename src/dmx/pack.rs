use packed_struct::{prelude::*, types::bits::Bits};

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
