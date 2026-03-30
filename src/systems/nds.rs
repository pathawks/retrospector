// References:
//   Nintendo DS cartridge header:
//     https://problemkaputt.de/gbatek.htm#dscartridgeheader

use byteorder::{ByteOrder, LittleEndian};

use crate::traits::error::ParseError;
use crate::traits::rominfo::{DatMeta, RomInfo};

use super::helpers::{dat_revision, nintendo_region_dat, non_empty};

// Nintendo DS header offsets used for lightweight authenticity checks.
const NDS_HEADER_MIN_BYTES: usize = 0x160;
const NDS_TITLE_START: usize = 0x00;
const NDS_TITLE_END: usize = 0x0C;
const NDS_GAME_CODE_START: usize = 0x0C;
const NDS_GAME_CODE_END: usize = 0x10;
const NDS_VERSION_OFFSET: usize = 0x01E;
const NDS_HEADER_SIZE_START: usize = 0x84;
const NDS_HEADER_SIZE_END: usize = 0x86;
const NDS_LOGO_CRC_START: usize = 0x15C;
const NDS_LOGO_CRC_END: usize = 0x15E;
const NDS_EXPECTED_LOGO_CRC: u16 = 0xCF56;
const NDS_EXPECTED_HEADER_SIZE: u16 = 0x4000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NdsParseError {
    BufferTooSmall { minimum: usize },
    InvalidLogoChecksum { found: u16 },
    InvalidHeaderSize { found: u16 },
    InvalidTitleEncoding,
    InvalidGameCodeEncoding,
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct NdsRomInfo {
    title: String,
    code: String,
    version: u8,
}

impl RomInfo for NdsRomInfo {
    fn console(&self) -> &'static str {
        "Nintendo DS"
    }

    fn dat_meta(&self) -> DatMeta {
        DatMeta {
            title: non_empty(&self.title),
            region: nintendo_region_dat(self.code.as_bytes().get(3).copied().unwrap_or(0)),
            version: dat_revision(self.version),
            serial: non_empty(&self.code),
            machine_id: non_empty(&self.code),
            ..DatMeta::default()
        }
    }
}

/// Parse the DS ROM header and return key identity fields.
///
/// Research notes:
/// - The Nintendo logo CRC at 0x15C..0x15D is a fast authenticity signal used
///   by hardware/boot ROM checks.
/// - Header size at 0x84..0x85 is expected to be 0x4000 for normal retail ROMs.
fn parse_nds_header(buffer: &[u8]) -> Result<NdsRomInfo, NdsParseError> {
    if buffer.len() < NDS_HEADER_MIN_BYTES {
        return Err(NdsParseError::BufferTooSmall {
            minimum: NDS_HEADER_MIN_BYTES,
        });
    }

    let logo_checksum = LittleEndian::read_u16(&buffer[NDS_LOGO_CRC_START..NDS_LOGO_CRC_END]);
    if logo_checksum != NDS_EXPECTED_LOGO_CRC {
        return Err(NdsParseError::InvalidLogoChecksum {
            found: logo_checksum,
        });
    }

    let header_size = LittleEndian::read_u16(&buffer[NDS_HEADER_SIZE_START..NDS_HEADER_SIZE_END]);
    if header_size != NDS_EXPECTED_HEADER_SIZE {
        return Err(NdsParseError::InvalidHeaderSize { found: header_size });
    }

    let title = String::from_utf8(buffer[NDS_TITLE_START..NDS_TITLE_END].trim_ascii().to_vec())
        .map_err(|_| NdsParseError::InvalidTitleEncoding)?;
    let code = String::from_utf8(buffer[NDS_GAME_CODE_START..NDS_GAME_CODE_END].to_vec())
        .map_err(|_| NdsParseError::InvalidGameCodeEncoding)?;

    Ok(NdsRomInfo {
        title,
        code,
        version: buffer[NDS_VERSION_OFFSET],
    })
}

impl TryFrom<&[u8]> for NdsRomInfo {
    type Error = ParseError;

    fn try_from(buffer: &[u8]) -> Result<Self, Self::Error> {
        parse_nds_header(buffer).map_err(|e| match e {
            NdsParseError::BufferTooSmall { .. } => ParseError::BufferTooSmall,
            NdsParseError::InvalidLogoChecksum { .. } | NdsParseError::InvalidHeaderSize { .. } => {
                ParseError::MagicNotFound
            }
            NdsParseError::InvalidTitleEncoding | NdsParseError::InvalidGameCodeEncoding => {
                ParseError::InvalidHeader
            }
        })
    }
}

impl std::fmt::Display for NdsRomInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Name: {}", self.title)?;
        writeln!(f, "Code: {}", self.code)?;
        writeln!(f, "Version: {}", self.version)?;
        Ok(())
    }
}
