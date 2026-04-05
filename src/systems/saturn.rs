// References:
//   Sega Saturn disc header (IP.BIN):
//     https://segaretro.org/Sega_Saturn/Disc_format

use super::helpers::{compute_sha1, non_empty};
use crate::systems::disc::pvd::{PrimaryVolumeDescriptor, parse_pvd};
use crate::systems::disc::sega_ip::{
    SegaIpBin, dat_region_from_area_codes, is_saturn, parse_saturn_ip,
};
use crate::traits::error::ParseError;
use crate::traits::rom_hash::RomHash;
use crate::traits::rominfo::{DatMeta, RomInfo};
use crate::traits::title::Title;

#[derive(Debug, Clone, Default)]
pub struct SaturnDisc {
    ip_bin: SegaIpBin,
    rom_sha1: [u8; 20],
    pvd: Option<PrimaryVolumeDescriptor>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SaturnParseError {
    MissingSaturnMagic,
    InvalidSaturnIpHeader,
}

impl RomInfo for SaturnDisc {
    fn console(&self) -> &'static str {
        "Sega Saturn"
    }

    fn dat_meta(&self) -> DatMeta {
        DatMeta {
            title: non_empty(&self.ip_bin.title),
            region: dat_region_from_area_codes(&self.ip_bin.area_codes),
            version: non_empty(&self.ip_bin.version),
            date: non_empty(&self.ip_bin.release_date),
            manufacturer: non_empty(&self.ip_bin.maker_id),
            serial: non_empty(&self.ip_bin.product_number),
            machine_id: None,
        }
    }
}

impl RomHash for SaturnDisc {
    fn sha1(&self) -> [u8; 20] {
        self.rom_sha1
    }
}

/// Parse Saturn metadata from sector-0 IP data and optional ISO9660 descriptors.
///
/// Research notes:
/// - `is_saturn` verifies the Saturn magic (`SEGA SEGASATURN `) in sector 0
///   user data across raw/cooked sector formats.
/// - `parse_saturn_ip` then validates that enough bytes exist for the expected
///   Saturn IP.BIN field layout.
fn parse_saturn_disc(buffer: &[u8]) -> Result<SaturnDisc, SaturnParseError> {
    if !is_saturn(buffer) {
        return Err(SaturnParseError::MissingSaturnMagic);
    }

    let ip_bin = parse_saturn_ip(buffer).ok_or(SaturnParseError::InvalidSaturnIpHeader)?;
    let rom_sha1 = compute_sha1(buffer);
    let pvd = parse_pvd(buffer);

    Ok(SaturnDisc {
        ip_bin,
        rom_sha1,
        pvd,
    })
}

impl TryFrom<&[u8]> for SaturnDisc {
    type Error = ParseError;

    fn try_from(buffer: &[u8]) -> Result<Self, Self::Error> {
        parse_saturn_disc(buffer).map_err(|e| match e {
            SaturnParseError::MissingSaturnMagic => ParseError::MagicNotFound,
            SaturnParseError::InvalidSaturnIpHeader => ParseError::InvalidHeader,
        })
    }
}

impl Title for SaturnDisc {
    fn title(&self) -> &str {
        &self.ip_bin.title
    }
}

impl std::fmt::Display for SaturnDisc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.ip_bin)?;
        if let Some(pvd) = &self.pvd {
            write!(f, "{pvd}")?;
        }
        writeln!(f, "{}", self as &dyn RomHash)
    }
}
