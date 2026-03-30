// References:
//   iNES and NES 2.0 format identification:
//     https://www.nesdev.org/wiki/INES
//     https://www.nesdev.org/wiki/NES_2.0

use std::convert::TryFrom;
use std::default::Default;
use std::fmt;

use crate::traits::error::ParseError;

const HEADER_BYTES_MIN: usize = 16;
const MAGIC_START: usize = 0;
const MAGIC_END: usize = 4;
const NES_PADDING_START: usize = 12;
const NES_PADDING_END: usize = 16;
const NES_FORMAT_MASK: u8 = 0b1100;
const NES2_FORMAT_BITS: u8 = 0b1000;
const ARCHAIC_INES_BITS: u8 = 0b0100;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum FileType {
    #[default]
    INES,
    INES07,
    ArchaicINES,
    NES20,
    TNES,
    Raw,
}

impl fmt::Display for FileType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                FileType::ArchaicINES => "Archaic iNES",
                FileType::INES => "iNES",
                FileType::INES07 => "iNES 0.7",
                FileType::NES20 => "NES 2.0",
                FileType::TNES => "TNES",
                FileType::Raw => "Raw (no header)",
            }
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FileTypeParseError {
    BufferTooSmall { minimum: usize },
    UnknownMagic,
}

/// Classify a ROM file by inspecting the first 16 bytes.
///
/// Research notes:
/// - `TNES` is an alternate legacy container found in some pirate/flash-dump sets.
/// - `NES\x1A` is the iNES family signature.
/// - Byte 7 bits 2-3 are commonly used to distinguish NES 2.0 (`10b`) and
///   archaic iNES (`01b`) variants.
/// - Bytes 12..15 are expected to be zero for modern iNES; non-zero values are
///   treated here as iNES 0.7-style headers.
fn parse_file_type(buffer: &[u8]) -> Result<FileType, FileTypeParseError> {
    if buffer.len() < HEADER_BYTES_MIN {
        return Err(FileTypeParseError::BufferTooSmall {
            minimum: HEADER_BYTES_MIN,
        });
    }

    match &buffer[MAGIC_START..MAGIC_END] {
        b"TNES" => Ok(FileType::TNES),
        b"NES\x1A" => match (
            buffer[7] & NES_FORMAT_MASK,
            &buffer[NES_PADDING_START..NES_PADDING_END],
        ) {
            (NES2_FORMAT_BITS, _) => Ok(FileType::NES20),
            (ARCHAIC_INES_BITS, _) => Ok(FileType::ArchaicINES),
            (0b0000, [0, 0, 0, 0]) => Ok(FileType::INES),
            _ => Ok(FileType::INES07),
        },
        _ => Err(FileTypeParseError::UnknownMagic),
    }
}

impl TryFrom<&[u8]> for FileType {
    type Error = ParseError;

    fn try_from(buffer: &[u8]) -> Result<Self, Self::Error> {
        parse_file_type(buffer).map_err(|e| match e {
            FileTypeParseError::BufferTooSmall { .. } => ParseError::BufferTooSmall,
            FileTypeParseError::UnknownMagic => ParseError::MagicNotFound,
        })
    }
}
