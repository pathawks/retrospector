// References:
//   FDS disk format and block structure:
//     https://www.nesdev.org/wiki/FDS_disk_format
//   FDS file format (.fds / fwNES header):
//     https://www.nesdev.org/wiki/FDS_file_format
//   BCD date encoding:
//     https://www.nesdev.org/wiki/FDS_disk_format#Disk_Info_Block

use std::fmt;

use super::helpers::compute_sha1;
use crc::{CRC_32_ISO_HDLC, Crc};

use crate::traits::{error::ParseError, rom_hash::RomHash, rominfo::RomInfo};

mod licensee;
use licensee::lookup_licensee;

const DISK_SIDE_SIZE: usize = 65500;
const NINTENDO_HVC: &[u8; 14] = b"*NINTENDO-HVC*";
const FWNES_MAGIC: &[u8; 4] = b"FDS\x1A";
const FWNES_HEADER_BYTES: usize = 16;

// FDS container block layout (CRCs omitted in .FDS dumps).
const SIDE_MIN_PARSE_BYTES: usize = 0x3A; // block 1 (0x38 bytes) + block 2 (2 bytes)
const BLOCK1_TYPE_OFFSET: usize = 0x00;
const BLOCK1_EXPECTED_TYPE: u8 = 0x01;
const BLOCK1_MAGIC_START: usize = 0x01;
const BLOCK1_MAGIC_END: usize = 0x0F;
const BLOCK2_TYPE_OFFSET: usize = 0x38;
const BLOCK2_EXPECTED_TYPE: u8 = 0x02;
const BLOCK2_FILE_COUNT_OFFSET: usize = 0x39;
const FILE_TABLE_START_OFFSET: usize = 0x3A;

const BLOCK3_EXPECTED_TYPE: u8 = 0x03;
const BLOCK3_BYTES: usize = 16;
const BLOCK3_FILE_NUMBER_OFFSET: usize = 1;
const BLOCK3_FILE_NAME_START: usize = 3;
const BLOCK3_FILE_NAME_END: usize = 11;
const BLOCK3_FILE_SIZE_LOW_OFFSET: usize = 13;
const BLOCK3_FILE_SIZE_HIGH_OFFSET: usize = 14;
const BLOCK3_FILE_TYPE_OFFSET: usize = 15;

const BLOCK4_EXPECTED_TYPE: u8 = 0x04;
const BLOCK4_TYPE_BYTES: usize = 1;

// Disk-info field offsets within block 1.
const LICENSEE_CODE_OFFSET: usize = 0x0F;
const GAME_NAME_START: usize = 0x10;
const GAME_NAME_MID: usize = 0x11;
const GAME_NAME_END: usize = 0x12;
const GAME_TYPE_OFFSET: usize = 0x13;
const GAME_VERSION_OFFSET: usize = 0x14;
const SIDE_NUMBER_OFFSET: usize = 0x15;
const DISK_NUMBER_OFFSET: usize = 0x16;
const MANUFACTURING_DATE_START: usize = 0x1F;
const MANUFACTURING_DATE_MID: usize = 0x20;
const MANUFACTURING_DATE_END: usize = 0x21;
const COUNTRY_CODE_OFFSET: usize = 0x22;
const REWRITE_DATE_START: usize = 0x2C;
const REWRITE_DATE_MID: usize = 0x2D;
const REWRITE_DATE_END: usize = 0x2E;
const DISK_REWRITE_COUNT_OFFSET: usize = 0x34;
const DISK_VERSION_OFFSET: usize = 0x37;

// ---------------------------------------------------------------------------
// Enums for FDS metadata fields
// ---------------------------------------------------------------------------

/// FDS Block 3 file type (standard FDS offset `0x0F` within the file header).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum FdsFileKind {
    /// Program data loaded to CPU address space.
    #[default]
    Program,
    /// Character/tile data loaded to PPU address space.
    Character,
    /// Nametable data loaded to PPU nametable address space.
    Nametable,
    /// Unrecognised file type byte.
    Unknown(u8),
}

