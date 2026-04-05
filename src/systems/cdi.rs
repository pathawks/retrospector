// References:
//   CD-i disc identification via ISO 9660 PVD:
//     https://en.wikipedia.org/wiki/Philips_CD-i#Technical_specifications

use crate::systems::disc::pvd::{PrimaryVolumeDescriptor, parse_pvd};
use crate::traits::error::ParseError;
use crate::traits::rominfo::RomInfo;

#[derive(Debug, Clone, Default)]
pub struct CdiDisc {
    pvd: PrimaryVolumeDescriptor,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CdiParseError {
    InvalidPrimaryVolumeDescriptor,
    InvalidSystemId,
}

impl RomInfo for CdiDisc {
    fn console(&self) -> &'static str {
        "Philips CD-i"
    }
}

/// Parse a CD-i disc by validating ISO9660 PVD fields and CD-i system ID marker.
///
/// Research notes:
/// - CD-i discs still carry an ISO9660 primary volume descriptor.
/// - The system ID field is expected to start with `CD-I`.
fn parse_cdi_disc(buffer: &[u8]) -> Result<CdiDisc, CdiParseError> {
    let pvd = parse_pvd(buffer).ok_or(CdiParseError::InvalidPrimaryVolumeDescriptor)?;
    if !pvd.system_id.starts_with("CD-I") {
        return Err(CdiParseError::InvalidSystemId);
    }

    Ok(CdiDisc { pvd })
}

impl TryFrom<&[u8]> for CdiDisc {
    type Error = ParseError;

    fn try_from(buffer: &[u8]) -> Result<Self, Self::Error> {
        parse_cdi_disc(buffer).map_err(|e| match e {
            CdiParseError::InvalidPrimaryVolumeDescriptor => ParseError::MagicNotFound,
            CdiParseError::InvalidSystemId => ParseError::InvalidHeader,
        })
    }
}

impl std::fmt::Display for CdiDisc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.pvd)
    }
}
