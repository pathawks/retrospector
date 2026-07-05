// References:
//   Game Boy cartridge header:
//     https://gbdev.io/pandocs/The_Cartridge_Header.html
//   Nintendo logo and boot sequence:
//     https://gbdev.io/pandocs/Power_Up_Sequence.html
//   Cartridge types (MBC1, MBC2, MBC3, MBC5, etc.):
//     https://gbdev.io/pandocs/MBCs.html

use super::helpers::{compute_sha1, dat_revision, non_empty, read_null_padded_string};
use byte_unit::{Byte, UnitType};
use byteorder::{BigEndian, ByteOrder};

use crate::traits::{
    error::ParseError,
    rom_hash::RomHash,
    rominfo::{DatMeta, RomInfo},
    stored_checksum::StoredChecksum,
    title::Title,
};

mod licensee;

pub use licensee::lookup_new_licensee;
use licensee::lookup_old_licensee;

const MIN_GB_ROM_SIZE: usize = 0x0150;
const LOGO_START: usize = 0x0104;
const LOGO_END: usize = 0x0134;
const TITLE_START: usize = 0x0134;
const TITLE_END: usize = 0x0143;
const CGB_FLAG_OFFSET: usize = 0x0143;
const NEW_LICENSEE_START: usize = 0x0144;
const NEW_LICENSEE_END: usize = 0x0146;
const SGB_FLAG_OFFSET: usize = 0x0146;
const CARTRIDGE_TYPE_OFFSET: usize = 0x0147;
const ROM_SIZE_OFFSET: usize = 0x0148;
const RAM_SIZE_OFFSET: usize = 0x0149;
const DESTINATION_CODE_OFFSET: usize = 0x014A;
const OLD_LICENSEE_OFFSET: usize = 0x014B;
const MASK_ROM_VERSION_OFFSET: usize = 0x014C;
const HEADER_CHECKSUM_OFFSET: usize = 0x014D;
const GLOBAL_CHECKSUM_START: usize = 0x014E;
const GLOBAL_CHECKSUM_END: usize = 0x0150;
const HEADER_CHECKSUM_SEED: u8 = 0xE7;
const CGB_FLAG_DUAL_MODE: u8 = 0x80;
const CGB_FLAG_EXCLUSIVE: u8 = 0xC0;

/// Decode the ROM-size header byte (0x0148) to a total byte count.
/// Codes 0x00..=0x08 double 32 KiB per step; 0x52/0x53/0x54 are the three
/// unofficial 72/80/96-bank sizes. Unknown codes return `None`.
#[allow(clippy::arithmetic_side_effects)]
fn rom_size_bytes(code: u8) -> Option<u64> {
    const BANK: u64 = 0x4000; // 16 KiB
    match code {
        0x00..=0x08 => Some(0x8000u64 << code),
        0x52 => Some(72 * BANK),
        0x53 => Some(80 * BANK),
        0x54 => Some(96 * BANK),
        _ => None,
    }
}

