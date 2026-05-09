// References:
//   SNES ROM header and registration data:
//     https://snes.nesdev.org/wiki/ROM_header
//   SNES memory mapping modes:
//     https://snes.nesdev.org/wiki/Memory_map
//   Super NES cart header technical details:
//     https://problemkaputt.de/fullsnes.htm#snescartridgeromheader

use super::helpers::{
    compute_sha1, dat_revision, detect_trailing_padding, detect_unique_size, non_empty,
};
use byte_unit::{Byte, UnitType};
use byteorder::{ByteOrder, LittleEndian};

use crate::traits::{
    error::ParseError,
    rom_hash::RomHash,
    rominfo::{DatMeta, RomInfo},
    stored_checksum::StoredChecksum,
    title::Title,
};

mod detection;
use detection::*;

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct SNESRomInfo {
    pub game_title: String,
    pub revision: u8,
    pub cartridge_type: u8,
    pub rom_size: Byte,
    pub file_size: Byte,
    pub region: u8,
    pub rom_speed: u8,
    pub map_mode: u8,
    pub stored_checksum: u16,
    pub calculated_checksum: u16,
    pub trimmed_checksum: Option<u16>,
    pub rom_sha1: [u8; 20],
    pub unique_rom_bytes: Option<usize>,
    pub game_code: Option<[u8; 4]>,
}

fn format_bits(bits: u64) -> String {
    const MIBIT: u64 = 1024 * 1024;
    const KIBIT: u64 = 1024;
    if bits >= MIBIT && bits.is_multiple_of(MIBIT) {
        format!("{} Mibit", bits / MIBIT)
    } else if bits >= MIBIT {
        format!("{:.2} Mibit", bits as f64 / MIBIT as f64)
    } else if bits >= KIBIT && bits.is_multiple_of(KIBIT) {
        format!("{} Kibit", bits / KIBIT)
    } else {
        format!("{} bit", bits)
    }
}

impl RomInfo for SNESRomInfo {
    fn console(&self) -> &'static str {
        "Super Nintendo/Super Famicom"
    }

    fn dat_meta(&self) -> DatMeta {
        let region = match self.region {
            0x00 => Some("Japan"),
            0x01 => Some("USA"),
            0x02 => Some("Europe"),
            0x03 => Some("Sweden"),
            0x04 => Some("Finland"),
            0x05 => Some("Denmark"),
            0x06 => Some("France"),
            0x07 => Some("Netherlands"),
            0x08 => Some("Spain"),
            0x09 => Some("Germany"),
            0x0A => Some("Italy"),
            0x0B | 0x0C => Some("Asia"),
            0x0D => Some("Korea"),
            0x0F => Some("World"),
            _ => None,
        }
        .map(String::from);

        let serial = self
            .game_code
            .and_then(|gc| String::from_utf8(gc.to_vec()).ok());

        DatMeta {
            title: non_empty(&self.game_title),
            region,
            version: dat_revision(self.revision),
            serial: serial.as_deref().and_then(non_empty),
            machine_id: serial.as_deref().and_then(non_empty),
            ..DatMeta::default()
        }
    }
}

impl TryFrom<&[u8]> for SNESRomInfo {
    type Error = ParseError;