impl FdsFileKind {
    fn from_byte(b: u8) -> Self {
        match b {
            0 => Self::Program,
            1 => Self::Character,
            2 => Self::Nametable,
            _ => Self::Unknown(b),
        }
    }

    /// True when the file contains program data (type 0).
    pub fn is_program(&self) -> bool {
        matches!(self, Self::Program)
    }

    /// True when the file contains character/tile data (type 1).
    pub fn is_character(&self) -> bool {
        matches!(self, Self::Character)
    }
}

impl fmt::Display for FdsFileKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Program => write!(f, "PRG"),
            Self::Character => write!(f, "CHR"),
            Self::Nametable => write!(f, "NT "),
            Self::Unknown(b) => write!(f, "?{:02X}", b),
        }
    }
}

/// FDS disk info game type byte (standard FDS Block 1 offset `0x13`).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum FdsGameType {
    /// Normal retail disk (`0x20`, ASCII space).
    #[default]
    Normal,
    /// Event/promotional disk (`0x45`, ASCII 'E').
    Event,
    /// Reduced-price disk sold via advertising (`0x52`, ASCII 'R').
    Reduction,
    /// Unrecognised game type byte.
    Unknown(u8),
}

impl FdsGameType {
    fn from_byte(b: u8) -> Self {
        match b {
            0x20 => Self::Normal,
            0x45 => Self::Event,
            0x52 => Self::Reduction,
            _ => Self::Unknown(b),
        }
    }
}

impl fmt::Display for FdsGameType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Normal => write!(f, "Normal disk"),
            Self::Event => write!(f, "Event"),
            Self::Reduction => write!(f, "Reduction in price via advertising"),
            Self::Unknown(b) => write!(f, "Unknown ({:#04X})", b),
        }
    }
}

/// FDS disk info country code (standard FDS Block 1 offset `0x22`).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum FdsCountry {
    /// Japan (`0x49`, ASCII 'I').
    #[default]
    Japan,
    /// Unrecognised country byte.
    Unknown(u8),
}

impl FdsCountry {
    fn from_byte(b: u8) -> Self {
        match b {
            0x49 => Self::Japan,
            _ => Self::Unknown(b),
        }
    }
}

impl fmt::Display for FdsCountry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Japan => write!(f, "Japan (0x49)"),
            Self::Unknown(b) => write!(f, "Unknown ({:#04X})", b),
        }
    }
}

/// FDS disk side label (standard FDS Block 1 offset `0x15`).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum FdsDiskSide {
    #[default]
    A,
    B,
    Unknown(u8),
}

impl FdsDiskSide {
    fn from_byte(b: u8) -> Self {
        match b {
            0x00 => Self::A,
            0x01 => Self::B,
            _ => Self::Unknown(b),
        }
    }
}

impl fmt::Display for FdsDiskSide {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::A => write!(f, "A"),
            Self::B => write!(f, "B"),
            Self::Unknown(b) => write!(f, "? ({:#04X})", b),
        }
    }
}

/// A BCD-encoded date as stored in FDS disk info blocks.
///
/// FDS dates use three BCD bytes: year, month, day. The year byte
/// encodes differently depending on era:
/// - `0x80`–`0x99`: Gregorian (e.g. `0x86` = 1986)
/// - `0x60`–`0x7F`: Shōwa era (year = 1925 + BCD value)
/// - `0x00`–`0x5F`: Heisei era (year = 1988 + BCD value)
///
/// A date of all zeros indicates the field is absent/unknown.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub struct FdsDate {
    raw: [u8; 3],
}

impl FdsDate {
    /// Create a date from the three raw BCD bytes `[year, month, day]`.
    pub fn from_bytes(bytes: [u8; 3]) -> Self {
        Self { raw: bytes }
    }

    /// True when all three bytes are zero (no date recorded).
    pub fn is_empty(&self) -> bool {
        self.raw == [0, 0, 0]
    }

