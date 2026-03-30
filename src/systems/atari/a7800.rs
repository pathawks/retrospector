// References:
//   A78 header format:
//     https://7800.8bitdev.org/index.php/A78_Header_Specification

use crate::systems::helpers::{compute_sha1, non_empty};
use byteorder::{BigEndian, ByteOrder, LittleEndian};

use crate::traits::{
    error::ParseError,
    rom_hash::RomHash,
    rominfo::{DatMeta, RomInfo},
    stored_checksum::StoredChecksum,
    title::Title,
};

// Atari 7800 128-byte header layout (A78 format).
const A7800_HEADER_BYTES: usize = 128;
const A7800_HEADER_WORDS: usize = A7800_HEADER_BYTES / 4;
const A7800_MAGIC_START: usize = 0x01;
const A7800_MAGIC_END: usize = 0x0A;
const A7800_MAGIC: &[u8; 9] = b"ATARI7800";
const A7800_CHECKSUM_START: usize = 0x1C;
const A7800_CHECKSUM_END: usize = 0x20;
const A7800_TITLE_START: usize = 0x11;
const A7800_TITLE_END: usize = 0x31;
const A7800_CART_TYPE_OFFSET: usize = 0x18;

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct Atari7800Info {
    pub title: String,
    pub stored_checksum: u32,
    pub calculated_checksum: u32,
    pub cart_type: u8,
    pub rom_sha1: [u8; 20],
}

impl StoredChecksum<u32> for Atari7800Info {
    fn stored_checksum(&self) -> u32 {
        self.stored_checksum
    }

    fn calculated_checksum(&self) -> u32 {
        self.calculated_checksum
    }
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

        // Read stored checksum from header at offset 0x1C to 0x1F
        let stored_checksum =
            BigEndian::read_u32(&buffer[A7800_CHECKSUM_START..A7800_CHECKSUM_END]);

        // Calculate checksum of the entire file excluding the header (from offset 128)
        let calculated_checksum = buffer
            .chunks_exact(4)
            .skip(A7800_HEADER_WORDS)
            .map(LittleEndian::read_u32)
            .fold(0u32, |sum, b| sum.wrapping_add(b));

        // Extract additional header information
        let title = String::from_utf8_lossy(&buffer[A7800_TITLE_START..A7800_TITLE_END])
            .trim_end_matches('\0')
            .to_string();
        let cart_type = buffer[A7800_CART_TYPE_OFFSET];

        Ok(Atari7800Info {
            title,
            stored_checksum,
            calculated_checksum,
            cart_type,
            rom_sha1,
        })
    }
}

impl std::fmt::Display for Atari7800Info {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self as &dyn Title)?;
        writeln!(f, "Cartridge Type: {:#X}", self.cart_type)?;
        writeln!(f, "{}", self as &dyn StoredChecksum<u32>)?;
        writeln!(f, "{}", self as &dyn RomHash)?;
        Ok(())
    }
}
