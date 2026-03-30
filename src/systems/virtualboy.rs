// References:
//   Virtual Boy ROM header format:
//     https://planetvb.com/modules/dokuwiki/doku.php?id=info_at_the_end_of_the_rom

use super::helpers::{
    compute_sha1, dat_revision, nintendo_region_dat, nintendo_region_display, non_empty,
};
use crate::systems::gameboy::lookup_new_licensee;
use crate::traits::{
    error::ParseError,
    rom_hash::RomHash,
    rominfo::{DatMeta, RomInfo},
    title::Title,
};
use encoding_rs::SHIFT_JIS;
use unicode_normalization::UnicodeNormalization;

const MIN_ROM_SIZE: usize = 0x20000;
const MAX_ROM_SIZE: usize = 0x200000;
const HEADER_FROM_END: usize = 0x220;
const HEADER_SIZE: usize = 32;
const TITLE_START: usize = 0x00;
const TITLE_END: usize = 0x14;
const RESERVED_START: usize = 0x14;
const RESERVED_END: usize = 0x19;
const MAKER_CODE_START: usize = 0x19;
const MAKER_CODE_END: usize = 0x1B;
const GAME_CODE_START: usize = 0x1B;
const GAME_CODE_END: usize = 0x1F;
const VERSION_OFFSET: usize = 0x1F;

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct VirtualBoyRomInfo {
    pub title: String,
    pub game_code: [u8; 4],
    pub maker_code: [u8; 2],
    pub version: u8,
    pub rom_sha1: [u8; 20],
}

impl RomInfo for VirtualBoyRomInfo {
    fn console(&self) -> &'static str {
        "Virtual Boy"
    }

    fn dat_meta(&self) -> DatMeta {
        let serial = String::from_utf8(self.game_code.to_vec()).ok();
        DatMeta {
            title: non_empty(&self.title),
            region: nintendo_region_dat(self.game_code[3]),
            version: dat_revision(self.version),
            serial: serial.as_deref().and_then(non_empty),
            machine_id: serial.as_deref().and_then(non_empty),
            ..DatMeta::default()
        }
    }
}

fn is_ascii_uppercase_alnum(b: u8) -> bool {
    b.is_ascii_uppercase() || b.is_ascii_digit()
}

impl TryFrom<&[u8]> for VirtualBoyRomInfo {
    type Error = ParseError;

    #[allow(clippy::arithmetic_side_effects)]
    fn try_from(buffer: &[u8]) -> Result<Self, Self::Error> {
        let len = buffer.len();

        // File size must be power-of-2, in range 128KB–2MB
        if !(MIN_ROM_SIZE..=MAX_ROM_SIZE).contains(&len) || !len.is_power_of_two() {
            return Err(ParseError::BufferTooSmall);
        }

        // Header is at file_size - 0x220
        let header_offset = len - HEADER_FROM_END;
        if header_offset + HEADER_SIZE > len {
            return Err(ParseError::BufferTooSmall);
        }

        let header = &buffer[header_offset..header_offset + HEADER_SIZE];

        // Reserved bytes (5 bytes at +0x14) must all be zero
        if header[RESERVED_START..RESERVED_END].iter().any(|&b| b != 0) {
            return Err(ParseError::InvalidHeader);
        }

        // Maker code (2 bytes at +0x19) must be ASCII uppercase alphanumeric
        if !is_ascii_uppercase_alnum(header[MAKER_CODE_START])
            || !is_ascii_uppercase_alnum(header[MAKER_CODE_END - 1])
        {
            return Err(ParseError::InvalidHeader);
        }

        // Game code (4 bytes at +0x1B) must be ASCII uppercase alphanumeric
        if !header[GAME_CODE_START..GAME_CODE_END]
            .iter()
            .all(|&b| is_ascii_uppercase_alnum(b))
        {
            return Err(ParseError::InvalidHeader);
        }

        // Title (20 bytes at +0x00): at least 1 printable ASCII character after trimming nulls
        let title_bytes = &header[TITLE_START..TITLE_END];
        let title = decode_title(title_bytes);
        if !title.bytes().any(|b| b.is_ascii_graphic() || b == b' ') {
            return Err(ParseError::InvalidHeader);
        }

        let rom_sha1 = compute_sha1(buffer);

        Ok(VirtualBoyRomInfo {
            title,
            game_code: header[GAME_CODE_START..GAME_CODE_END]
                .try_into()
                .map_err(|_| ParseError::InvalidHeader)?,
            maker_code: header[MAKER_CODE_START..MAKER_CODE_END]
                .try_into()
                .map_err(|_| ParseError::InvalidHeader)?,
            version: header[VERSION_OFFSET],
            rom_sha1,
        })
    }
}

impl RomHash for VirtualBoyRomInfo {
    fn sha1(&self) -> [u8; 20] {
        self.rom_sha1
    }
}

impl Title for VirtualBoyRomInfo {
    fn title(&self) -> &str {
        &self.title
    }
}

impl std::fmt::Display for VirtualBoyRomInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let region = nintendo_region_display(self.game_code[3]);

        write!(f, "{}", self as &dyn Title)?;
        writeln!(f, "Game Code: {}", String::from_utf8_lossy(&self.game_code))?;
        writeln!(f, "Region: {}", region)?;
        if self.version != 0 {
            writeln!(f, "Version: {}", self.version)?;
        }
        let maker_code_str = String::from_utf8_lossy(&self.maker_code);
        if let Some(name) = lookup_new_licensee(&self.maker_code) {
            writeln!(f, "Maker: {name} (\"{maker_code_str}\")")?;
        } else {
            writeln!(f, "Maker: \"{maker_code_str}\"")?;
        }
        writeln!(f, "{}", self as &dyn RomHash)
    }
}

fn decode_title(buffer: &[u8]) -> String {
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
