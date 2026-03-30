// References:
//   GameCube disc header and format:
//     https://wiibrew.org/wiki/Wii_disc#Header

use crate::systems::disc::nintendo_disc::{
    dat_region, detect_nintendo_disc, parse_nintendo_disc_header, NintendoDiscHeader,
    NintendoDiscType,
};
use crate::systems::helpers::{dat_revision, non_empty};
use crate::traits::error::ParseError;
use crate::traits::rominfo::{DatMeta, RomInfo};
use crate::traits::title::Title;

#[derive(Debug, Clone, Default)]
pub struct GameCubeDisc {
    header: NintendoDiscHeader,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GameCubeParseError {
    NotGameCubeDisc,
    InvalidNintendoHeader,
}

impl RomInfo for GameCubeDisc {
    fn console(&self) -> &'static str {
        "Nintendo GameCube"
    }

    fn dat_meta(&self) -> DatMeta {
        // Normalize optional metadata via shared helpers to keep DAT output
        // behavior consistent across console modules.
        DatMeta {
            title: non_empty(&self.header.title),
            region: dat_region(self.header.region_code).map(String::from),
            version: dat_revision(self.header.version),
            serial: non_empty(&self.header.game_id),
            ..DatMeta::default()
        }
    }
}

/// Parse a GameCube disc by validating Nintendo optical-disc magics and header layout.
///
/// Research notes:
/// - GameCube detection uses the shared helper and only matches when the Wii
///   magic check fails and the GameCube magic at 0x1C is present.
/// - Header parsing requires the full 0x60-byte Nintendo header block.
fn parse_gamecube_disc(buffer: &[u8]) -> Result<GameCubeDisc, GameCubeParseError> {
    if detect_nintendo_disc(buffer) != Some(NintendoDiscType::GameCube) {
        return Err(GameCubeParseError::NotGameCubeDisc);
    }

    let header =
        parse_nintendo_disc_header(buffer).ok_or(GameCubeParseError::InvalidNintendoHeader)?;
    Ok(GameCubeDisc { header })
}

impl TryFrom<&[u8]> for GameCubeDisc {
    type Error = ParseError;

    fn try_from(buffer: &[u8]) -> Result<Self, Self::Error> {
        parse_gamecube_disc(buffer).map_err(|e| match e {
            GameCubeParseError::NotGameCubeDisc => ParseError::MagicNotFound,
            GameCubeParseError::InvalidNintendoHeader => ParseError::InvalidHeader,
        })
    }
}

impl Title for GameCubeDisc {
    fn title(&self) -> &str {
        &self.header.title
    }
}

impl std::fmt::Display for GameCubeDisc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.header)
    }
}
