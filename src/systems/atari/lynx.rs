// References:
//   Atari Lynx LNX file format header:
//     https://atarilynxdeveloper.wordpress.com/documentation/file-formats/

use crate::systems::helpers::{compute_sha1, non_empty};
use byteorder::{ByteOrder, LittleEndian};

use crate::traits::{
    error::ParseError,
    rom_hash::RomHash,
    rominfo::{DatMeta, RomInfo},
    title::Title,
};

// Atari Lynx 64-byte header layout (LNX format).
const LYNX_HEADER_BYTES: usize = 64;
const LYNX_MAGIC_START: usize = 0x00;
const LYNX_MAGIC_END: usize = 0x04;
const LYNX_MAGIC: &[u8; 4] = b"LYNX";
const LYNX_BANK0_PAGE_SIZE_START: usize = 0x04;
const LYNX_BANK1_PAGE_SIZE_START: usize = 0x06;
const LYNX_VERSION_START: usize = 0x08;
const LYNX_TITLE_START: usize = 0x0A;
const LYNX_TITLE_END: usize = 0x2A;
const LYNX_MANUFACTURER_START: usize = 0x2A;
const LYNX_MANUFACTURER_END: usize = 0x3A;
const LYNX_ROTATION_OFFSET: usize = 0x3A;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum LynxRotation {
    #[default]
    None,
    Left,
    Right,
    Unknown,
}

impl From<u8> for LynxRotation {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::None,
            1 => Self::Left,
            2 => Self::Right,
            _ => Self::Unknown,
        }
    }
}

impl std::fmt::Display for LynxRotation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "None"),
            Self::Left => write!(f, "Left"),
            Self::Right => write!(f, "Right"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct AtariLynxInfo {
    pub title: String,
    pub manufacturer: String,
    pub version: u16,
    pub rotation: LynxRotation,
    pub bank0_page_size: u16,
    pub bank1_page_size: u16,
    pub rom_sha1: [u8; 20],
}

impl RomHash for AtariLynxInfo {
    fn sha1(&self) -> [u8; 20] {
        self.rom_sha1
    }
}

impl Title for AtariLynxInfo {
    fn title(&self) -> &str {
        &self.title
    }
}
impl RomInfo for AtariLynxInfo {
    fn console(&self) -> &'static str {
        "Atari Lynx"
    }

    fn dat_meta(&self) -> DatMeta {
        DatMeta {
            title: non_empty(&self.title),
            manufacturer: non_empty(&self.manufacturer),
            ..DatMeta::default()
        }
    }
}

impl TryFrom<&[u8]> for AtariLynxInfo {
    type Error = ParseError;

    fn try_from(buffer: &[u8]) -> Result<Self, Self::Error> {
        if buffer.len() < LYNX_HEADER_BYTES {
            return Err(ParseError::BufferTooSmall);
        }
        if &buffer[LYNX_MAGIC_START..LYNX_MAGIC_END] != LYNX_MAGIC {
            return Err(ParseError::MagicNotFound);
        }

        // Calculate SHA1 of ROM data (after 64-byte header)
        let rom_sha1 = compute_sha1(&buffer[LYNX_HEADER_BYTES..]);

        let bank0_page_size =
            LittleEndian::read_u16(&buffer[LYNX_BANK0_PAGE_SIZE_START..LYNX_BANK1_PAGE_SIZE_START]);
        let bank1_page_size =
            LittleEndian::read_u16(&buffer[LYNX_BANK1_PAGE_SIZE_START..LYNX_VERSION_START]);
        let version = LittleEndian::read_u16(&buffer[LYNX_VERSION_START..LYNX_TITLE_START]);
        let rotation = LynxRotation::from(buffer[LYNX_ROTATION_OFFSET]);

        let title = String::from_utf8_lossy(&buffer[LYNX_TITLE_START..LYNX_TITLE_END])
            .trim_end_matches('\0')
            .to_string();

        let manufacturer =
            String::from_utf8_lossy(&buffer[LYNX_MANUFACTURER_START..LYNX_MANUFACTURER_END])
                .trim_end_matches('\0')
                .to_string();

        Ok(AtariLynxInfo {
            title,
            manufacturer,
            version,
            rotation,
            bank0_page_size,
            bank1_page_size,
            rom_sha1,
        })
    }
}

impl std::fmt::Display for AtariLynxInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self as &dyn Title)?;
        writeln!(f, "Screen Rotation: {}", self.rotation)?;
        if !self.manufacturer.is_empty() {
            writeln!(f, "Manufacturer: {}", self.manufacturer)?;
        }
        writeln!(f, "Header Version: {}", self.version)?;
        writeln!(f, "{}", self as &dyn RomHash)?;
        Ok(())
    }
}