    /// Decode the BCD year byte to a Gregorian year.
    pub fn year(&self) -> u16 {
        parse_fds_year(self.raw[0])
    }

    /// Decode the BCD month byte (1–12, or 0 if absent).
    pub fn month(&self) -> u8 {
        bcd_to_u8(self.raw[1])
    }

    /// Decode the BCD day byte (1–31, or 0 if absent).
    pub fn day(&self) -> u8 {
        bcd_to_u8(self.raw[2])
    }
}

impl fmt::Display for FdsDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_empty() {
            write!(f, "(none)")
        } else {
            write!(
                f,
                "{:04}-{:02}-{:02}",
                self.year(),
                self.month(),
                self.day()
            )
        }
    }
}

// ---------------------------------------------------------------------------
// Core structs
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Eq, PartialEq)]
struct FdsFileEntry {
    file_number: u8,
    file_name: [u8; 8],
    file_kind: FdsFileKind,
    file_size: u16,
    crc32: u32,
    sha1: [u8; 20],
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
struct FdsSideInfo {
    licensee_code: u8,
    game_name: [u8; 3],
    game_type: FdsGameType,
    game_version: u8,
    side: FdsDiskSide,
    disk_number: u8,
    manufacturing_date: FdsDate,
    country: FdsCountry,
    rewrite_date: FdsDate,
    disk_rewrite_count: u8,
    disk_version: u8,
    file_count: u8,
    files: Vec<FdsFileEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct FdsRomInfo {
    has_header: bool,
    sides: Vec<FdsSideInfo>,
    prg_size: u32,
    chr_size: u32,
    prg_crc32: u32,
    chr_crc32: Option<u32>,
    rom_sha1: [u8; 20],
}

impl RomInfo for FdsRomInfo {
    fn console(&self) -> &'static str {
        "Famicom Disk System"
    }
}

impl RomHash for FdsRomInfo {
    fn sha1(&self) -> [u8; 20] {
        self.rom_sha1
    }
}

impl TryFrom<&[u8]> for FdsRomInfo {
    type Error = ParseError;

