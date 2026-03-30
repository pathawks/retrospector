// References:
//   N64 ROM header and cartridge format:
//     https://n64brew.dev/wiki/ROM_Header
//   N64 CIC chips and boot checksums:
//     https://n64brew.dev/wiki/CIC-NUS
//   N64 ROM byte ordering (big-endian, little-endian, byte-swapped):
//     https://n64brew.dev/wiki/ROM_Header#Byte_Swap

use super::helpers::compute_sha1;
use crate::traits::{
    error::ParseError,
    rom_hash::RomHash,
    rominfo::{DatMeta, RomInfo},
    stored_checksum::StoredChecksum,
};

use super::helpers::{dat_revision, non_empty};
use byteorder::{BigEndian, ByteOrder, LittleEndian};
use encoding_rs::SHIFT_JIS;
use unicode_normalization::UnicodeNormalization;

const FORMAT_MAGIC_LEN: usize = 4;
const MAGIC_BIG_ENDIAN: u32 = 0x80_37_12_40;
const MAGIC_LITTLE_ENDIAN: u32 = 0x40_12_37_80;
const MAGIC_BYTE_SWAPPED: u32 = 0x37_80_40_12;

const CRC1_START: usize = 0x10;
const CRC1_END: usize = 0x14;
const CRC2_START: usize = 0x14;
const CRC2_END: usize = 0x18;
const TITLE_START: usize = 0x20;
const TITLE_END: usize = 0x34;
const MEDIA_OFFSET: usize = 0x3B;
const STORED_FORMAT_START: usize = 0x3C;
const STORED_FORMAT_END: usize = 0x3F;
const COUNTRY_CODE_OFFSET: usize = 0x3E;
const REVISION_OFFSET: usize = 0x3F;

const CRC_MIN_ROM_SIZE: usize = 0x1000;
const CIC_SELECTOR_OFFSET: usize = 0x29B;
const CIC_6105: u8 = 0x1C;
const CIC_6103: u8 = 0x8D;
const CIC_6106: u8 = 0x9E;
const CIC_6105_SEED: u32 = 0xDF26F436;
const CIC_6103_SEED: u32 = 0xA3886759;
const CIC_6106_SEED: u32 = 0x1FEA617A;
const CIC_DEFAULT_SEED: u32 = 0xF8CA4DDC;
const CRC_DATA_START: usize = 0x1000;
const CRC_DATA_LEN: usize = 0x100000;
const CIC_6105_TABLE_BASE: usize = 0x0750;

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
pub enum N64Format {
    #[default]
    BigEndian,
    LittleEndian,
    ByteSwapped,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum N64ParseError {
    BufferTooSmall { minimum: usize },
    UnknownFormat,
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct N64RomInfo {
    pub rom_format: N64Format,
    pub stored_title: String,
    pub stored_format: String,
    pub media: char,
    pub country_code: u8,
    pub crc1: u32,
    pub crc2: u32,
    pub calculated_crc1: u32,
    pub calculated_crc2: u32,
    pub revision: u8,
    pub rom_sha1: [u8; 20],
}

impl StoredChecksum<(u32, u32)> for N64RomInfo {
    fn stored_checksum(&self) -> (u32, u32) {
        (self.crc1, self.crc2)
    }

    fn calculated_checksum(&self) -> (u32, u32) {
        (self.calculated_crc1, self.calculated_crc2)
    }
}

impl RomHash for N64RomInfo {
    fn sha1(&self) -> [u8; 20] {
        self.rom_sha1
    }
}

impl RomInfo for N64RomInfo {
    fn console(&self) -> &'static str {
        "Nintendo 64"
    }

    fn dat_meta(&self) -> DatMeta {
        let region = match self.country_code {
            0x41 => Some("Asia"),
            0x42 => Some("Brazil"),
            0x43 => Some("China"),
            0x44 => Some("Germany"),
            0x45 => Some("USA"),
            0x46 => Some("France"),
            0x48 => Some("Netherlands"),
            0x49 => Some("Italy"),
            0x4A => Some("Japan"),
            0x4B => Some("Korea"),
            0x4E => Some("Canada"),
            0x50 | 0x58 | 0x59 => Some("Europe"),
            0x53 => Some("Spain"),
            0x55 => Some("Australia"),
            0x57 => Some("Scandinavia"),
            _ => None,
        }
        .map(String::from);

        let serial = format!("{}{}", self.media, self.stored_format);

        DatMeta {
            title: non_empty(&self.stored_title),
            region,
            version: dat_revision(self.revision),
            serial: non_empty(&serial),
            machine_id: non_empty(&serial),
            ..DatMeta::default()
        }
    }
}

impl std::fmt::Display for N64RomInfo {
    #[allow(clippy::just_underscores_and_digits)]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Stored title: {}", self.stored_title)?;
        writeln!(f, "Stored Id: {}{}", self.media, self.stored_format)?;
        writeln!(
            f,
            "Region Code: {}",
            match self.country_code {
                0x37 => "Beta",
                0x41 => "Asia (NTSC)",
                0x42 => "Brazil",
                0x43 => "China",
                0x44 => "Germany",
                0x45 => "North America",
                0x46 => "French",
                0x47 => "Gateway 64 (NTSC)",
                0x48 => "Dutch",
                0x49 => "Italy",
                0x4A => "Japan",
                0x4B => "Korea",
                0x4C => "Gateway 64 (PAL)",
                0x4E => "Canada",
                0x50 => "Europe (basic spec.)",
                0x53 => "Spain",
                0x55 => "Australia",
                0x57 => "Scandinavia",
                0x58 => "Europe",
                0x59 => "Europe",
                ____ => "Unknown",
            }
        )?;
        writeln!(f, "Revision: {}", self.revision)?;
        writeln!(
            f,
            "Media Format: {}",
            match self.media {
                'N' => "Cartridge",
                'D' => "64DD disk",
                'C' => "Cartridge part of expandable game",
                'E' => "64DD expansion for cart",
                'Z' => "Aleck64 cart",
                ___ => "Unknown",
            }
        )?;
        writeln!(
            f,
            "Canonical file name: {}.{}{}.{}",
            self.stored_title
                .replace(" ", "")
                .replace(":", "")
                .replace("'", ""),
            self.media,
            self.stored_format,
            match self.rom_format {
                N64Format::BigEndian => "z64",
                N64Format::LittleEndian => "n64",
                N64Format::ByteSwapped => "v64",
            }
        )?;

