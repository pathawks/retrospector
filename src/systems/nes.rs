// References:
//   iNES and NES 2.0 header format:
//     https://www.nesdev.org/wiki/INES
//     https://www.nesdev.org/wiki/NES_2.0
//   PRG/CHR ROM and RAM sizes:
//     https://www.nesdev.org/wiki/NES_2.0#PRG-ROM_Area
//     https://www.nesdev.org/wiki/NES_2.0#CHR-ROM_Area

use super::helpers::compute_sha1;
use crate::traits::error::ParseError;
use crate::traits::rom_hash::RomHash;
use crate::traits::rominfo::RomInfo;
pub mod analysis;
pub mod console_type;
pub mod expansion_device;
pub mod file_type;
pub mod mapper;
pub mod mirror_type;
pub mod timing;

use analysis::NesAnalysis;
use console_type::ConsoleType;
use crc::{CRC_32_ISO_HDLC, Crc};
use expansion_device::ExpansionDevice;
use file_type::FileType;
use mapper::Mapper;
use mirror_type::MirrorType;
use std::convert::TryFrom;
use std::fmt;
use std::num::NonZeroU16;
use timing::Timing;

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct NesRomInfo {
    file_type: FileType,
    prg_rom_size: u16,
    chr_rom_size: Option<NonZeroU16>,
    prg_ram_size: u32,
    prg_nvram_size: u32,
    chr_ram_size: u32,
    chr_nvram_size: u32,
    mirror_type: MirrorType,
    console_type: ConsoleType,
    battery_present: bool,
    trainer_present: bool,
    hard_wired_four_screen_mode: bool,
    mapper: Mapper,
    submapper_number: u8,
    ppu_type: u16,
    hardware_type: u16,
    number_of_misc_roms: u8,
    default_expansion_device: ExpansionDevice,
    timing: Timing,
    rom_sha1: [u8; 20],
    prg_crc32: u32,
    chr_crc32: Option<u32>,
    analysis: NesAnalysis,
}

pub trait NesHeader {
    fn prg_rom_bytes(&self) -> usize;
    fn chr_rom_bytes(&self) -> Option<usize>;
}

impl NesHeader for NesRomInfo {
    #[allow(clippy::arithmetic_side_effects)]
    fn prg_rom_bytes(&self) -> usize {
        16usize * 1024usize * (self.prg_rom_size as usize)
    }

    #[allow(clippy::arithmetic_side_effects)]
    fn chr_rom_bytes(&self) -> Option<usize> {
        self.chr_rom_size
            .map(|v| 8usize * 1024usize * (v.get() as usize))
    }
}

impl RomInfo for NesRomInfo {
    fn console(&self) -> &'static str {
        "Nintendo Entertainment System (NES)"
    }
}

impl RomHash for NesRomInfo {
    fn sha1(&self) -> [u8; 20] {
        self.rom_sha1
    }
}

impl TryFrom<&[u8]> for NesRomInfo {
    type Error = ParseError;

    #[allow(clippy::arithmetic_side_effects)]
    fn try_from(buffer: &[u8]) -> Result<Self, Self::Error> {
        if buffer.len() <= 16 {
            return Err(ParseError::BufferTooSmall);
        }

        let file_type = FileType::try_from(buffer)?;

        // Calculate SHA1 of ROM data (everything after the 16-byte header)
        let rom_sha1 = compute_sha1(&buffer[16..]);

        let is_nes20 = file_type == FileType::NES20;
        let trainer_present = check_bit_flag(2)(buffer[6]);

        // NES 2.0 uses extended size in byte 9, regular iNES just uses bytes 4-5
        let prg_rom_size = if is_nes20 {
            char_to_u12(buffer[9] & 0x0F, buffer[4])
        } else {
            buffer[4] as u16
        };
        let chr_rom_size = NonZeroU16::new(if is_nes20 {
            char_to_u12(buffer[9] >> 4, buffer[5])
        } else {
            buffer[5] as u16
        });

        // Calculate PRG and CHR offsets, clamping to actual buffer size
        let prg_offset = 16 + if trainer_present { 512 } else { 0 };
        let prg_bytes = 16384 * prg_rom_size as usize;
        let chr_offset = (prg_offset + prg_bytes).min(buffer.len());
        let chr_bytes = chr_rom_size.map(|s| 8192 * s.get() as usize).unwrap_or(0);
        let chr_end = (chr_offset + chr_bytes).min(buffer.len());

        // Calculate CRC32 for PRG-ROM (using available data)
        let crc = Crc::<u32>::new(&CRC_32_ISO_HDLC);
        let prg_crc32 = if prg_offset < buffer.len() {
            crc.checksum(&buffer[prg_offset..chr_offset])
        } else {
            0
        };

        // Calculate CRC32 for CHR-ROM (if present and data available)
        let chr_crc32 = chr_rom_size.and_then(|_| {
            if chr_offset < buffer.len() {
                Some(crc.checksum(&buffer[chr_offset..chr_end]))
            } else {
                None
            }
        });

        // Extract data slices for analysis
        let prg_data = if prg_offset < buffer.len() {
            &buffer[prg_offset..chr_offset]
        } else {
            &[]
        };
        let chr_data = if chr_rom_size.is_some() && chr_offset < buffer.len() {
            Some(&buffer[chr_offset..chr_end])
        } else {
            None
        };

        let mapper: Mapper = char_to_u12(buffer[8], buffer[6] >> 4 | buffer[7] & 0xf0).into();
        let rom_analysis = analysis::analyze(prg_data, chr_data, &mapper);

        let header = NesRomInfo {
            file_type,
            prg_rom_size,
            chr_rom_size,
            prg_ram_size: if is_nes20 {
                nes20_ram_size(buffer[10] & 0x0F)
            } else if buffer[8] > 0 {
                buffer[8] as u32 * 8192
            } else {
                0
            },
            prg_nvram_size: if is_nes20 {
                nes20_ram_size(buffer[10] >> 4)
            } else {
                0
            },
            chr_ram_size: if is_nes20 {
                nes20_ram_size(buffer[11] & 0x0F)
            } else {
                0
            },
            chr_nvram_size: if is_nes20 {
                nes20_ram_size(buffer[11] >> 4)
            } else {
                0
            },
            mirror_type: buffer[6].into(),
            console_type: buffer[7].into(),
            battery_present: check_bit_flag(1)(buffer[6]),
            trainer_present,
            hard_wired_four_screen_mode: check_bit_flag(3)(buffer[6]),
            timing: buffer[12].into(),
            default_expansion_device: buffer[15].into(),
            mapper,
            submapper_number: if is_nes20 { buffer[8] >> 4 } else { 0 },
            ppu_type: if is_nes20 {
                (buffer[13] & 0x0F) as u16
            } else {
                0
            },
            hardware_type: if is_nes20 {
                (buffer[13] >> 4) as u16
            } else {
                0
            },
            number_of_misc_roms: if is_nes20 { buffer[14] & 0x03 } else { 0 },
            rom_sha1,
            prg_crc32,
            chr_crc32,
            analysis: rom_analysis,
        };
        Ok(header)
    }
}

