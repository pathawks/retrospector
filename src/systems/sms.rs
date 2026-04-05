// References:
//   SMS/GG ROM header ("TMR SEGA" signature):
//     https://www.smspower.org/Development/ROMHeader
//   Checksum algorithm and region/size nibbles:
//     https://www.smspower.org/Development/ROMHeader

use super::helpers::compute_sha1;
use byte_unit::{Byte, UnitType};
use byteorder::{ByteOrder, LittleEndian};

use crate::traits::{
    error::ParseError,
    rom_hash::RomHash,
    rominfo::{DatMeta, RomInfo},
    stored_checksum::StoredChecksum,
};

// Sega 8-bit header research constants (SMS/GG "TMR SEGA" block).
const TMR_SEGA_SIGNATURE: &[u8; 8] = b"TMR SEGA";
const HEADER_OFFSETS: [usize; 3] = [0x7FF0, 0x3FF0, 0x1FF0];
const HEADER_BYTES: usize = 16;
const SIGNATURE_BYTES: usize = 8;
const STORED_CHECKSUM_START: usize = 10;
const STORED_CHECKSUM_END: usize = 12;
const PRODUCT_LOW_OFFSET: usize = 12;
const PRODUCT_MID_OFFSET: usize = 13;
const PRODUCT_CODE_BYTE_OFFSET: usize = 14;
const REGION_VERSION_OFFSET: usize = 15;
const LOW_NIBBLE_MASK: u8 = 0x0F;

// Region nibble values from Sega header docs.
const REGION_SMS_JAPAN: u8 = 3;
const REGION_SMS_EXPORT: u8 = 4;
const REGION_GG_JAPAN: u8 = 5;
const REGION_GG_EXPORT: u8 = 6;
const REGION_GG_INTERNATIONAL: u8 = 7;

// ROM size codes used in the Sega header nibble.
const SIZE_CODE_256KB: u8 = 0x0;
const SIZE_CODE_512KB: u8 = 0x1;
const SIZE_CODE_1MB: u8 = 0x2;
const SIZE_CODE_8KB: u8 = 0xA;
const SIZE_CODE_16KB: u8 = 0xB;
const SIZE_CODE_32KB: u8 = 0xC;
const SIZE_CODE_48KB: u8 = 0xD;
const SIZE_CODE_64KB: u8 = 0xE;
const SIZE_CODE_128KB: u8 = 0xF;

const CHECKSUM_EXTRA_START: usize = 0x8000;
const CHECKSUM_END_64KB: usize = 0x10000;
const CHECKSUM_END_128KB: usize = 0x20000;
const CHECKSUM_END_256KB: usize = 0x40000;
const CHECKSUM_END_512KB: usize = 0x80000;
const CHECKSUM_END_1MB: usize = 0x100000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SmsParseError {
    HeaderSignatureNotFound,
}

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
pub enum SmsConsole {
    #[default]
    MasterSystem,
    GameGear,
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct SmsRomInfo {
    pub console: SmsConsole,
    pub product_code: u32,
    pub version: u8,
    pub region_code: u8,
    pub rom_size_code: u8,
    pub file_size: usize,
    pub stored_checksum: u16,
    pub calculated_checksum: u16,
    pub header_offset: usize,
    pub size_header_mismatch: bool,
    pub rom_sha1: [u8; 20],
}

impl RomInfo for SmsRomInfo {
    fn console(&self) -> &'static str {
        match self.console {
            SmsConsole::MasterSystem => "Sega Master System",
            SmsConsole::GameGear => "Sega Game Gear",
        }
    }

    fn dat_meta(&self) -> DatMeta {
        let serial = if self.product_code > 0 {
            Some(self.product_code.to_string())
        } else {
            None
        };
        DatMeta {
            serial,
            ..DatMeta::default()
        }
    }
}

