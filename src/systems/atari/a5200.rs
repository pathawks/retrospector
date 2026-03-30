// References:
//   Atari 5200 cartridge formats and CAR header:
//     https://github.com/atari800/atari800/blob/master/DOC/cart.txt
//   ANTIC, GTIA, and POKEY register maps:
//     https://problemkaputt.de/8bitspecs.htm
//   Altirra hardware reference (memory map):
//     https://www.virtualdub.org/downloads/Altirra%20Hardware%20Reference%20Manual.pdf

use crate::systems::helpers::{compute_sha1, non_empty};
use byteorder::{BigEndian, ByteOrder, LittleEndian};

use crate::traits::{
    error::ParseError,
    rom_hash::RomHash,
    rominfo::{DatMeta, RomInfo},
    title::Title,
};

// Atari 5200 CAR header and raw-ROM parsing constants.
const A5200_CAR_HEADER_BYTES: usize = 16;
const A5200_CAR_MAGIC_END: usize = 4;
const A5200_CAR_MAGIC: &[u8; 4] = b"CART";
const A5200_CAR_TYPE_START: usize = 4;
const A5200_CAR_TYPE_END: usize = 8;
const A5200_VALID_RAW_SIZES: [usize; 5] = [0x1000, 0x2000, 0x4000, 0x8000, 0xA000];
const A5200_STA_ABS_OPCODE: u8 = 0x8D;
const A5200_ANTIC_PAGE: u8 = 0xD4;
const A5200_GTIA_PAGE: u8 = 0xC0;
const A5200_POKEY_PAGE: u8 = 0xE8;
const A5200_HW_PATTERN_THRESHOLD: u8 = 2;
const A5200_TITLE_TRAILER_BYTES: usize = 24;
const A5200_TITLE_FOOTER_BYTES: usize = 4;

// ANTIC mode 6/7 title decoding constants.
const ANTIC_CHAR_MASK: u8 = 0x7F;
const ANTIC_DIRECT_RANGE_MAX: u8 = 0x3F;
const ANTIC_ASCII_BASE: u8 = 0x20;

/// Atari 5200 cartridge types in CAR format
const ATARI_5200_CART_TYPES: &[u32] = &[
    4,   // Standard 32 KB 5200 cartridge
    6,   // Two chip 16 KB 5200 cartridge
    7,   // Bounty Bob Strikes Back 40 KB 5200 cartridge
    16,  // One chip 16 KB 5200 cartridge
    19,  // Standard 8 KB 5200 cartridge
    20,  // Standard 4 KB 5200 cartridge
    71,  // Super Cart 64 KB (32K banks)
    72,  // Super Cart 128 KB (32K banks)
    73,  // Super Cart 256 KB (32K banks)
    74,  // Super Cart 512 KB (32K banks)
    159, // Bounty Bob Strikes Back 40 KB 5200 alt
];

/// Convert ANTIC mode 6/7 internal character code to ASCII
/// Internal codes are basically ASCII - 0x20 for the printable range
#[allow(clippy::arithmetic_side_effects)]
fn antic_to_ascii(code: u8) -> char {
    let code = code & ANTIC_CHAR_MASK; // Strip inverse bit
    if code <= ANTIC_DIRECT_RANGE_MAX {
        // Codes 0x00-0x3F map to ASCII 0x20-0x5F (space through underscore)
        ((code & ANTIC_DIRECT_RANGE_MAX) + ANTIC_ASCII_BASE) as char
    } else {
        // Codes 0x40-0x7F are inverse versions, same character
        ((code & ANTIC_DIRECT_RANGE_MAX) + ANTIC_ASCII_BASE) as char
    }
}

/// Extract title from ANTIC mode 7 display codes
fn extract_antic_title(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| antic_to_ascii(b))
        .collect::<String>()
        .trim()
        .to_string()
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct Atari5200Info {
    pub title: String,
    pub cart_type: Option<u32>,
    pub has_car_header: bool,
    pub rom_sha1: [u8; 20],
}

impl Title for Atari5200Info {
    fn title(&self) -> &str {
        &self.title
    }
}

impl RomHash for Atari5200Info {
    fn sha1(&self) -> [u8; 20] {
        self.rom_sha1
    }
}

impl RomInfo for Atari5200Info {
    fn console(&self) -> &'static str {
        "Atari 5200"
    }

    fn dat_meta(&self) -> DatMeta {
        DatMeta {
            title: non_empty(&self.title),
            ..DatMeta::default()
        }
    }
}

impl TryFrom<&[u8]> for Atari5200Info {
    type Error = ParseError;

