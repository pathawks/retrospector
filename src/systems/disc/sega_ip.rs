// References:
//   Saturn IP.BIN layout:
//     https://segaretro.org/Sega_Saturn/Disc_format
//   Dreamcast IP.BIN layout:
//     https://mc.pp.se/dc/ip0000.bin.html
//     https://segaretro.org/Dreamcast/Disc_format

//! Sega IP.BIN parser for Saturn and Dreamcast discs
//! Both formats start at offset 0x0000 in user data of sector 0.
//! In raw sector images, user data is preceded by sector headers.

use super::sector::{detect_sector_format, sector_data_offset};

pub const SATURN_MAGIC: &[u8; 16] = b"SEGA SEGASATURN ";
pub const DREAMCAST_MAGIC: &[u8; 16] = b"SEGA SEGAKATANA ";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SegaDiscType {
    #[default]
    Saturn,
    Dreamcast,
}

#[derive(Debug, Clone, Default)]
pub struct SegaIpBin {
    pub disc_type: Option<SegaDiscType>,
    pub maker_id: String,
    pub product_number: String,
    pub version: String,
    pub release_date: String,
    pub device_info: String,
    pub area_codes: String,
    pub peripherals: String,
    pub title: String,
    // Dreamcast-only fields
    pub boot_filename: Option<String>,
    pub producer: Option<String>,
}

/// Find the byte offset where user data starts in sector 0 by detecting
/// the sector format and computing the data offset.
#[allow(clippy::arithmetic_side_effects)]
fn find_data_start(buffer: &[u8], magic: &[u8]) -> Option<usize> {
    let format = detect_sector_format(buffer);
    let base = sector_data_offset(0, format);
    if buffer.len() >= base + magic.len() && &buffer[base..base + magic.len()] == magic {
        Some(base)
    } else {
        None
    }
}

/// Check if buffer contains Saturn magic
pub fn is_saturn(buffer: &[u8]) -> bool {
    find_data_start(buffer, SATURN_MAGIC).is_some()
}

/// Check if buffer contains Dreamcast magic
pub fn is_dreamcast(buffer: &[u8]) -> bool {
    find_data_start(buffer, DREAMCAST_MAGIC).is_some()
}

/// Parse Saturn IP.BIN header
#[allow(clippy::arithmetic_side_effects)]
pub fn parse_saturn_ip(buffer: &[u8]) -> Option<SegaIpBin> {
    let base = find_data_start(buffer, SATURN_MAGIC)?;
    if buffer.len() < base + 0xD0 {
        return None;
    }

    Some(SegaIpBin {
        disc_type: Some(SegaDiscType::Saturn),
        maker_id: extract_string(buffer, base + 0x10, 16),
        product_number: extract_string(buffer, base + 0x20, 10),
        version: extract_string(buffer, base + 0x2A, 6),
        release_date: format_date(&extract_string(buffer, base + 0x30, 8)),
        device_info: extract_string(buffer, base + 0x38, 8),
        area_codes: extract_string(buffer, base + 0x40, 10),
        peripherals: extract_string(buffer, base + 0x50, 16),
        title: extract_string(buffer, base + 0x60, 112),
        boot_filename: None,
        producer: None,
    })
}

/// Parse Dreamcast IP.BIN header
#[allow(clippy::arithmetic_side_effects)]
pub fn parse_dreamcast_ip(buffer: &[u8]) -> Option<SegaIpBin> {
    let base = find_data_start(buffer, DREAMCAST_MAGIC)?;
    if buffer.len() < base + 0x100 {
        return None;
    }

    Some(SegaIpBin {
        disc_type: Some(SegaDiscType::Dreamcast),
        maker_id: extract_string(buffer, base + 0x10, 16),
        device_info: extract_string(buffer, base + 0x20, 16),
        area_codes: extract_string(buffer, base + 0x30, 8),
        peripherals: extract_string(buffer, base + 0x38, 8),
        product_number: extract_string(buffer, base + 0x40, 10),
        version: extract_string(buffer, base + 0x4A, 6),
        release_date: extract_string(buffer, base + 0x50, 16),
        boot_filename: Some(extract_string(buffer, base + 0x60, 16)),
        producer: Some(extract_string(buffer, base + 0x70, 16)),
        title: extract_string(buffer, base + 0x80, 128),
    })
}