    #[allow(clippy::arithmetic_side_effects)]
    fn try_from(buffer: &[u8]) -> Result<Self, Self::Error> {
        let header_location =
            detect_header_offset(buffer).map_err(|_| ParseError::MagicNotFound)?;

        let offset = if has_copier_trainer(buffer) {
            TRAINER_BYTES
        } else {
            0
        };

        // Calculate SHA1 of ROM data (excluding trainer/copier header if present)
        let rom_sha1 = compute_sha1(&buffer[offset..]);

        // Calculate effective ROM size - use file size if header byte is invalid
        let header_rom_size = decoded_rom_size(buffer[header_location + HEADER_ROM_SIZE_OFFSET]);
        let file_rom_size = buffer.len() - offset;
        let rom_size = effective_rom_size(header_rom_size, file_rom_size);

        // Detect overdumps: duplication (mirrored data) or trailing padding (0x00/0xFF fill)
        let rom_data = &buffer[offset..];
        let unique_rom_bytes = detect_unique_size(rom_data, SNES_ROM_MIN_BYTES)
            .or_else(|| detect_trailing_padding(rom_data, SNES_ROM_MIN_BYTES));
        let _ram_size = decoded_rom_size(buffer[header_location + HEADER_RAM_SIZE_OFFSET]);

        let calculated_checksum = !snes_checksum(rom_data, rom_size);
        let trimmed_checksum = unique_rom_bytes.map(|unique| {
            let trimmed_rom_size = effective_rom_size(header_rom_size, unique);
            !snes_checksum(&rom_data[..unique], trimmed_rom_size)
        });

        // Read extended header game code (4 bytes at header_base - 0x0E)
        let game_code = if header_location >= HEADER_GAME_CODE_OFFSET {
            let gc_offset = header_location - HEADER_GAME_CODE_OFFSET;
            buffer
                .get(gc_offset..gc_offset + 4)
                .filter(|gc| {
                    gc.iter()
                        .all(|&b| b.is_ascii_uppercase() || b.is_ascii_digit())
                })
                .map(|gc| [gc[0], gc[1], gc[2], gc[3]])
        } else {
            None
        };

        // Read the game title
        let game_title = buffer
            .iter()
            .skip(header_location)
            .take(HEADER_TITLE_LEN)
            .map(|&b| b as char)
            .take_while(char::is_ascii)
            .collect::<String>()
            .trim_end_matches(char::is_whitespace)
            .to_string();

        // Display results
        let snes_info = SNESRomInfo {
            game_title,
            rom_speed: (buffer[header_location + HEADER_MAP_MODE_OFFSET] & MAP_MODE_FASTROM_BIT)
                >> 4,
            map_mode: buffer[header_location + HEADER_MAP_MODE_OFFSET] & MAP_MODE_MASK,
            cartridge_type: buffer[header_location + HEADER_CART_TYPE_OFFSET],
            region: buffer[header_location + HEADER_REGION_OFFSET],
            revision: buffer[header_location + HEADER_REVISION_OFFSET],
            file_size: Byte::from_u64(buffer.len() as u64),
            stored_checksum: !LittleEndian::read_u16(
                &buffer[header_location + HEADER_CHECKSUM_OFFSET
                    ..header_location + HEADER_CHECKSUM_OFFSET + HEADER_VECTOR_BYTES],
            ),
            calculated_checksum,
            rom_size: Byte::from_u64(rom_size as u64),
            trimmed_checksum,
            rom_sha1,
            unique_rom_bytes,
            game_code,
        };

        Ok(snes_info)
    }
}

impl StoredChecksum<u16> for SNESRomInfo {
    fn stored_checksum(&self) -> u16 {
        self.stored_checksum
    }

    fn calculated_checksum(&self) -> u16 {
        self.calculated_checksum
    }
}

impl RomHash for SNESRomInfo {
    fn sha1(&self) -> [u8; 20] {
        self.rom_sha1
    }
}

impl Title for SNESRomInfo {
    fn title(&self) -> &str {
        &self.game_title
    }
}