    #[allow(clippy::arithmetic_side_effects)]
    fn try_from(buffer: &[u8]) -> Result<Self, Self::Error> {
        // Detect optional fwNES header (0x46 0x44 0x53 0x1A = "FDS\x1A")
        let (has_header, data_start) =
            if buffer.len() >= FWNES_MAGIC.len() && &buffer[0..FWNES_MAGIC.len()] == FWNES_MAGIC {
                (true, FWNES_HEADER_BYTES)
            } else {
                (false, 0usize)
            };

        let disk_data = &buffer[data_start..];

        // Need at least block 1 (0x38 bytes) + block 2 (2 bytes).
        if disk_data.len() < SIDE_MIN_PARSE_BYTES {
            return Err(ParseError::BufferTooSmall);
        }

        // Verify first side starts with a valid disk info block
        if disk_data[BLOCK1_TYPE_OFFSET] != BLOCK1_EXPECTED_TYPE
            || &disk_data[BLOCK1_MAGIC_START..BLOCK1_MAGIC_END] != NINTENDO_HVC
        {
            return Err(ParseError::MagicNotFound);
        }

        let rom_sha1 = compute_sha1(disk_data);

        // Parse all disk sides (each 65500 bytes in the .FDS format)
        let mut sides = Vec::new();
        let mut prg_data_all = Vec::new();
        let mut chr_data_all = Vec::new();
        let mut offset = 0;
        while offset + SIDE_MIN_PARSE_BYTES <= disk_data.len() {
            let side = &disk_data[offset..];

            // Each side must start with a valid disk info block
            if side[BLOCK1_TYPE_OFFSET] != BLOCK1_EXPECTED_TYPE
                || &side[BLOCK1_MAGIC_START..BLOCK1_MAGIC_END] != NINTENDO_HVC
            {
                break;
            }

            // Block 2 (file amount) immediately follows block 1 at offset 0x38
            // (CRCs are omitted from .FDS files)
            let file_count = if side.len() >= SIDE_MIN_PARSE_BYTES
                && side[BLOCK2_TYPE_OFFSET] == BLOCK2_EXPECTED_TYPE
            {
                side[BLOCK2_FILE_COUNT_OFFSET]
            } else {
                0
            };

            // Parse file entries (Block 3 / Block 4 pairs)
            let mut files = Vec::new();
            let side_end = side.len().min(DISK_SIDE_SIZE);
            let mut file_offset = FILE_TABLE_START_OFFSET; // right after Block 2

            for _ in 0..file_count {
                // Block 3: File Header (16 bytes: 1 type + 15 header)
                if file_offset + BLOCK3_BYTES > side_end {
                    break;
                }
                if side[file_offset] != BLOCK3_EXPECTED_TYPE {
                    break;
                }
                let file_number = side[file_offset + BLOCK3_FILE_NUMBER_OFFSET];
                let mut file_name = [0u8; 8];
                file_name.copy_from_slice(
                    &side[file_offset + BLOCK3_FILE_NAME_START..file_offset + BLOCK3_FILE_NAME_END],
                );
                let file_size = u16::from_le_bytes([
                    side[file_offset + BLOCK3_FILE_SIZE_LOW_OFFSET],
                    side[file_offset + BLOCK3_FILE_SIZE_HIGH_OFFSET],
                ]);
                let file_type = FdsFileKind::from_byte(side[file_offset + BLOCK3_FILE_TYPE_OFFSET]);
                file_offset += BLOCK3_BYTES;

                // Block 4: File Data (1 type byte + file_size data bytes)
                if file_offset + BLOCK4_TYPE_BYTES + file_size as usize > side_end {
                    break;
                }
                if side[file_offset] != BLOCK4_EXPECTED_TYPE {
                    break;
                }
                let file_data = &side[file_offset + BLOCK4_TYPE_BYTES
                    ..file_offset + BLOCK4_TYPE_BYTES + file_size as usize];

                match file_type {
                    FdsFileKind::Program => prg_data_all.extend_from_slice(file_data),
                    FdsFileKind::Character => chr_data_all.extend_from_slice(file_data),
                    _ => {}
                }

                let crc = Crc::<u32>::new(&CRC_32_ISO_HDLC);
                let crc32 = crc.checksum(file_data);

                let sha1 = compute_sha1(file_data);

                files.push(FdsFileEntry {
                    file_number,
                    file_name,
                    file_kind: file_type,
                    file_size,
                    crc32,
                    sha1,
                });

                file_offset += BLOCK4_TYPE_BYTES + file_size as usize;
            }

            sides.push(FdsSideInfo {
                licensee_code: side[LICENSEE_CODE_OFFSET],
                game_name: [
                    side[GAME_NAME_START],
                    side[GAME_NAME_MID],
                    side[GAME_NAME_END],
                ],
                game_type: FdsGameType::from_byte(side[GAME_TYPE_OFFSET]),
                game_version: side[GAME_VERSION_OFFSET],
                side: FdsDiskSide::from_byte(side[SIDE_NUMBER_OFFSET]),
                disk_number: side[DISK_NUMBER_OFFSET],
                manufacturing_date: FdsDate::from_bytes([
                    side[MANUFACTURING_DATE_START],
                    side[MANUFACTURING_DATE_MID],
                    side[MANUFACTURING_DATE_END],
                ]),
                country: FdsCountry::from_byte(side[COUNTRY_CODE_OFFSET]),
                rewrite_date: FdsDate::from_bytes([
                    side[REWRITE_DATE_START],
                    side[REWRITE_DATE_MID],
                    side[REWRITE_DATE_END],
                ]),
                disk_rewrite_count: side[DISK_REWRITE_COUNT_OFFSET],
                disk_version: side[DISK_VERSION_OFFSET],
                file_count,
                files,
            });

            offset += DISK_SIDE_SIZE;
        }

        if sides.is_empty() {
            return Err(ParseError::InvalidHeader);
        }

        let crc = Crc::<u32>::new(&CRC_32_ISO_HDLC);
        let prg_size = prg_data_all.len() as u32;
        let chr_size = chr_data_all.len() as u32;
        let prg_crc32 = crc.checksum(&prg_data_all);
        let chr_crc32 = if chr_data_all.is_empty() {
            None
        } else {
            Some(crc.checksum(&chr_data_all))
        };

        Ok(FdsRomInfo {
            has_header,
            sides,
            prg_size,
            chr_size,
            prg_crc32,
            chr_crc32,
            rom_sha1,
        })
    }
}

impl std::fmt::Display for FdsRomInfo {
    #[allow(clippy::arithmetic_side_effects)]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let first = &self.sides[0];