/// Decode the RAM-size header byte (0x0149) to a total byte count.
/// This is a lookup, not a shift: note code 0x05 (64 KiB) is smaller than
/// code 0x04 (128 KiB). Unknown/unused codes return `None`.
fn ram_size_bytes(code: u8) -> Option<u64> {
    match code {
        0x00 => Some(0),
        0x02 => Some(8 * 1024),
        0x03 => Some(32 * 1024),
        0x04 => Some(128 * 1024),
        0x05 => Some(64 * 1024),
        _ => None,
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct GameboyInfo {
    stored_checksum: u16,
    calculated_checksum: u16,
    stored_header_checksum: u8,
    calculated_header_checksum: u8,
    title: String,
    rom_size_code: u8,
    ram_size_code: u8,
    sgb_support: String,
    cgb_support: String,
    region: String,
    cartridge_type: u8,
    old_licensee_code: u8,
    new_licensee_code: [u8; 2],
    mask_rom_version: u8,
    buffer_len: usize,
    rom_sha1: [u8; 20],
}

impl RomInfo for GameboyInfo {
    fn console(&self) -> &'static str {
        "Game Boy"
    }

    fn dat_meta(&self) -> DatMeta {
        // Destination code 0x00 = Japan; anything else is non-specific "Non-Japanese"
        let region = if self.region == "Japan" {
            Some("Japan".to_string())
        } else {
            None
        };

        DatMeta {
            title: non_empty(&self.title),
            region,
            version: dat_revision(self.mask_rom_version),
            ..DatMeta::default()
        }
    }
}

impl std::fmt::Display for GameboyInfo {
    #[allow(clippy::arithmetic_side_effects)]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let cartridge_description = match self.cartridge_type {
            0x00 => "ROM ONLY",
            0x01 => "MBC1",
            0x02 => "MBC1+RAM",
            0x03 => "MBC1+RAM+BATTERY",
            0x05 => "MBC2",
            0x06 => "MBC2+BATTERY",
            0x08 => "ROM+RAM",
            0x09 => "ROM+RAM+BATTERY",
            0x0B => "MMM01",
            0x0C => "MMM01+RAM",
            0x0D => "MMM01+RAM+BATTERY",
            0x0F => "MBC3+TIMER+BATTERY",
            0x10 => "MBC3+TIMER+RAM+BATTERY",
            0x11 => "MBC3",
            0x12 => "MBC3+RAM",
            0x13 => "MBC3+RAM+BATTERY",
            0x19 => "MBC5",
            0x1A => "MBC5+RAM",
            0x1B => "MBC5+RAM+BATTERY",
            0x1C => "MBC5+RUMBLE",
            0x1D => "MBC5+RUMBLE+RAM",
            0x1E => "MBC5+RUMBLE+RAM+BATTERY",
            // Add more mappings as needed
            _ => "Unknown",
        };

        write!(f, "{}", self as &dyn Title)?;
        writeln!(f, "Region: {}", self.region)?;
        if self.mask_rom_version != 0 {
            writeln!(f, "Version: {}", self.mask_rom_version)?;
        }
        if self.old_licensee_code == 0x33 {
            let code = String::from_utf8_lossy(&self.new_licensee_code);
            match lookup_new_licensee(&self.new_licensee_code) {
                Some(name) => writeln!(f, "Maker: {name} (\"{code}\")")?,
                None => writeln!(f, "Maker: \"{code}\"")?,
            }
        } else {
            match lookup_old_licensee(self.old_licensee_code) {
                Some(name) => writeln!(f, "Maker: {name} ({:#04X})", self.old_licensee_code)?,
                None => writeln!(f, "Maker: {:#04X}", self.old_licensee_code)?,
            }
        }
        writeln!(f, "Super Game Boy Support: {}", self.sgb_support)?;
        writeln!(f, "Game Boy Color Support: {}", self.cgb_support)?;
        writeln!(
            f,
            "Cartridge Type: {} ({:#04X})",
            cartridge_description, self.cartridge_type
        )?;
        match rom_size_bytes(self.rom_size_code) {
            Some(bytes) => writeln!(
                f,
                "ROM Size: {}",
                Byte::from(bytes).get_appropriate_unit(UnitType::Both)
            )?,
            None => writeln!(f, "ROM Size: Unknown (0x{:02X})", self.rom_size_code)?,
        }
        match ram_size_bytes(self.ram_size_code) {
            Some(bytes) => writeln!(
                f,
                "RAM Size: {}",
                Byte::from(bytes).get_appropriate_unit(UnitType::Both)
            )?,
            None => writeln!(f, "RAM Size: Unknown (0x{:02X})", self.ram_size_code)?,
        }
        writeln!(f, "Header Checksum: {:02X}", self.stored_header_checksum)?;
        if self.stored_header_checksum != self.calculated_header_checksum {
            writeln!(f, "Invalid header checksum")?
        }

        writeln!(f, "{}", self as &dyn StoredChecksum<u16>)?;
        writeln!(f, "{}", self as &dyn RomHash)?;

        if let Some(rom_len) = rom_size_bytes(self.rom_size_code).map(|b| b as usize)
            && rom_len < self.buffer_len
        {
            writeln!(
                f,
                "Possible overdump? {} extra bytes at end",
                self.buffer_len.saturating_sub(rom_len)
            )?;
        };

        Ok(())
    }
}

impl StoredChecksum<u16> for GameboyInfo {
    fn stored_checksum(&self) -> u16 {
        self.stored_checksum
    }

    fn calculated_checksum(&self) -> u16 {
        self.calculated_checksum
    }
}

impl RomHash for GameboyInfo {
    fn sha1(&self) -> [u8; 20] {
        self.rom_sha1
    }
}

impl Title for GameboyInfo {
    fn title(&self) -> &str {
        &self.title
    }
}

/// Expected first byte and checksum of the 48-byte logo bitmap at 0x0104–0x0133.
/// We verify the logo by checksumming rather than embedding the copyrighted bitmap.
const LOGO_FIRST_BYTE: u8 = 0xCE;
const LOGO_CHECKSUM: u16 = 0x1546;