        // Verify checksums
        writeln!(f, "{}", self as &dyn StoredChecksum<(u32, u32)>)?;
        writeln!(f, "{}", self as &dyn RomHash)?;

        Ok(())
    }
}

impl TryFrom<&[u8]> for N64RomInfo {
    type Error = ParseError;

    #[allow(clippy::arithmetic_side_effects)]
    fn try_from(raw_buffer: &[u8]) -> Result<Self, Self::Error> {
        if !is_n64_rom(raw_buffer) {
            return Err(ParseError::MagicNotFound);
        }
        let format = detect_n64_format(raw_buffer).map_err(|e| match e {
            N64ParseError::BufferTooSmall { .. } => ParseError::BufferTooSmall,
            N64ParseError::UnknownFormat => ParseError::MagicNotFound,
        })?;
        let buffer = correct_n64_byte_order(raw_buffer, format);

        // Calculate SHA1 of big-endian ROM data (for No-Intro compatibility)
        let rom_sha1 = compute_sha1(&buffer);

        let stored_title = decode_n64_title(&buffer[TITLE_START..TITLE_END]);
        let media = buffer[MEDIA_OFFSET];
        let country_code = buffer[COUNTRY_CODE_OFFSET];
        let revision = buffer[REVISION_OFFSET];
        let (calculated_crc1, calculated_crc2) =
            calculate_n64_crc(&buffer).map_err(|_| ParseError::BufferTooSmall)?;
        let stored_format =
            String::from_utf8_lossy(&buffer[STORED_FORMAT_START..STORED_FORMAT_END]).to_string();

        // Calculate the checksum
        let rom_info = N64RomInfo {
            rom_format: format,
            stored_format,
            stored_title,
            media: media as char,
            country_code,
            crc1: BigEndian::read_u32(&buffer[CRC1_START..CRC1_END]),
            crc2: BigEndian::read_u32(&buffer[CRC2_START..CRC2_END]),
            calculated_crc1,
            calculated_crc2,
            revision,
            rom_sha1,
        };
        Ok(rom_info)
    }
}

