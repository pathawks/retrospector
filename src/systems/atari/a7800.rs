// References:
//   A78 header format:
//     https://7800.8bitdev.org/index.php/A78_Header_Specification

use crate::systems::helpers::{compute_sha1, non_empty};
use byteorder::{BigEndian, ByteOrder};

use crate::traits::{
    error::ParseError,
    rom_hash::RomHash,
    rominfo::{DatMeta, RomInfo},
    title::Title,
};

// Atari 7800 128-byte header layout (A78 format).
const A7800_HEADER_BYTES: usize = 128;
const A7800_MAGIC_START: usize = 0x01;
const A7800_MAGIC_END: usize = 0x0A;
const A7800_MAGIC: &[u8; 9] = b"ATARI7800";
const A7800_TITLE_START: usize = 0x11;
const A7800_TITLE_END: usize = 0x31;
const A7800_CART_TYPE_START: usize = 0x35;
const A7800_CART_TYPE_END: usize = 0x37;

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct Atari7800Info {
    pub title: String,
    pub cart_type: u16,
    pub rom_sha1: [u8; 20],
}

impl RomHash for Atari7800Info {
    fn sha1(&self) -> [u8; 20] {
        self.rom_sha1
    }
}

impl Title for Atari7800Info {
    fn title(&self) -> &str {
        &self.title
    }
}
impl RomInfo for Atari7800Info {
    fn console(&self) -> &'static str {
        "Atari 7800"
    }

    fn dat_meta(&self) -> DatMeta {
        DatMeta {
            title: non_empty(&self.title),
            ..DatMeta::default()
        }
    }
}

impl TryFrom<&[u8]> for Atari7800Info {
    type Error = ParseError;

    fn try_from(buffer: &[u8]) -> Result<Self, Self::Error> {
        if buffer.len() < A7800_HEADER_BYTES {
            return Err(ParseError::BufferTooSmall);
        }
        if &buffer[A7800_MAGIC_START..A7800_MAGIC_END] != A7800_MAGIC {
            return Err(ParseError::MagicNotFound);
        }

        // Calculate SHA1 of ROM data (after 128-byte header)
        let rom_sha1 = compute_sha1(&buffer[A7800_HEADER_BYTES..]);

        // Title is a 32-byte, space/null-padded field at 0x11.
        let title = String::from_utf8_lossy(&buffer[A7800_TITLE_START..A7800_TITLE_END])
            .trim_end_matches(['\0', ' '])
            .to_string();

        // Cart type is a 16-bit big-endian bitfield at 0x35; the A78 header
        // has no stored checksum, so we do not compute or compare one.
        let cart_type = BigEndian::read_u16(&buffer[A7800_CART_TYPE_START..A7800_CART_TYPE_END]);

        Ok(Atari7800Info {
            title,
            cart_type,
            rom_sha1,
        })
    }
}

impl std::fmt::Display for Atari7800Info {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self as &dyn Title)?;
        writeln!(f, "Cartridge Type: {:#06X}", self.cart_type)?;
        writeln!(f, "{}", self as &dyn RomHash)?;
        Ok(())
    }
}
