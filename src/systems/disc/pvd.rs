// References:
//   ISO 9660 / ECMA-119 specification (Primary Volume Descriptor):
//     https://www.ecma-international.org/publications-and-standards/standards/ecma-119/
//   ISO 9660 overview and PVD field layout:
//     https://wiki.osdev.org/ISO_9660

//! ISO 9660 Primary Volume Descriptor parser
//! PVD is located at sector 16, but byte offset depends on sector format

pub use super::sector::SectorFormat;
use super::sector::{detect_sector_format, sector_data_offset};

// PVD is always at sector 16
const PVD_SECTOR: usize = 16;
const PVD_MAGIC_BYTES: usize = 6;
const PVD_TYPE_PRIMARY: u8 = 0x01;
const PVD_MAGIC_START: usize = 1;
const PVD_MAGIC_END: usize = 6;

// ISO9660 PVD field offsets (byte offsets within the 2048-byte descriptor payload).
const SYSTEM_ID_OFFSET: usize = 0x08;
const SYSTEM_ID_LEN: usize = 32;
const VOLUME_ID_OFFSET: usize = 0x28;
const VOLUME_ID_LEN: usize = 32;
const VOLUME_SIZE_START: usize = 0x50;
const VOLUME_SIZE_END: usize = 0x54;
const PUBLISHER_OFFSET: usize = 0x11E;
const PUBLISHER_LEN: usize = 128;
const DATA_PREPARER_OFFSET: usize = 0x19E;
const DATA_PREPARER_LEN: usize = 128;
const APPLICATION_OFFSET: usize = 0x21E;
const APPLICATION_LEN: usize = 128;
const CREATION_DATE_OFFSET: usize = 0x32D;
const CREATION_DATE_TOTAL_BYTES: usize = 17;
const CREATION_DATE_DIGIT_BYTES: usize = 16;

#[derive(Debug, Clone, Default)]
pub struct PrimaryVolumeDescriptor {
    pub system_id: String,
    pub volume_id: String,
    pub volume_size: u32,
    pub publisher: String,
    pub data_preparer: String,
    pub application: String,
    pub creation_date: String,
    pub sector_format: SectorFormat,
}

/// Check for PVD magic at a specific offset
#[allow(clippy::arithmetic_side_effects)]
fn check_pvd_at(buffer: &[u8], offset: usize) -> bool {
    if buffer.len() < offset + PVD_MAGIC_BYTES {
        return false;
    }
    // Type 1 = Primary Volume Descriptor, followed by "CD001" magic
    buffer[offset] == PVD_TYPE_PRIMARY
        && &buffer[offset + PVD_MAGIC_START..offset + PVD_MAGIC_END] == b"CD001"
}

/// Check if buffer contains a valid ISO 9660 PVD
pub fn has_pvd(buffer: &[u8]) -> bool {
    let format = detect_sector_format(buffer);
    let offset = sector_data_offset(PVD_SECTOR, format);
    check_pvd_at(buffer, offset)
}

/// Parse the Primary Volume Descriptor from an ISO/BIN image
#[allow(clippy::arithmetic_side_effects)]
pub fn parse_pvd(buffer: &[u8]) -> Option<PrimaryVolumeDescriptor> {
    let sector_format = detect_sector_format(buffer);
    let offset = sector_data_offset(PVD_SECTOR, sector_format);

    if !check_pvd_at(buffer, offset) {
        return None;
    }

    let pvd = &buffer[offset..];
    if pvd.len() < CREATION_DATE_OFFSET + CREATION_DATE_TOTAL_BYTES {
        return None;
    }

    Some(PrimaryVolumeDescriptor {
        system_id: extract_string(pvd, SYSTEM_ID_OFFSET, SYSTEM_ID_LEN),
        volume_id: extract_string(pvd, VOLUME_ID_OFFSET, VOLUME_ID_LEN),
        volume_size: u32::from_le_bytes([
            pvd[VOLUME_SIZE_START],
            pvd[VOLUME_SIZE_START + 1],
            pvd[VOLUME_SIZE_START + 2],
            pvd[VOLUME_SIZE_END - 1],
        ]),
        publisher: extract_string(pvd, PUBLISHER_OFFSET, PUBLISHER_LEN),
        data_preparer: extract_string(pvd, DATA_PREPARER_OFFSET, DATA_PREPARER_LEN),
        application: extract_string(pvd, APPLICATION_OFFSET, APPLICATION_LEN),
        creation_date: extract_date(pvd, CREATION_DATE_OFFSET),
        sector_format,
    })
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

#[allow(clippy::arithmetic_side_effects)]
fn extract_date(buffer: &[u8], offset: usize) -> String {
    if buffer.len() < offset + CREATION_DATE_TOTAL_BYTES {
        return String::new();
    }
    // ISO 9660 date format: YYYYMMDDHHMMSScc (17 bytes, ASCII digits)
    let date_bytes = &buffer[offset..offset + CREATION_DATE_DIGIT_BYTES];
    let date_str = String::from_utf8_lossy(date_bytes).trim().to_string();

    // Format as readable date if valid
    if date_str.len() >= 8 && date_str.chars().all(|c| c.is_ascii_digit()) {
        let year = &date_str[0..4];
        let month = &date_str[4..6];
        let day = &date_str[6..8];
        format!("{}-{}-{}", year, month, day)
    } else {
        date_str
    }
}

impl std::fmt::Display for PrimaryVolumeDescriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Volume ID: {}", self.volume_id)?;
        if !self.system_id.is_empty() {
            writeln!(f, "System ID: {}", self.system_id)?;
        }
        if !self.publisher.is_empty() {
            writeln!(f, "Publisher: {}", self.publisher)?;
        }
        if !self.data_preparer.is_empty() {
            writeln!(f, "Data Preparer: {}", self.data_preparer)?;
        }
        if !self.application.is_empty() {
            writeln!(f, "Application: {}", self.application)?;
        }
        if !self.creation_date.is_empty() {
            writeln!(f, "Creation Date: {}", self.creation_date)?;
        }
        Ok(())
    }
}
