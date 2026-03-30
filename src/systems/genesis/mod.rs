// References:
//   Sega Mega Drive / Genesis ROM header format:
//     https://segaretro.org/Sega_Mega_Drive/ROM_format
//   Checksum algorithm:
//     https://segaretro.org/Sega_Mega_Drive/Checksum
//   Region codes and device support strings:
//     https://plutiedev.com/rom-header#region

use super::helpers::{compute_sha1, first_non_empty};
use crate::systems::disc::sega_ip::dat_region_from_area_codes;
use crate::traits::error::ParseError;
use crate::traits::rom_hash::RomHash;
use crate::traits::rominfo::{DatMeta, RomInfo};
use crate::traits::stored_checksum::StoredChecksum;
use byteorder::{BigEndian, ByteOrder};
use encoding_rs::SHIFT_JIS;
use std::io::{self, Error};
use unicode_normalization::UnicodeNormalization;

mod publisher;
pub use publisher::publisher_from_copyright;

// Sega cartridge header layout (0x100..0x1FF), shared by Mega Drive/Genesis and 32X.
const HEADER_MIN_BYTES: usize = 0x200;
const CONSOLE_NAME_START: usize = 0x100;
const CONSOLE_NAME_END: usize = 0x110;
const COPYRIGHT_START: usize = 0x110;
const COPYRIGHT_END: usize = 0x120;
const DOMESTIC_TITLE_START: usize = 0x120;
const DOMESTIC_TITLE_END: usize = 0x150;
const OVERSEAS_TITLE_START: usize = 0x150;
const OVERSEAS_TITLE_END: usize = 0x180;
const STORED_CHECKSUM_START: usize = 0x18E;
const STORED_CHECKSUM_END: usize = 0x190;
const DEVICE_SUPPORT_START: usize = 0x190;
const DEVICE_SUPPORT_END: usize = 0x1A0;
const ROM_SIZE_START: usize = 0x1A4;
const ROM_SIZE_END: usize = 0x1A8;
const REGIONS_START: usize = 0x1F0;
const REGIONS_END: usize = 0x1F4;

// Header signature values stored at 0x100..0x10F.
const CONSOLE_NAME_32X: &[u8; 16] = b"SEGA 32X        ";
const CONSOLE_NAME_MEGA_DRIVE: &[u8; 16] = b"SEGA MEGA DRIVE ";
const CONSOLE_NAME_GENESIS: &[u8; 16] = b"SEGA GENESIS    ";

// Legacy heuristic: Sonic & Knuckles lock-on base ROM has this stored checksum.
const SONIC_AND_KNUCKLES_CHECKSUM: u16 = 0xDFB3;
const CHECKSUM_WORD_START: usize = HEADER_MIN_BYTES / 2;

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
pub enum SegaConsole {
    #[default]
    Genesis,
    X32,
}
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct SegaRomInfo {
    pub domestic_title: String,
    pub overseas_title: String,
    pub copyright: String,
    pub device_support: String,
    pub regions: String,
    pub stored_checksum: u16,
    pub calced_checksum: u16,
    pub lock_on: Option<Box<SegaRomInfo>>,
    pub console: SegaConsole,
    pub rom_sha1: [u8; 20],
}

impl RomInfo for SegaRomInfo {
    fn console(&self) -> &'static str {
        match self.console {
            SegaConsole::Genesis => "Sega Genesis/Mega Drive",
            SegaConsole::X32 => "Sega 32X",
        }
    }

    fn dat_meta(&self) -> DatMeta {
        // Genesis header uses both letter (J/U/E) and hex-bitmask (1/4/5/8/F) region codes.
        // Normalize to J/U/E for dat_region_from_area_codes.
        let has_j = self.regions.contains('J')
            || self.regions.contains('1')
            || self.regions.contains('5')
            || self.regions.contains('F');
        let has_u = self.regions.contains('U')
            || self.regions.contains('4')
            || self.regions.contains('5')
            || self.regions.contains('F');
        let has_e =
            self.regions.contains('E') || self.regions.contains('8') || self.regions.contains('F');

        let mut normalized = String::new();
        if has_j {
            normalized.push('J');
        }
        if has_u {
            normalized.push('U');
        }
        if has_e {
            normalized.push('E');
        }

        DatMeta {
            title: first_non_empty(&[&self.overseas_title, &self.domestic_title]),
            region: dat_region_from_area_codes(&normalized),
            ..DatMeta::default()
        }
    }
}

impl TryFrom<&[u8]> for SegaRomInfo {
    type Error = ParseError;

    fn try_from(buffer: &[u8]) -> Result<Self, Self::Error> {
        if !is_genesis_rom(buffer) && !is_32x_rom(buffer) {
            return Err(ParseError::MagicNotFound);
        }
        parse_sega_rom(buffer).map_err(|_| ParseError::InvalidHeader)
    }
}

impl StoredChecksum<u16> for SegaRomInfo {
    fn stored_checksum(&self) -> u16 {
        self.stored_checksum
    }

    fn calculated_checksum(&self) -> u16 {
        self.calced_checksum
    }
}

impl RomHash for SegaRomInfo {
    fn sha1(&self) -> [u8; 20] {
        self.rom_sha1
    }
}

