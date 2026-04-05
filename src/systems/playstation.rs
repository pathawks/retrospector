// References:
//   PlayStation disc identification via ISO 9660 PVD:
//     https://problemkaputt.de/psx-spx.htm#cdromfilesystemiso9660

use super::helpers::compute_sha1;
use crate::systems::disc::pvd::{PrimaryVolumeDescriptor, has_pvd, parse_pvd};
use crate::traits::error::ParseError;
use crate::traits::rom_hash::RomHash;
use crate::traits::rominfo::RomInfo;

#[derive(Debug, Clone, Default)]
pub struct PlaystationDisc {
    pvd: PrimaryVolumeDescriptor,
    rom_sha1: [u8; 20],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PlaystationParseError {
    MissingPrimaryVolumeDescriptor,
    InvalidPrimaryVolumeDescriptor,
    InvalidSystemId,
}

impl RomInfo for PlaystationDisc {
    fn console(&self) -> &'static str {
        "Sony PlayStation"
    }
}

impl RomHash for PlaystationDisc {
    fn sha1(&self) -> [u8; 20] {
        self.rom_sha1
    }
}

/// Parse a PlayStation disc by validating ISO9660 descriptors and system ID.
///
/// Research notes:
/// - A valid ISO9660 PVD must exist at sector 16 in the detected sector format.
/// - PlayStation discs identify themselves with a `PLAYSTATION*` system ID.
fn parse_playstation_disc(buffer: &[u8]) -> Result<PlaystationDisc, PlaystationParseError> {
    if !has_pvd(buffer) {
        return Err(PlaystationParseError::MissingPrimaryVolumeDescriptor);
    }

    let pvd = parse_pvd(buffer).ok_or(PlaystationParseError::InvalidPrimaryVolumeDescriptor)?;
    if !pvd.system_id.starts_with("PLAYSTATION") {
        return Err(PlaystationParseError::InvalidSystemId);
    }

    let rom_sha1 = compute_sha1(buffer);
    Ok(PlaystationDisc { pvd, rom_sha1 })
}

impl TryFrom<&[u8]> for PlaystationDisc {
    type Error = ParseError;

    fn try_from(buffer: &[u8]) -> Result<Self, Self::Error> {
        parse_playstation_disc(buffer).map_err(|e| match e {
            PlaystationParseError::MissingPrimaryVolumeDescriptor => ParseError::MagicNotFound,
            PlaystationParseError::InvalidPrimaryVolumeDescriptor
            | PlaystationParseError::InvalidSystemId => ParseError::InvalidHeader,
        })
    }
}

impl std::fmt::Display for PlaystationDisc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}", self.pvd)?;
        writeln!(f, "{}", self as &dyn RomHash)
    }
}
