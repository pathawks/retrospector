// References:
//   Dreamcast disc header (IP.BIN):
//     https://mc.pp.se/dc/ip0000.bin.html
//     https://segaretro.org/Dreamcast/Disc_format

use super::helpers::{compute_sha1, non_empty};
use crate::systems::disc::pvd::{parse_pvd, PrimaryVolumeDescriptor};
use crate::systems::disc::sega_ip::{
    dat_region_from_area_codes, is_dreamcast, parse_dreamcast_ip, SegaIpBin,
};
use crate::traits::error::ParseError;
use crate::traits::rom_hash::RomHash;
use crate::traits::rominfo::{DatMeta, RomInfo};
use crate::traits::title::Title;

#[derive(Debug, Clone, Default)]
pub struct DreamcastDisc {
    ip_bin: SegaIpBin,
    rom_sha1: [u8; 20],
    pvd: Option<PrimaryVolumeDescriptor>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DreamcastParseError {
    MissingDreamcastMagic,
    InvalidDreamcastIpHeader,
}

impl RomInfo for DreamcastDisc {
    fn console(&self) -> &'static str {
        "Sega Dreamcast"
    }

    fn dat_meta(&self) -> DatMeta {
        // Dreamcast stores the producer name in ip_bin.producer
        let manufacturer = self.ip_bin.producer.as_deref().and_then(non_empty);
        DatMeta {
            title: non_empty(&self.ip_bin.title),
            region: dat_region_from_area_codes(&self.ip_bin.area_codes),
            version: non_empty(&self.ip_bin.version),
            date: non_empty(&self.ip_bin.release_date),
            manufacturer,
            serial: non_empty(&self.ip_bin.product_number),
            machine_id: None,
        }
    }
}

impl RomHash for DreamcastDisc {
    fn sha1(&self) -> [u8; 20] {
        self.rom_sha1
    }
}

/// Parse Dreamcast metadata from sector-0 IP data and optional ISO9660 descriptors.
///
/// Research notes:
/// - `is_dreamcast` verifies the Dreamcast magic (`SEGA SEGAKATANA `) in sector
///   0 user data across raw/cooked sector formats.
/// - `parse_dreamcast_ip` then validates that enough bytes exist for the
///   Dreamcast IP.BIN field layout.
fn parse_dreamcast_disc(buffer: &[u8]) -> Result<DreamcastDisc, DreamcastParseError> {
    if !is_dreamcast(buffer) {
        return Err(DreamcastParseError::MissingDreamcastMagic);
    }

    let ip_bin = parse_dreamcast_ip(buffer).ok_or(DreamcastParseError::InvalidDreamcastIpHeader)?;
    let rom_sha1 = compute_sha1(buffer);
    let pvd = parse_pvd(buffer);

    Ok(DreamcastDisc {
        ip_bin,
        rom_sha1,
        pvd,
    })
}

impl TryFrom<&[u8]> for DreamcastDisc {
    type Error = ParseError;

    fn try_from(buffer: &[u8]) -> Result<Self, Self::Error> {
        parse_dreamcast_disc(buffer).map_err(|e| match e {
            DreamcastParseError::MissingDreamcastMagic => ParseError::MagicNotFound,
            DreamcastParseError::InvalidDreamcastIpHeader => ParseError::InvalidHeader,
        })
    }
}

impl Title for DreamcastDisc {
    fn title(&self) -> &str {
        &self.ip_bin.title
    }
}

impl std::fmt::Display for DreamcastDisc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}", self.ip_bin)?;
        if let Some(pvd) = &self.pvd {
            write!(f, "{pvd}")?;
        }
        writeln!(f, "{}", self as &dyn RomHash)
    }
}