impl std::fmt::Display for SegaRomInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let region_description = if self.regions.contains('J')
            && self.regions.contains('U')
            && self.regions.contains('E')
        {
            "Japan, USA, Europe"
        } else if self.regions.contains('J') && self.regions.contains('U') {
            "Japan, USA"
        } else if self.regions.contains('J') && self.regions.contains('E') {
            "Japan, Europe"
        } else if self.regions.contains('U') && self.regions.contains('E') {
            "USA, Europe"
        } else if self.regions.contains('J') {
            "Japan"
        } else if self.regions.contains('U') {
            "USA"
        } else if self.regions.contains('E') {
            "Europe"
        } else if self.regions.contains('1') {
            "NTSC-J"
        } else if self.regions.contains('4') {
            "NTSC-U"
        } else if self.regions.contains('5') {
            "NTSC"
        } else if self.regions.contains('8') {
            "PAL"
        } else if self.regions.contains('F') {
            "All Consoles"
        } else {
            "Unknown"
        };

        // Display results
        writeln!(f, "Domestic Name: {}", self.domestic_title)?;
        writeln!(f, "Overseas Name: {}", self.overseas_title)?;
        writeln!(f, "Region: {}", region_description)?;
        match publisher_from_copyright(&self.copyright) {
            Some(name) => writeln!(f, "Copyright: {} ({})", self.copyright, name)?,
            None => writeln!(f, "Copyright: {}", self.copyright)?,
        }
        writeln!(f, "Device Support: {}", self.device_support)?;
        writeln!(f, "{}", self as &dyn StoredChecksum<u16>)?;
        writeln!(f, "{}", self as &dyn RomHash)?;

        if let Some(locked_on_game) = &self.lock_on {
            writeln!(f, "+ Lock-On game")?;
            write!(f, "{}", locked_on_game.as_ref())?;
        }

        Ok(())
    }
}

// Sega 32X and Genesis/Mega Drive
pub fn is_32x_rom(buffer: &[u8]) -> bool {
    if buffer.len() < HEADER_MIN_BYTES {
        return false;
    }

    // Console identity string is stored at 0x100..0x10F in the Sega header.
    let console_name = &buffer[CONSOLE_NAME_START..CONSOLE_NAME_END];
    console_name == CONSOLE_NAME_32X
}

pub fn is_genesis_rom(buffer: &[u8]) -> bool {
    if buffer.len() < HEADER_MIN_BYTES {
        return false;
    }

    // Console identity string is stored at 0x100..0x10F in the Sega header.
    let console_name = &buffer[CONSOLE_NAME_START..CONSOLE_NAME_END];
    console_name == CONSOLE_NAME_MEGA_DRIVE || console_name == CONSOLE_NAME_GENESIS
}

#[allow(clippy::arithmetic_side_effects)]
fn parse_sega_rom(raw_buffer: &[u8]) -> io::Result<SegaRomInfo> {
    // Check if the buffer is large enough to contain the header
    if raw_buffer.len() < HEADER_MIN_BYTES {
        return Err(Error::new(
            io::ErrorKind::UnexpectedEof,
            "ROM file is too small to contain a valid Sega ROM header.",
        ));
    }

    // Calculate SHA1 of entire ROM
    let rom_sha1 = compute_sha1(raw_buffer);

    let rom_size: usize =
        (BigEndian::read_u32(&raw_buffer[ROM_SIZE_START..ROM_SIZE_END]) + 1) as usize;

    // Stored checksum field at 0x18E..0x18F.
    let stored_checksum =
        BigEndian::read_u16(&raw_buffer[STORED_CHECKSUM_START..STORED_CHECKSUM_END]);
    let is_sonic_and_knuckles = stored_checksum == SONIC_AND_KNUCKLES_CHECKSUM;
    let buffer = if rom_size < raw_buffer.len() {
        &raw_buffer[..rom_size]
    } else {
        raw_buffer
    };

    // Calculate the checksum of the ROM data from offset 0x200 to the end
    // Read region codes
    Ok(SegaRomInfo {
        copyright: decode_name(&buffer[COPYRIGHT_START..COPYRIGHT_END]),
        domestic_title: decode_name(&buffer[DOMESTIC_TITLE_START..DOMESTIC_TITLE_END]),
        overseas_title: decode_name(&buffer[OVERSEAS_TITLE_START..OVERSEAS_TITLE_END]),
        device_support: decode_name(&buffer[DEVICE_SUPPORT_START..DEVICE_SUPPORT_END]),
        regions: String::from_utf8_lossy(&buffer[REGIONS_START..REGIONS_END])
            .trim_end_matches('\0')
            .to_uppercase(),
        stored_checksum,
        calced_checksum: buffer
            .chunks_exact(2)
            // Cartridge checksum sums 16-bit words from 0x200 to EOF.
            .skip(CHECKSUM_WORD_START)
            .map(BigEndian::read_u16)
            .fold(0u16, |lhs, rhs| lhs.wrapping_add(rhs)),
        lock_on: if is_sonic_and_knuckles && rom_size < raw_buffer.len() {
            match parse_sega_rom(&raw_buffer[rom_size..]) {
                Ok(rom) => Some(rom.into()),
                _ => None,
            }
        } else {
            None
        },
        console: match (is_genesis_rom(buffer), is_32x_rom(buffer)) {
            (false, true) => SegaConsole::X32,
            _ => SegaConsole::Genesis,
        },
        rom_sha1,
    })
}

pub fn verify_sega_checksum(raw_buffer: &[u8]) -> io::Result<SegaRomInfo> {
    let result = parse_sega_rom(raw_buffer);
    if let Ok(rom_info) = &result {
        print!("{}", rom_info);
    }
    result
}

fn decode_name(buffer: &[u8]) -> String {
    let decoded = if let Ok(utf8_str) = String::from_utf8(buffer.to_vec()) {
        utf8_str
    } else {
        let (result, _encoding, errors) = SHIFT_JIS.decode(buffer);
        if !errors {
            result.into_owned()
        } else {
            String::from_utf8_lossy(buffer).into_owned()
        }
    };

    decoded
        .nfkc()
        .collect::<String>()
        .trim_matches(|ch| ch == ' ' || ch == '\0')
        .to_owned()
}