/// Parse the Sega SMS/GG footer header ("TMR SEGA") and derive metadata.
///
/// Research notes:
/// - The header is mirrored at fixed boundaries (0x1FF0/0x3FF0/0x7FF0) based on
///   dump size and mapper conventions.
/// - Product code uses packed BCD digits across bytes 12..14.
/// - Checksum coverage depends on the declared ROM size nibble.
#[allow(clippy::arithmetic_side_effects)]
fn parse_sms_info(buffer: &[u8]) -> Result<SmsRomInfo, SmsParseError> {
    let header_offset = HEADER_OFFSETS
        .iter()
        .find(|&&offset| {
            buffer.len() >= offset + HEADER_BYTES
                && &buffer[offset..offset + SIGNATURE_BYTES] == TMR_SEGA_SIGNATURE
        })
        .copied()
        .ok_or(SmsParseError::HeaderSignatureNotFound)?;

    let rom_sha1 = compute_sha1(buffer);
    let header = &buffer[header_offset..header_offset + HEADER_BYTES];

    let stored_checksum =
        LittleEndian::read_u16(&header[STORED_CHECKSUM_START..STORED_CHECKSUM_END]);
    let product_code = {
        let low = header[PRODUCT_LOW_OFFSET];
        let mid = header[PRODUCT_MID_OFFSET];
        let high = (header[PRODUCT_CODE_BYTE_OFFSET] & LOW_NIBBLE_MASK) as u32;
        // BCD decode: each byte holds two decimal digits (tens in high nibble).
        let low_dec = ((low >> 4) as u32) * 10 + (low & LOW_NIBBLE_MASK) as u32;
        let mid_dec = ((mid >> 4) as u32) * 10 + (mid & LOW_NIBBLE_MASK) as u32;
        high * 10000 + mid_dec * 100 + low_dec
    };

    let rom_size_code = header[PRODUCT_CODE_BYTE_OFFSET] >> 4;
    let region_code = header[REGION_VERSION_OFFSET] >> 4;
    let version = header[REGION_VERSION_OFFSET] & LOW_NIBBLE_MASK;

    let console = match region_code {
        REGION_GG_JAPAN | REGION_GG_EXPORT | REGION_GG_INTERNATIONAL => SmsConsole::GameGear,
        _ => SmsConsole::MasterSystem,
    };

    let expected_header_offset = expected_header_offset_for_size(rom_size_code);
    let size_header_mismatch = expected_header_offset != header_offset;
    let calculated_checksum = calculate_checksum(buffer, header_offset, rom_size_code);

    Ok(SmsRomInfo {
        console,
        product_code,
        version,
        region_code,
        rom_size_code,
        file_size: buffer.len(),
        stored_checksum,
        calculated_checksum,
        header_offset,
        size_header_mismatch,
        rom_sha1,
    })
}

impl TryFrom<&[u8]> for SmsRomInfo {
    type Error = ParseError;

    fn try_from(buffer: &[u8]) -> Result<Self, Self::Error> {
        parse_sms_info(buffer).map_err(|_| ParseError::MagicNotFound)
    }
}

impl StoredChecksum<u16> for SmsRomInfo {
    fn stored_checksum(&self) -> u16 {
        self.stored_checksum
    }

    fn calculated_checksum(&self) -> u16 {
        self.calculated_checksum
    }
}

impl RomHash for SmsRomInfo {
    fn sha1(&self) -> [u8; 20] {
        self.rom_sha1
    }
}

