// References:
//   CD-ROM sector formats (Mode 1, Mode 2 Form 1):
//     https://wiki.osdev.org/ISO_9660#CD-ROM_Sector_Format
//   CD-ROM XA and raw sector layout:
//     https://problemkaputt.de/psx-spx.htm#cdromsectorencoding

//! Centralized CD sector format detection and offset calculation.
//!
//! CD images come in three common layouts:
//! - **Cooked** (2048 bytes/sector): just user data, no headers.
//! - **Raw Mode 1** (2352 bytes/sector): 12-byte sync + 4-byte header + 2048 user data + ECC/EDC.
//! - **Raw Mode 2 Form 1** (2352 bytes/sector): 12-byte sync + 4-byte header + 8-byte subheader + 2048 user data + ECC/EDC.

/// Sector format detected in the disc image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SectorFormat {
    /// 2048-byte data sectors (standard ISO)
    #[default]
    Mode1Cooked,
    /// 2352-byte raw sectors, Mode 1 (16-byte header before user data)
    Mode1Raw,
    /// 2352-byte raw sectors, Mode 2 Form 1 (24-byte header before user data)
    Mode2Form1,
}

/// Cooked sector size (user data only).
pub const SECTOR_COOKED: usize = 2048;
/// Raw sector size (full sector including sync, header, user data, ECC/EDC).
pub const SECTOR_RAW: usize = 2352;
/// Byte offset to user data in a raw Mode 1 sector (12 sync + 4 header).
pub const MODE1_HEADER: usize = 16;
/// Byte offset to user data in a raw Mode 2 Form 1 sector (12 sync + 4 header + 8 subheader).
pub const MODE2_HEADER: usize = 24;

/// The 12-byte sync pattern that begins every raw CD sector.
const SYNC_PATTERN: [u8; 12] = [
    0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00,
];

/// Detect sector format by examining the first sector of the buffer.
///
/// Checks for the 12-byte CD sync pattern at offset 0. If present, the image
/// is raw — the mode byte at offset 15 distinguishes Mode 1 (value `1`) from
/// Mode 2 (value `2`). If no sync pattern is found, the image is cooked.
pub fn detect_sector_format(buffer: &[u8]) -> SectorFormat {
    if buffer.len() >= SYNC_PATTERN.len() && buffer[..SYNC_PATTERN.len()] == SYNC_PATTERN {
        // Raw image — check mode byte at offset 15
        if buffer.len() > 15 && buffer[15] == 2 {
            SectorFormat::Mode2Form1
        } else {
            SectorFormat::Mode1Raw
        }
    } else {
        SectorFormat::Mode1Cooked
    }
}

/// Byte offset to the start of user data for a given sector number.
///
/// - Cooked: `sector * 2048`
/// - Raw Mode 1: `sector * 2352 + 16`
/// - Raw Mode 2 Form 1: `sector * 2352 + 24`
#[allow(clippy::arithmetic_side_effects)]
pub fn sector_data_offset(sector: usize, format: SectorFormat) -> usize {
    match format {
        SectorFormat::Mode1Cooked => sector * SECTOR_COOKED,
        SectorFormat::Mode1Raw => sector * SECTOR_RAW + MODE1_HEADER,
        SectorFormat::Mode2Form1 => sector * SECTOR_RAW + MODE2_HEADER,
    }
}

/// Translate a logical byte offset (as if sectors were continuous 2048-byte
/// blocks) to a physical byte offset in the image, accounting for sector
/// headers in raw images.
///
/// For cooked images this is the identity function. For raw images it splits
/// the logical offset into sector number + intra-sector offset and
/// reconstructs the physical position.
#[allow(clippy::arithmetic_side_effects)]
pub fn logical_to_physical(logical_offset: usize, format: SectorFormat) -> usize {
    match format {
        SectorFormat::Mode1Cooked => logical_offset,
        SectorFormat::Mode1Raw => {
            let sector = logical_offset / SECTOR_COOKED;
            let offset_in_sector = logical_offset % SECTOR_COOKED;
            sector * SECTOR_RAW + MODE1_HEADER + offset_in_sector
        }
        SectorFormat::Mode2Form1 => {
            let sector = logical_offset / SECTOR_COOKED;
            let offset_in_sector = logical_offset % SECTOR_COOKED;
            sector * SECTOR_RAW + MODE2_HEADER + offset_in_sector
        }
    }
}