pub fn is_n64_rom(buffer: &[u8]) -> bool {
    detect_n64_format(buffer).is_ok()
}

pub fn detect_n64_format(buffer: &[u8]) -> Result<N64Format, N64ParseError> {
    if buffer.len() < FORMAT_MAGIC_LEN {
        return Err(N64ParseError::BufferTooSmall {
            minimum: FORMAT_MAGIC_LEN,
        });
    }

    match BigEndian::read_u32(&buffer[..FORMAT_MAGIC_LEN]) {
        MAGIC_BIG_ENDIAN => Ok(N64Format::BigEndian),
        MAGIC_LITTLE_ENDIAN => Ok(N64Format::LittleEndian),
        MAGIC_BYTE_SWAPPED => Ok(N64Format::ByteSwapped),
        _ => Err(N64ParseError::UnknownFormat),
    }
}

pub fn correct_n64_byte_order(buffer: &[u8], format: N64Format) -> Vec<u8> {
    match format {
        N64Format::BigEndian => buffer.to_vec(),
        N64Format::LittleEndian => buffer
            .chunks_exact(4)
            .map(LittleEndian::read_u32)
            .flat_map(u32::to_be_bytes)
            .collect(),
        N64Format::ByteSwapped => buffer
            .chunks_exact(2)
            .map(LittleEndian::read_u16)
            .flat_map(u16::to_be_bytes)
            .collect(),
    }
}

fn decode_n64_title(buffer: &[u8]) -> String {
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

#[allow(clippy::arithmetic_side_effects)]
fn calculate_n64_crc(buffer: &[u8]) -> Result<(u32, u32), N64ParseError> {
    if buffer.len() < CRC_MIN_ROM_SIZE {
        return Err(N64ParseError::BufferTooSmall {
            minimum: CRC_MIN_ROM_SIZE,
        });
    }

    let cic_selector = buffer[CIC_SELECTOR_OFFSET];
    let seed: u32 = match cic_selector {
        CIC_6105 => CIC_6105_SEED,
        CIC_6103 => CIC_6103_SEED,
        CIC_6106 => CIC_6106_SEED,
        _ => CIC_DEFAULT_SEED,
    };
    let mut i = CRC_DATA_START;
    let [t1, t2, t3, t4, t5, t6] = buffer
        .iter()
        .skip(CRC_DATA_START)
        .take(CRC_DATA_LEN)
        .collect::<Vec<&u8>>()
        .chunks_exact(4)
        .filter_map(|c| match c {
            &[&a, &b, &c, &d] => Some(BigEndian::read_u32(&[a, b, c, d])),
            _ => unreachable!(),
        })
        .fold(
            [seed, seed, seed, seed, seed, seed],
            |[t1, t2, t3, t4, t5, t6], d| {
                let r = d.rotate_left(d & 0x1F);
                let weird_index = CIC_6105_TABLE_BASE + (i & 0xff);
                [
                    if cic_selector == CIC_6105 {
                        i += 4;
                        t1.wrapping_add(
                            BigEndian::read_u32(&buffer[weird_index..weird_index + 4]) ^ d,
                        )
                    } else {
                        t1.wrapping_add(t5.wrapping_add(r) ^ d)
                    },
                    t2 ^ if t2 > d { r } else { t6.wrapping_add(d) ^ d },
                    t3 ^ d,
                    t4.wrapping_add(if t6.checked_add(d).is_none() { 1 } else { 0 }),
                    t5.wrapping_add(r),
                    t6.wrapping_add(d),
                ]
            },
        );

    let (crc1, crc2) = match cic_selector {
        CIC_6106 => (
            t6.wrapping_mul(t4).wrapping_add(t3),
            t5.wrapping_mul(t2).wrapping_add(t1),
        ),
        CIC_6103 => ((t6 ^ t4).wrapping_add(t3), (t5 ^ t2).wrapping_add(t1)),
        _ => (t6 ^ t4 ^ t3, t5 ^ t2 ^ t1),
    };

    Ok((crc1, crc2))
}
