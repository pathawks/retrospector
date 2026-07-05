// References:
//   GameCube disc header and format:
//     https://wiibrew.org/wiki/Wii_disc#Header

use crate::systems::disc::nintendo_disc::{
    NintendoDiscHeader, NintendoDiscParseError, NintendoDiscType, parse_typed_nintendo_disc_header,
};
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
        self.header.dat_meta()
    }
}

/// Parse a GameCube disc by validating Nintendo optical-disc magics and header layout.
///
/// Research notes:
/// - GameCube detection uses the shared helper and only matches when the Wii
///   magic check fails and the GameCube magic at 0x1C is present.
/// - Header parsing requires the full 0x60-byte Nintendo header block.
fn parse_gamecube_disc(buffer: &[u8]) -> Result<GameCubeDisc, GameCubeParseError> {
    let header = parse_typed_nintendo_disc_header(buffer, NintendoDiscType::GameCube).map_err(
        |e| match e {
            NintendoDiscParseError::UnexpectedDiscType => GameCubeParseError::NotGameCubeDisc,
            NintendoDiscParseError::InvalidHeader => GameCubeParseError::InvalidNintendoHeader,
        },
    )?;
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
