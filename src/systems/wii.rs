// References:
//   Wii disc header and format:
//     https://wiibrew.org/wiki/Wii_disc#Header

use crate::systems::disc::nintendo_disc::{
    NintendoDiscHeader, NintendoDiscParseError, NintendoDiscType, parse_typed_nintendo_disc_header,
};
use crate::traits::error::ParseError;
use crate::traits::rominfo::{DatMeta, RomInfo};
use crate::traits::title::Title;

#[derive(Debug, Clone, Default)]
pub struct WiiDisc {
    header: NintendoDiscHeader,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WiiParseError {
    NotWiiDisc,
    InvalidNintendoHeader,
}

impl RomInfo for WiiDisc {
    fn console(&self) -> &'static str {
        "Nintendo Wii"
    }

    fn dat_meta(&self) -> DatMeta {
        self.header.dat_meta()
    }
}

/// Parse a Wii disc by validating Nintendo optical-disc magics and header layout.
///
/// Research notes:
/// - Wii detection uses the shared Nintendo disc helper, which prioritizes the
///   Wii magic at 0x18 before checking the GameCube magic at 0x1C.
/// - Header parsing requires the full 0x60-byte Nintendo header block.
fn parse_wii_disc(buffer: &[u8]) -> Result<WiiDisc, WiiParseError> {
    let header =
        parse_typed_nintendo_disc_header(buffer, NintendoDiscType::Wii).map_err(|e| match e {
            NintendoDiscParseError::UnexpectedDiscType => WiiParseError::NotWiiDisc,
            NintendoDiscParseError::InvalidHeader => WiiParseError::InvalidNintendoHeader,
        })?;
    Ok(WiiDisc { header })
}

impl TryFrom<&[u8]> for WiiDisc {
    type Error = ParseError;

    fn try_from(buffer: &[u8]) -> Result<Self, Self::Error> {
        parse_wii_disc(buffer).map_err(|e| match e {
            WiiParseError::NotWiiDisc => ParseError::MagicNotFound,
            WiiParseError::InvalidNintendoHeader => ParseError::InvalidHeader,
        })
    }
}

impl Title for WiiDisc {
    fn title(&self) -> &str {
        &self.header.title
    }
}

impl std::fmt::Display for WiiDisc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.header)
    }
}