    #[allow(clippy::arithmetic_side_effects)]
    fn try_from(buffer: &[u8]) -> Result<Self, Self::Error> {
        // Check for CAR header format first
        if buffer.len() >= A5200_CAR_HEADER_BYTES
            && &buffer[..A5200_CAR_MAGIC_END] == A5200_CAR_MAGIC
        {
            let cart_type = BigEndian::read_u32(&buffer[A5200_CAR_TYPE_START..A5200_CAR_TYPE_END]);

            if ATARI_5200_CART_TYPES.contains(&cart_type) {
                // CAR format: skip 16-byte header for ROM data
                let rom_data = &buffer[A5200_CAR_HEADER_BYTES..];
                let title = extract_title_from_rom(rom_data);

                // Calculate SHA1 of ROM data (excluding 16-byte CAR header)
                let rom_sha1 = compute_sha1(rom_data);

                return Ok(Atari5200Info {
                    title,
                    cart_type: Some(cart_type),
                    has_car_header: true,
                    rom_sha1,
                });
            }
            return Err(ParseError::MagicNotFound);
        }

        // Raw ROM detection (no header)
        let len = buffer.len();

        // Valid 5200 cart sizes: 4KB, 8KB, 16KB, 32KB, 40KB
        if !A5200_VALID_RAW_SIZES.contains(&len) {
            return Err(ParseError::BufferTooSmall);
        }

        // Check start vector at end of ROM (0xBFFE-0xBFFF relative to cart base)
        // The vector must point into the cartridge's actual address range
        if len < 2 {
            return Err(ParseError::BufferTooSmall);
        }

        let start_vector = LittleEndian::read_u16(&buffer[len - 2..]);

        // Cartridge ROM ends at 0xBFFF, so base address = 0xC000 - size
        // 32KB: 0x4000-0xBFFF
        // 16KB: 0x8000-0xBFFF (one-chip) or 0x4000-0x7FFF + 0x8000-0xBFFF (two-chip)
        // 8KB:  0xA000-0xBFFF
        // 4KB:  0xB000-0xBFFF
        // 40KB: Special Bounty Bob mapping
        let (min_addr, max_addr): (u16, u16) = match len {
            0x8000 => (0x4000, 0xBFFF), // 32KB
            0x4000 => (0x4000, 0xBFFF), // 16KB - two-chip can start at 0x4000
            0x2000 => (0xA000, 0xBFFF), // 8KB
            0x1000 => (0xB000, 0xBFFF), // 4KB
            0xA000 => (0x4000, 0xBFFF), // 40KB (Bounty Bob)
            _ => return Err(ParseError::BufferTooSmall),
        };

        if start_vector < min_addr || start_vector > max_addr {
            return Err(ParseError::MagicNotFound);
        }

        // Check for 5200-specific hardware register writes
        // ANTIC at 0xD4xx, GTIA at 0xC0xx, POKEY at 0xE8xx
        // Look for STA absolute (0x8D lo hi) to these addresses
        let has_antic = buffer
            .windows(3)
            .any(|w| w[0] == A5200_STA_ABS_OPCODE && w[2] == A5200_ANTIC_PAGE);
        let has_gtia = buffer
            .windows(3)
            .any(|w| w[0] == A5200_STA_ABS_OPCODE && w[2] == A5200_GTIA_PAGE);
        let has_pokey = buffer
            .windows(3)
            .any(|w| w[0] == A5200_STA_ABS_OPCODE && w[2] == A5200_POKEY_PAGE);

        // Require at least two distinct 5200 hardware register types
        let hw_count = has_antic as u8 + has_gtia as u8 + has_pokey as u8;
        if hw_count < A5200_HW_PATTERN_THRESHOLD {
            return Err(ParseError::MagicNotFound);
        }

        // Calculate SHA1 of entire ROM (no header)
        let rom_sha1 = compute_sha1(buffer);

        let title = extract_title_from_rom(buffer);

        Ok(Atari5200Info {
            title,
            cart_type: None,
            has_car_header: false,
            rom_sha1,
        })
    }
}

/// Extract title from raw ROM data
/// Title is stored at 0xBFE8-0xBFFB (20 bytes) in ANTIC mode 7 display codes
#[allow(clippy::arithmetic_side_effects)]
fn extract_title_from_rom(rom: &[u8]) -> String {
    let len = rom.len();
    if len < A5200_TITLE_TRAILER_BYTES {
        return String::new();
    }

    // Title is at offset -24 to -4 from end (20 bytes before the 4-byte footer)
    // Footer: 2 bytes copyright + 2 bytes start vector
    let title_start = len - A5200_TITLE_TRAILER_BYTES;
    let title_end = len - A5200_TITLE_FOOTER_BYTES;

    if title_end > len {
        return String::new();
    }

    extract_antic_title(&rom[title_start..title_end])
}

impl std::fmt::Display for Atari5200Info {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if !self.title.is_empty() {
            write!(f, "{}", self as &dyn Title)?;
        }
        if let Some(cart_type) = self.cart_type {
            writeln!(f, "Cart Type: {}", cart_type)?;
        }
        if self.has_car_header {
            writeln!(f, "Format: CAR (headered)")?;
        }
        writeln!(f, "{}", self as &dyn RomHash)?;
        Ok(())
    }
}
