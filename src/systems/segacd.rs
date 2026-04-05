// References:
//   Sega CD disc format and header:
//     https://segaretro.org/Sega_Mega-CD/Disc_format

use super::helpers::{compute_sha1, first_non_empty, non_empty};
use crate::systems::disc::decode_regions;
use crate::systems::disc::pvd::{PrimaryVolumeDescriptor, parse_pvd};
use crate::systems::disc::sector::{
    SECTOR_RAW, SectorFormat, detect_sector_format, logical_to_physical, sector_data_offset,
};
use crate::systems::disc::sega_ip::dat_region_from_area_codes;
use crate::systems::genesis::publisher_from_copyright;
use crate::traits::error::ParseError;
use crate::traits::rom_hash::RomHash;
use crate::traits::rominfo::{DatMeta, RomInfo};
use crate::traits::title::Title;

const MAGIC: &[u8] = b"SEGADISCSYSTEM";
const MAGIC_SEARCH_LIMIT: usize = 0x8000;
const GENESIS_HEADER_LOGICAL_OFFSET: usize = 0x100;
const MIN_HEADER_BYTES: usize = 0x200;

const HARDWARE_ID_OFFSET: usize = 0x00;
const HARDWARE_ID_LEN: usize = 16;
const COPYRIGHT_OFFSET: usize = 0x10;
const COPYRIGHT_LEN: usize = 16;
const DOMESTIC_TITLE_OFFSET: usize = 0x20;
const DOMESTIC_TITLE_LEN: usize = 48;
const OVERSEAS_TITLE_OFFSET: usize = 0x50;
const OVERSEAS_TITLE_LEN: usize = 48;
const PRODUCT_CODE_OFFSET: usize = 0x80;
const PRODUCT_CODE_LEN: usize = 14;
const REGIONS_OFFSET: usize = 0xF0;
const REGIONS_LEN: usize = 16;

#[derive(Debug, Clone, Default)]
pub struct SegaCdDisc {
    pub domestic_title: String,
    pub overseas_title: String,
    pub copyright: String,
    pub product_code: String,
    pub regions: String,
    pub hardware_id: String,
    pub rom_sha1: [u8; 20],
    pub pvd: Option<PrimaryVolumeDescriptor>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SegaCdParseError {
    MissingSegaCdMagic,
}

impl RomInfo for SegaCdDisc {
    fn console(&self) -> &'static str {
        "Sega CD/Mega CD"
    }

    fn dat_meta(&self) -> DatMeta {
        DatMeta {
            title: first_non_empty(&[&self.overseas_title, &self.domestic_title]),
            region: dat_region_from_area_codes(&self.regions),
            serial: non_empty(&self.product_code),
            ..DatMeta::default()
        }
    }
}

impl RomHash for SegaCdDisc {
    fn sha1(&self) -> [u8; 20] {
        self.rom_sha1
    }
}

/// Parse Sega CD metadata from the `SEGADISCSYSTEM` boot marker and header fields.
///
/// Research notes:
/// - A valid Sega CD image should contain `SEGADISCSYSTEM` near sector 0 user data.
/// - Header fields are read from the Genesis-style header mapped at logical 0x100.
#[allow(clippy::arithmetic_side_effects)]
fn parse_sega_cd_disc(buffer: &[u8]) -> Result<SegaCdDisc, SegaCdParseError> {
    let (magic_offset, format) =
        find_header_base(buffer).ok_or(SegaCdParseError::MissingSegaCdMagic)?;
    let rom_sha1 = compute_sha1(buffer);

    // Genesis-style header is at offset 0x100 from disc start
    // In raw sector format, we need to account for sector headers
    let header_offset = calculate_header_offset(format, magic_offset, buffer);

    if buffer.len() < header_offset + MIN_HEADER_BYTES {
        // We found the magic but can't read the full header
        return Ok(SegaCdDisc {
            rom_sha1,
            ..Default::default()
        });
    }

    let header = &buffer[header_offset..];
    let pvd = parse_pvd(buffer);

    Ok(SegaCdDisc {
        hardware_id: extract_string(header, HARDWARE_ID_OFFSET, HARDWARE_ID_LEN),
        copyright: extract_string(header, COPYRIGHT_OFFSET, COPYRIGHT_LEN),
        domestic_title: extract_string(header, DOMESTIC_TITLE_OFFSET, DOMESTIC_TITLE_LEN),
        overseas_title: extract_string(header, OVERSEAS_TITLE_OFFSET, OVERSEAS_TITLE_LEN),
        product_code: extract_string(header, PRODUCT_CODE_OFFSET, PRODUCT_CODE_LEN),
        regions: extract_string(header, REGIONS_OFFSET, REGIONS_LEN).to_uppercase(),
        rom_sha1,
        pvd,
    })
}