        let name = String::from_utf8_lossy(&first.game_name);
        writeln!(f, "Game Name: {}", name.trim_end_matches('\0'))?;
        writeln!(f, "Game Version: {}", first.game_version)?;
        writeln!(f, "Game Type: {}", first.game_type)?;

        match lookup_licensee(first.licensee_code) {
            Some(name) => writeln!(f, "Licensee: {} ({:#04X})", name, first.licensee_code)?,
            None => writeln!(f, "Licensee: {:#04X}", first.licensee_code)?,
        }

        writeln!(f, "Manufacturing Date: {}", first.manufacturing_date)?;
        writeln!(f, "Country: {}", first.country)?;

        writeln!(f, "Disk Sides: {}", self.sides.len())?;
        for (i, side) in self.sides.iter().enumerate() {
            writeln!(
                f,
                "  Side {}: Disk {} Side {} - {} files",
                i + 1,
                side.disk_number + 1,
                side.side,
                side.file_count
            )?;
            for file in &side.files {
                let name = String::from_utf8_lossy(&file.file_name);
                let name = name.trim_end_matches('\0');
                let sha1_hex: String = file.sha1.iter().map(|b| format!("{:02X}", b)).collect();
                writeln!(
                    f,
                    "    File {:2}: {:<8} ({})  {:5} bytes  CRC32: {:08X}  SHA1: {}",
                    file.file_number, name, file.file_kind, file.file_size, file.crc32, sha1_hex
                )?;
            }
        }

        write!(
            f,
            "PRG-ROM: {:3}kb\tCRC32: {:08X}",
            self.prg_size / 1024,
            self.prg_crc32
        )?;
        if let Some(chr_crc32) = self.chr_crc32 {
            write!(
                f,
                "\nCHR-ROM: {:3}kb\tCRC32: {:08X}",
                self.chr_size / 1024,
                chr_crc32
            )?;
        }
        writeln!(f)?;

        if first.disk_rewrite_count != 0x00 {
            writeln!(f, "Rewrite Count: {}", bcd_to_u8(first.disk_rewrite_count))?;
            writeln!(f, "Rewrite Date: {}", first.rewrite_date)?;
        }

        if self.has_header {
            writeln!(f, "File Format: fwNES (.FDS with header)")?;
        } else {
            writeln!(f, "File Format: .FDS (headerless)")?;
        }

        writeln!(f, "{}", self as &dyn RomHash)
    }
}

#[allow(clippy::arithmetic_side_effects)]
fn bcd_to_u8(bcd: u8) -> u8 {
    (bcd >> 4) * 10 + (bcd & 0x0F)
}

#[allow(clippy::arithmetic_side_effects)]
fn parse_fds_year(bcd_byte: u8) -> u16 {
    let val = bcd_to_u8(bcd_byte) as u16;
    if bcd_byte >= 0x80 {
        // Last two digits of Gregorian year (e.g., 0x85 = 1985, 0x86 = 1986)
        1900 + val
    } else if bcd_byte >= 0x60 {
        // Showa era (Showa 1 = 1926, year = 1925 + val)
        // Disk Writer kiosks used Showa dates past the era's end (1988) through 2003
        1925 + val
    } else {
        // Heisei era (Heisei 1 = 1989, year = 1988 + val)
        1988 + val
    }
}
