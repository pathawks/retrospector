use crate::systems::disc::pvd::{PrimaryVolumeDescriptor, has_pvd, parse_pvd};
use crate::traits::error::ParseError;
use crate::traits::rominfo::RomInfo;

#[derive(Debug, Clone, Default)]
pub struct IsoImage {
    pvd: PrimaryVolumeDescriptor,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum IsoParseError {
    MissingPrimaryVolumeDescriptor,
    InvalidPrimaryVolumeDescriptor,
}

impl RomInfo for IsoImage {
    fn console(&self) -> &'static str {
        "ISO Image"
    }
}

/// Parse a generic ISO9660 image by validating and decoding the primary volume descriptor.
fn parse_iso_image(buffer: &[u8]) -> Result<IsoImage, IsoParseError> {
    if !has_pvd(buffer) {
        return Err(IsoParseError::MissingPrimaryVolumeDescriptor);
    }

    let pvd = parse_pvd(buffer).ok_or(IsoParseError::InvalidPrimaryVolumeDescriptor)?;
    Ok(IsoImage { pvd })
}

impl TryFrom<&[u8]> for IsoImage {
    type Error = ParseError;

    fn try_from(buffer: &[u8]) -> Result<Self, Self::Error> {
        parse_iso_image(buffer).map_err(|e| match e {
            IsoParseError::MissingPrimaryVolumeDescriptor => ParseError::MagicNotFound,
            IsoParseError::InvalidPrimaryVolumeDescriptor => ParseError::InvalidHeader,
        })
    }
}

impl std::fmt::Display for IsoImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Format: ISO 9660 CD-ROM image")?;
        write!(f, "{}", self.pvd)
    }
}