impl std::fmt::Display for SNESRomInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let cartridge_description = match &self.cartridge_type {
            0x00 => "ROM Only",
            0x01 => "ROM + RAM",
            0x02 => "ROM + RAM + Battery",
            0x03 => "ROM + Enhancement Chip",
            0x13 => "ROM + Super FX",
            0x1A => "ROM + SA-1",
            0x1B => "ROM + SA-1 + RAM",
            // Add more mappings as needed
            _ => "Unknown",
        };

        let region = match &self.region {
            0x00 => "Japan",
            0x01 => "North America",
            0x02 => "Europe",
            0x03 => "Sweden",
            0x04 => "Finland",
            0x05 => "Denmark",
            0x06 => "France",
            0x07 => "Holland",
            0x08 => "Spain",
            0x09 => "Germany",
            0x0A => "Italy",
            0x0B => "Hong Kong",
            0x0C => "Indonesia",
            0x0D => "South Korea",
            0x0E => "Unknown",
            0x0F => "Global",
            _ => "Unknown",
        };

        // Determine the mapping mode
        let map_mode_description = match &self.map_mode {
            0x00 => "LoROM",
            0x01 => "HiROM",
            0x02 => "SA-1 ROM",
            0x03 => "ExLoROM",
            0x05 => "ExHiROM",
            _ => "Unknown",
        };

        let rom_speed_description = match &self.rom_speed {
            1 => "FastROM (3.58 MHz)",
            _ => "SlowROM (2.68 MHz)",
        };

        let trainer_detected = self
            .rom_size
            .add((TRAINER_BYTES as u64).into())
            .unwrap_or(0u64.into())
            == self.file_size;

        write!(f, "{}", self as &dyn Title)?;
        writeln!(f, "Region: {}", region)?;
        writeln!(f, "Revision: {}", &self.revision)?;
        writeln!(
            f,
            "Cartridge Type: {} ({:#04X})",
            cartridge_description, &self.cartridge_type
        )?;
        writeln!(f, "ROM Speed: {}", rom_speed_description)?;
        writeln!(f, "Map Mode: {}", map_mode_description)?;
        writeln!(
            f,
            "ROM Size:  {}",
            &self.rom_size.get_appropriate_unit(UnitType::Both)
        )?;
        writeln!(
            f,
            "File Size: {}",
            &self.file_size.get_appropriate_unit(UnitType::Both)
        )?;

        // Verify checksum
        writeln!(f, "{}", self as &dyn StoredChecksum<u16>)?;
        writeln!(f, "{}", self as &dyn RomHash)?;

        if trainer_detected {
            writeln!(
                f,
                "Possible Trainer Detected ({TRAINER_BYTES} extra bytes in file)",
            )?;
        }
        if let Some(unique_bytes) = self.unique_rom_bytes
            && self.trimmed_checksum == Some(self.stored_checksum)
        {
            writeln!(
                f,
                "Overdump Detected: file contains {} of unique ROM data ({} file)",
                format_bits((unique_bytes as u64).saturating_mul(8)),
                format_bits(self.file_size.as_u64().saturating_mul(8))
            )?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_power_of_2_rom() {
        // 64KB ROM, declared 64KB — straight sum
        let data: Vec<u8> = (0..65536u32).map(|i| (i % 251) as u8).collect();
        let expected: u16 = data.iter().fold(0u16, |s, &b| s.wrapping_add(b as u16));
        assert_eq!(snes_checksum(&data, data.len()), expected);
    }

    #[test]
    fn checksum_mirrored_3mb_to_4mb() {
        // Simulate 3MB file with 4MB declared: [2MB base][1MB remainder]
        // Remainder should be counted twice to fill 4MB
        let base: Vec<u8> = (0..2 * 1024 * 1024u32).map(|i| (i % 251) as u8).collect();
        let remainder: Vec<u8> = (0..1024 * 1024u32).map(|i| (i % 239) as u8).collect();
        let mut data = base.clone();
        data.extend_from_slice(&remainder);

        let base_sum: u32 = base.iter().map(|&b| b as u32).sum();
        let rem_sum: u32 = remainder.iter().map(|&b| b as u32).sum();
        let expected = (base_sum + rem_sum * 2) as u16;

        assert_eq!(snes_checksum(&data, 4 * 1024 * 1024), expected);
    }

    #[test]
    fn checksum_mirrored_1_5mb_to_2mb() {
        // 1.5MB file with 2MB declared: [1MB base][512KB remainder]
        // Remainder counted twice to fill 2MB
        let base: Vec<u8> = (0..1024 * 1024u32).map(|i| (i % 251) as u8).collect();
        let remainder: Vec<u8> = (0..512 * 1024u32).map(|i| (i % 239) as u8).collect();
        let mut data = base.clone();
        data.extend_from_slice(&remainder);

        let base_sum: u32 = base.iter().map(|&b| b as u32).sum();
        let rem_sum: u32 = remainder.iter().map(|&b| b as u32).sum();
        let expected = (base_sum + rem_sum * 2) as u16;

        assert_eq!(snes_checksum(&data, 2 * 1024 * 1024), expected);
    }

    #[test]
    fn checksum_file_larger_than_declared() {
        // Overdump: 2MB file but header says 1MB — should only sum first 1MB
        let data: Vec<u8> = (0..2 * 1024 * 1024u32).map(|i| (i % 251) as u8).collect();
        let expected: u16 = data[..1024 * 1024]
            .iter()
            .fold(0u16, |s, &b| s.wrapping_add(b as u16));
        assert_eq!(snes_checksum(&data, 1024 * 1024), expected);
    }

    #[test]
    fn detects_2x_snes_overdump() {
        // 64KB unique data repeated to 128KB
        let half: Vec<u8> = (0..SNES_ROM_MIN_BYTES).map(|i| (i % 251) as u8).collect();
        let mut data = half.clone();
        data.extend_from_slice(&half);
        let result = detect_unique_size(&data, SNES_ROM_MIN_BYTES);
        assert_eq!(result, Some(SNES_ROM_MIN_BYTES));
    }

    #[test]
    fn detects_snes_padding_overdump() {
        // 32KB data + 32KB of 0xFF padding
        let real: Vec<u8> = (0..SNES_ROM_MIN_BYTES).map(|i| (i % 251) as u8).collect();
        let mut data = real;
        data.extend(vec![0xFF; SNES_ROM_MIN_BYTES]);
        let unique = detect_unique_size(&data, SNES_ROM_MIN_BYTES);
        let padding = detect_trailing_padding(&data, SNES_ROM_MIN_BYTES);
        // Duplication won't match (different content), but padding will
        assert!(unique.is_none() || padding.is_some());
        assert_eq!(padding, Some(SNES_ROM_MIN_BYTES));
    }

    #[test]
    fn no_snes_overdump_for_unique_data() {
        // 64KB of distinct data — no overdump
        let mut data = vec![0u8; 2 * SNES_ROM_MIN_BYTES];
        for (i, byte) in data.iter_mut().enumerate() {
            *byte = (i.wrapping_mul(2654435761) >> 16) as u8;
        }
        assert_eq!(detect_unique_size(&data, SNES_ROM_MIN_BYTES), None);
        assert_eq!(detect_trailing_padding(&data, SNES_ROM_MIN_BYTES), None);
    }
}