impl std::fmt::Display for SmsRomInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let region_description = match self.region_code {
            REGION_SMS_JAPAN => "Japan (SMS)",
            REGION_SMS_EXPORT => "Export (SMS)",
            REGION_GG_JAPAN => "Japan (GG)",
            REGION_GG_EXPORT => "Export (GG)",
            REGION_GG_INTERNATIONAL => "International (GG)",
            _ => "Unknown",
        };

        let header_rom_size: Option<u64> = match self.rom_size_code {
            SIZE_CODE_8KB => Some(8 * 1024),
            SIZE_CODE_16KB => Some(16 * 1024),
            SIZE_CODE_32KB => Some(32 * 1024),
            SIZE_CODE_48KB => Some(48 * 1024),
            SIZE_CODE_64KB => Some(64 * 1024),
            SIZE_CODE_128KB => Some(128 * 1024),
            SIZE_CODE_256KB => Some(256 * 1024),
            SIZE_CODE_512KB => Some(512 * 1024),
            SIZE_CODE_1MB => Some(1024 * 1024),
            _ => None,
        };
        let file_size = Byte::from(self.file_size as u64);

        writeln!(f, "Product Code: {}", self.product_code)?;
        writeln!(f, "Region: {}", region_description)?;
        if self.version != 0 {
            writeln!(f, "Version: {}", self.version)?;
        }
        writeln!(
            f,
            "ROM Size: {}",
            file_size.get_appropriate_unit(UnitType::Binary)
        )?;
        if self.stored_checksum != 0 && header_rom_size.is_some_and(|h| h != self.file_size as u64)
        {
            let header_size = Byte::from(header_rom_size.unwrap_or(0));
            writeln!(
                f,
                "Warning: Header declares {} (checksum covers only this range), but file is {}",
                header_size.get_appropriate_unit(UnitType::Binary),
                file_size.get_appropriate_unit(UnitType::Binary),
            )?;
        }
        writeln!(f, "Header Location: {:#06X}", self.header_offset)?;
        if self.stored_checksum != 0 {
            writeln!(f, "{}", self as &dyn StoredChecksum<u16>)?;
        }
        writeln!(f, "{}", self as &dyn RomHash)?;
        if self.size_header_mismatch {
            let expected = expected_header_offset_for_size(self.rom_size_code);
            writeln!(
                f,
                "Warning: ROM size code ({:#X}) expects header at {:#06X}, but found at {:#06X}",
                self.rom_size_code, expected, self.header_offset
            )?;
        }

        Ok(())
    }
}

/// Returns the expected header offset for a given ROM size code.
/// - 8KB ROMs (0xA): header at 0x1FF0
/// - 16KB ROMs (0xB): header at 0x3FF0
/// - 32KB+ ROMs (0xC-0x2): header at 0x7FF0
fn expected_header_offset_for_size(rom_size_code: u8) -> usize {
    match rom_size_code {
        SIZE_CODE_8KB => 0x1FF0,  // 8KB
        SIZE_CODE_16KB => 0x3FF0, // 16KB
        _ => 0x7FF0,              // 32KB and larger
    }
}

/// Calculate SMS/GG checksum based on ROM size code.
/// The checksum range depends on the size code and excludes the header (0x7FF0-0x7FFF).
#[allow(clippy::arithmetic_side_effects)]
fn calculate_checksum(buffer: &[u8], header_offset: usize, rom_size_code: u8) -> u16 {
    let mut checksum = 0u16;

    // Helper to sum a range of bytes
    let sum_range = |start: usize, end: usize| -> u16 {
        if end > buffer.len() {
            return buffer[start.min(buffer.len())..]
                .iter()
                .fold(0u16, |acc, &b| acc.wrapping_add(b as u16));
        }
        buffer[start..end]
            .iter()
            .fold(0u16, |acc, &b| acc.wrapping_add(b as u16))
    };

    // Always include 0x0000 to header_offset (excludes header at 0x7FF0-0x7FFF)
    checksum = checksum.wrapping_add(sum_range(0, header_offset));

    // Add additional ranges based on ROM size code
    match rom_size_code {
        SIZE_CODE_8KB..=SIZE_CODE_48KB => {
            // 8KB-48KB: Only first bank (already covered above)
        }
        SIZE_CODE_64KB => {
            // 64KB: Add 0x8000-0xFFFF
            checksum = checksum.wrapping_add(sum_range(CHECKSUM_EXTRA_START, CHECKSUM_END_64KB));
        }
        SIZE_CODE_128KB => {
            // 128KB: Add 0x8000-0x1FFFF
            checksum = checksum.wrapping_add(sum_range(CHECKSUM_EXTRA_START, CHECKSUM_END_128KB));
        }
        SIZE_CODE_256KB => {
            // 256KB: Add 0x8000-0x3FFFF
            checksum = checksum.wrapping_add(sum_range(CHECKSUM_EXTRA_START, CHECKSUM_END_256KB));
        }
        SIZE_CODE_512KB => {
            // 512KB: Add 0x8000-0x7FFFF
            checksum = checksum.wrapping_add(sum_range(CHECKSUM_EXTRA_START, CHECKSUM_END_512KB));
        }
        SIZE_CODE_1MB => {
            // 1MB: Add 0x8000-0xFFFFF
            checksum = checksum.wrapping_add(sum_range(CHECKSUM_EXTRA_START, CHECKSUM_END_1MB));
        }
        _ => {}
    }

    checksum
}
