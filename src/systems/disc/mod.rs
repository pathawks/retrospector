pub mod nintendo_disc;
pub mod pvd;
pub mod sector;
pub mod sega_ip;

use super::cdi::CdiDisc;
use super::dreamcast::DreamcastDisc;
use super::helpers::{compute_sha1, non_empty};
use super::iso::IsoImage;
use super::playstation::PlaystationDisc;
use super::saturn::SaturnDisc;
use super::segacd::SegaCdDisc;
use crate::output::cue::format_sha1;
use crate::traits::rom_hash::RomHash;
use crate::traits::rominfo::RomInfo;
use crate::traits::title::Title;

/// Analysis data for a disc extracted by retrospector
#[derive(Debug, Clone, Default)]
pub struct DiscAnalysis {
    pub console: Option<String>,
    pub title: Option<String>,
    pub region: Option<String>,
    pub product_code: Option<String>,
    pub disc_sha1: Option<String>,
}

/// Detect disc type and extract analysis information from buffer
pub fn detect_disc(buffer: &[u8]) -> DiscAnalysis {
    // Calculate SHA1 of entire buffer for disc types that don't store it
    let calculate_sha1 = || format_sha1(&compute_sha1(buffer));

    // Sega CD
    if let Ok(disc) = SegaCdDisc::try_from(buffer) {
        return DiscAnalysis {
            console: Some(disc.console().to_string()),
            title: Some(disc.title().to_string()),
            // Normalize optional fields through shared helpers while preserving
            // existing empty-string semantics for output surfaces.
            region: non_empty(&disc.regions).map(|regions| decode_regions(&regions).to_string()),
            product_code: non_empty(&disc.product_code),
            disc_sha1: Some(format_sha1(&disc.sha1())),
        };
    }

    // Saturn
    if let Ok(disc) = SaturnDisc::try_from(buffer) {
        return DiscAnalysis {
            console: Some(disc.console().to_string()),
            title: Some(disc.title().to_string()),
            region: None,
            product_code: None,
            disc_sha1: Some(format_sha1(&disc.sha1())),
        };
    }

    // Dreamcast
    if let Ok(disc) = DreamcastDisc::try_from(buffer) {
        return DiscAnalysis {
            console: Some(disc.console().to_string()),
            title: Some(disc.title().to_string()),
            region: None,
            product_code: None,
            disc_sha1: Some(format_sha1(&disc.sha1())),
        };
    }

    // PlayStation
    if let Ok(disc) = PlaystationDisc::try_from(buffer) {
        return DiscAnalysis {
            console: Some(disc.console().to_string()),
            title: None,
            region: None,
            product_code: None,
            disc_sha1: Some(format_sha1(&disc.sha1())),
        };
    }

    // CDi (doesn't implement RomHash, calculate manually)
    if let Ok(disc) = CdiDisc::try_from(buffer) {
        return DiscAnalysis {
            console: Some(disc.console().to_string()),
            title: None,
            region: None,
            product_code: None,
            disc_sha1: Some(calculate_sha1()),
        };
    }

    // Generic ISO (doesn't implement RomHash, calculate manually)
    if let Ok(disc) = IsoImage::try_from(buffer) {
        return DiscAnalysis {
            console: Some(disc.console().to_string()),
            title: None,
            region: None,
            product_code: None,
            disc_sha1: Some(calculate_sha1()),
        };
    }

    DiscAnalysis::default()
}

/// Decode region codes to human-readable string
pub fn decode_regions(regions: &str) -> &'static str {
    let has_j = regions.contains('J');
    let has_u = regions.contains('U');
    let has_e = regions.contains('E');

    match (has_j, has_u, has_e) {
        (true, true, true) => "Japan, USA, Europe",
        (true, true, false) => "Japan, USA",
        (true, false, true) => "Japan, Europe",
        (false, true, true) => "USA, Europe",
        (true, false, false) => "Japan",
        (false, true, false) => "USA",
        (false, false, true) => "Europe",
        _ => "Unknown",
    }
}