/// Try to parse an 8-digit date string into YYYY-MM-DD.
/// Tries YYYYMMDD first; if the month is invalid, falls back to MMDDYYYY.
/// Returns the original string if neither works.
fn format_date(raw: &str) -> String {
    if raw.len() != 8 || !raw.chars().all(|c| c.is_ascii_digit()) {
        return raw.to_string();
    }

    // Try YYYYMMDD
    let mm = &raw[4..6];
    if let Ok(month) = mm.parse::<u32>()
        && (1..=12).contains(&month)
    {
        return format!("{}-{}-{}", &raw[0..4], mm, &raw[6..8]);
    }

    // Fall back to MMDDYYYY
    let mm = &raw[0..2];
    if let Ok(month) = mm.parse::<u32>()
        && (1..=12).contains(&month)
    {
        return format!("{}-{}-{}", &raw[4..8], mm, &raw[2..4]);
    }

    raw.to_string()
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

/// Map Saturn/Dreamcast area codes to a single canonical No-Intro region string.
/// Collapses multi-region codes: J+U+E → "World", single codes → their name.
/// Returns `None` when the combination is unrecognised.
pub fn dat_region_from_area_codes(area_codes: &str) -> Option<String> {
    let j = area_codes.contains('J');
    let u = area_codes.contains('U');
    let e = area_codes.contains('E');
    match (j, u, e) {
        (true, true, true) => Some("World".to_string()),
        (true, false, false) => Some("Japan".to_string()),
        (false, true, false) => Some("USA".to_string()),
        (false, false, true) => Some("Europe".to_string()),
        (true, true, false) => Some("Japan".to_string()), // NTSC multi → Japan primary
        (true, false, true) => Some("Japan".to_string()),
        (false, true, true) => Some("Europe".to_string()), // PAL multi → Europe primary
        _ => None,
    }
}

/// Decode Saturn area codes to region names
pub fn decode_saturn_regions(area_codes: &str) -> String {
    let mut regions = Vec::new();
    for c in area_codes.chars() {
        match c {
            'J' => regions.push("Japan"),
            'T' => regions.push("Taiwan/Korea"),
            'U' => regions.push("North America"),
            'B' => regions.push("Brazil"),
            'K' => regions.push("Korea"),
            'A' => regions.push("Asia (PAL)"),
            'E' => regions.push("Europe"),
            'L' => regions.push("Latin America"),
            _ => {}
        }
    }
    if regions.is_empty() {
        "Unknown".to_string()
    } else {
        regions.join(", ")
    }
}

/// Decode Dreamcast area symbols to region names
pub fn decode_dreamcast_regions(area_symbols: &str) -> String {
    let mut regions = Vec::new();
    for c in area_symbols.chars() {
        match c {
            'J' => regions.push("Japan"),
            'U' => regions.push("North America"),
            'E' => regions.push("Europe"),
            _ => {}
        }
    }
    if regions.is_empty() {
        "Unknown".to_string()
    } else {
        regions.join(", ")
    }
}

impl std::fmt::Display for SegaIpBin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Title: {}", self.title)?;
        writeln!(f, "Product Number: {}", self.product_number)?;

        // Region decoding
        let region_desc = match self.disc_type {
            Some(SegaDiscType::Saturn) => decode_saturn_regions(&self.area_codes),
            Some(SegaDiscType::Dreamcast) => decode_dreamcast_regions(&self.area_codes),
            None => self.area_codes.clone(),
        };
        writeln!(f, "Region: {}", region_desc)?;

        writeln!(f, "Version: {}", self.version)?;
        if !self.maker_id.is_empty() {
            writeln!(f, "Maker ID: {}", self.maker_id)?;
        }
        if !self.release_date.is_empty() {
            writeln!(f, "Release Date: {}", self.release_date)?;
        }

        if !self.device_info.is_empty() {
            writeln!(f, "Device Info: {}", self.device_info)?;
        }

        // Dreamcast-only fields
        if let Some(ref producer) = self.producer
            && !producer.is_empty()
        {
            writeln!(f, "Producer: {}", producer)?;
        }
        if let Some(ref boot_filename) = self.boot_filename
            && !boot_filename.is_empty()
        {
            writeln!(f, "Boot File: {}", boot_filename)?;
        }

        Ok(())
    }
}