impl std::fmt::Display for NesRomInfo {
    #[allow(clippy::arithmetic_side_effects)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.file_type != FileType::Raw {
            writeln!(f, "File Type: {}", self.file_type)?;
        }

        writeln!(
            f,
            "Console Type: {}\tTiming: {}",
            self.console_type, self.timing
        )?;

        if self.mapper != Mapper::Unrecognized {
            write!(f, "Mapper: {}", self.mapper)?;
        }
        if self.submapper_number != 0 {
            write!(f, " (submapper {})", self.submapper_number)?;
        }
        write!(
            f,
            "\nPRG-ROM: {:3}kb\tCRC32: {:08X}",
            16usize * <u16 as Into<usize>>::into(self.prg_rom_size),
            self.prg_crc32
        )?;
        if let Some(chr_rom_size) = self.chr_rom_size {
            write!(
                f,
                "\nCHR-ROM: {:3}kb\tCRC32: {:08X}",
                8usize * <u16 as Into<usize>>::into(chr_rom_size.get()),
                self.chr_crc32.unwrap_or(0)
            )?;
        }
        if self.prg_ram_size > 0 {
            write!(f, "\tPRG-RAM: {:3}kb", self.prg_ram_size / 1024)?;
        }
        if self.prg_nvram_size > 0 {
            write!(f, "\tPRG-NVRAM: {:3}kb", self.prg_nvram_size / 1024)?;
        }
        if self.chr_ram_size > 0 {
            write!(f, "\tCHR-RAM: {:3}kb", self.chr_ram_size / 1024)?;
        }
        if self.chr_nvram_size > 0 {
            write!(f, "\tCHR-NVRAM: {:3}kb", self.chr_nvram_size / 1024)?;
        }
        if self.battery_present {
            write!(f, "\n\t* Battery backup")?;
        }
        if self.trainer_present {
            write!(f, "\n\t* Trainer present")?;
        }
        write!(f, "\nMirror Type: {}", self.mirror_type)?;
        if self.console_type == ConsoleType::VsSystem {
            if self.ppu_type != 0 {
                write!(f, "\nVs. PPU Type: {}", self.ppu_type)?;
            }
            if self.hardware_type != 0 {
                write!(f, "\nVs. Hardware Type: {}", self.hardware_type)?;
            }
        }
        if self.number_of_misc_roms > 0 {
            write!(f, "\nMiscellaneous ROMs: {}", self.number_of_misc_roms)?;
        }
        if self.default_expansion_device != ExpansionDevice::Unspecified {
            write!(
                f,
                "\nDefault Expansion Device: {}",
                self.default_expansion_device
            )?;
        }
        write!(f, "\n{}", self as &dyn RomHash)?;
        if let Some(prg_unique) = self.analysis.prg_unique_bytes {
            write!(
                f,
                "\n\t* PRG ROM appears overdumped: {}kb declared, likely {}kb unique data",
                self.prg_rom_bytes() / 1024,
                prg_unique / 1024
            )?;
        }
        if let Some(chr_unique) = self.analysis.chr_unique_bytes {
            write!(
                f,
                "\n\t* CHR ROM appears overdumped: {}kb declared, likely {}kb unique data",
                self.chr_rom_bytes().unwrap_or(0) / 1024,
                chr_unique / 1024
            )?;
        }
        for warning in &self.analysis.warnings {
            write!(f, "\n\t* {}", warning)?;
        }
        Ok(())
    }
}

#[allow(clippy::arithmetic_side_effects)]
fn nes20_ram_size(shift_count: u8) -> u32 {
    if shift_count == 0 {
        0
    } else {
        64 << shift_count
    }
}

#[allow(clippy::arithmetic_side_effects)]
fn char_to_u12(hi: u8, lo: u8) -> u16 {
    if hi == 0x0f {
        let m: u16 = (lo & 0b0000_0011).into();
        let e = (lo & 0b1111_1100) >> 2;
        (1 << e) * (m * 2 + 1)
    } else {
        u16::from_be_bytes([hi & 0x0f, lo])
    }
}

#[inline]
const fn check_bit_flag(pos: u32) -> impl Fn(u8) -> bool {
    let mask = 1u8 << pos;
    move |value: u8| value & mask != 0
}