impl TryFrom<&[u8]> for SegaCdDisc {
    type Error = ParseError;

    fn try_from(buffer: &[u8]) -> Result<Self, Self::Error> {
        parse_sega_cd_disc(buffer).map_err(|_| ParseError::MagicNotFound)
    }
}

/// Find where the SEGADISCSYSTEM magic is located
#[allow(clippy::arithmetic_side_effects)]
fn find_header_base(buffer: &[u8]) -> Option<(usize, SectorFormat)> {
    // Detect sector format and check the expected location
    let format = detect_sector_format(buffer);
    let base = sector_data_offset(0, format);
    if buffer.len() >= base + MAGIC.len() && &buffer[base..base + MAGIC.len()] == MAGIC {
        return Some((base, format));
    }

    // Fallback: search for magic in unusual formats
    let limit = buffer.len().min(MAGIC_SEARCH_LIMIT);
    let offset = buffer[..limit]
        .windows(MAGIC.len())
        .position(|window| window == MAGIC)?;
    Some((offset, format))
}

/// Calculate where the Genesis-style header should be.
/// The header is at logical offset 0x100 from disc start.
#[allow(clippy::arithmetic_side_effects)]
fn calculate_header_offset(format: SectorFormat, magic_offset: usize, buffer: &[u8]) -> usize {
    // Use centralized logical-to-physical translation
    let result = logical_to_physical(GENESIS_HEADER_LOGICAL_OFFSET, format);

    // If the magic was found at an unexpected offset (fallback search),
    // verify by checking stride, then adjust
    let expected_base = sector_data_offset(0, format);
    if magic_offset != expected_base {
        let test_offset = magic_offset + SECTOR_RAW;
        if buffer.len() > test_offset + 4
            && buffer[test_offset..test_offset + 4] == buffer[magic_offset..magic_offset + 4]
        {
            // Looks like raw sectors at an unusual base
            return magic_offset + result - expected_base;
        }
        return magic_offset + GENESIS_HEADER_LOGICAL_OFFSET;
    }

    result
}

#[allow(clippy::arithmetic_side_effects)]
fn extract_string(buffer: &[u8], offset: usize, len: usize) -> String {
    if buffer.len() < offset + len {
        return String::new();
    }
    String::from_utf8_lossy(&buffer[offset..offset + len])
        .trim()
        .to_string()
}

impl Title for SegaCdDisc {
    fn title(&self) -> &str {
        if !self.overseas_title.is_empty() {
            &self.overseas_title
        } else {
            &self.domestic_title
        }
    }
}

impl std::fmt::Display for SegaCdDisc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if !self.domestic_title.is_empty() {
            writeln!(f, "Domestic Title: {}", self.domestic_title)?;
        }
        if !self.overseas_title.is_empty() && self.overseas_title != self.domestic_title {
            writeln!(f, "Overseas Title: {}", self.overseas_title)?;
        }
        if !self.product_code.is_empty() {
            writeln!(f, "Product Code: {}", self.product_code)?;
        }
        if !self.regions.is_empty() {
            writeln!(f, "Region: {}", decode_regions(&self.regions))?;
        }
        if !self.copyright.is_empty() {
            match publisher_from_copyright(&self.copyright) {
                Some(name) => writeln!(f, "Copyright: {} ({})", self.copyright, name)?,
                None => writeln!(f, "Copyright: {}", self.copyright)?,
            }
        }
        if let Some(pvd) = &self.pvd {
            write!(f, "{pvd}")?;
        }
        writeln!(f, "{}", self as &dyn RomHash)?;
        Ok(())
    }
}