// Game Boy and Game Boy Color
pub fn is_gb_rom(buffer: &[u8]) -> bool {
    if buffer.len() < MIN_GB_ROM_SIZE {
        return false;
    }

    verify_logo(buffer) && verify_gb_header_checksum(buffer)
}

/// Verify the logo region by checking the first byte (fast reject) then
/// summing all 48 bytes against a known checksum.
fn verify_logo(buffer: &[u8]) -> bool {
    let logo = &buffer[LOGO_START..LOGO_END];
    if logo[0] != LOGO_FIRST_BYTE {
        return false;
    }
    let sum: u16 = logo.iter().map(|&b| b as u16).sum();
    sum == LOGO_CHECKSUM
}

pub fn is_gbc_rom(buffer: &[u8]) -> bool {
    is_gb_rom(buffer)
        && (buffer[CGB_FLAG_OFFSET] == CGB_FLAG_DUAL_MODE
            || buffer[CGB_FLAG_OFFSET] == CGB_FLAG_EXCLUSIVE)
}

impl TryFrom<&[u8]> for GameboyInfo {
    type Error = ParseError;

    fn try_from(buffer: &[u8]) -> Result<Self, Self::Error> {
        if !is_gb_rom(buffer) {
            return Err(ParseError::MagicNotFound);
        }

        let rom_sha1 = compute_sha1(buffer);

        let calculated_header_checksum = buffer[TITLE_START..HEADER_CHECKSUM_OFFSET]
            .iter()
            .fold(HEADER_CHECKSUM_SEED, |lhs, &rhs| lhs.wrapping_sub(rhs));

        // Read stored checksums
        let stored_header_checksum = buffer[HEADER_CHECKSUM_OFFSET];

        // Read the SGB flag
        let sgb_flag = buffer[SGB_FLAG_OFFSET];
        let sgb_support = match sgb_flag {
            3 => "Yes",
            _ => "No",
        };

        // Read the CGB flag
        let cgb_flag = buffer[CGB_FLAG_OFFSET];
        let cgb_support = match cgb_flag {
            CGB_FLAG_DUAL_MODE => "Yes (Compatible with GB and GBC)",
            CGB_FLAG_EXCLUSIVE => "Yes (GBC Exclusive)",
            _ => "No",
        };

        // Read the game title
        let game_title = read_null_padded_string(&buffer[TITLE_START..TITLE_END]);

        // Read the Cartridge Type
        let cartridge_type = buffer[CARTRIDGE_TYPE_OFFSET];

        // Read ROM and RAM size codes (decoded via lookup, not a bit shift)
        let rom_size_code = buffer[ROM_SIZE_OFFSET];
        let ram_size_code = buffer[RAM_SIZE_OFFSET];

        // Read the destination code
        let destination_code = buffer[DESTINATION_CODE_OFFSET];
        let region = match destination_code {
            0x00 => "Japan",
            0x01 => "Non-Japanese",
            _ => "Unknown",
        };

        let stored_checksum =
            BigEndian::read_u16(&buffer[GLOBAL_CHECKSUM_START..GLOBAL_CHECKSUM_END]);

        let check_seed = stored_checksum
            .to_ne_bytes()
            .iter()
            .fold(0u16, |lhs, &rhs| lhs.wrapping_add(rhs as u16))
            .wrapping_neg();

        let gameboy_info = GameboyInfo {
            stored_checksum,
            calculated_checksum: buffer
                .iter()
                .fold(check_seed, |lhs, &rhs| lhs.wrapping_add(rhs as u16)),
            title: game_title,
            stored_header_checksum,
            calculated_header_checksum,
            rom_size_code,
            ram_size_code,
            sgb_support: sgb_support.to_owned(),
            cgb_support: cgb_support.to_owned(),
            region: region.to_owned(),
            cartridge_type,
            old_licensee_code: buffer[OLD_LICENSEE_OFFSET],
            new_licensee_code: buffer[NEW_LICENSEE_START..NEW_LICENSEE_END]
                .try_into()
                .map_err(|_| ParseError::InvalidHeader)?,
            mask_rom_version: buffer[MASK_ROM_VERSION_OFFSET],
            buffer_len: buffer.len(),
            rom_sha1,
        };
        Ok(gameboy_info)
    }
}

fn verify_gb_header_checksum(buffer: &[u8]) -> bool {
    let stored_checksum = buffer[HEADER_CHECKSUM_OFFSET];

    let calculated_checksum = buffer[TITLE_START..HEADER_CHECKSUM_OFFSET]
        .iter()
        .fold(HEADER_CHECKSUM_SEED, |lhs, &rhs| lhs.wrapping_sub(rhs));
    calculated_checksum == stored_checksum
}
