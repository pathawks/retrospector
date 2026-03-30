// References:
//   GBA cartridge header:
//     https://problemkaputt.de/gbatek.htm#gbacartridgeheader

use super::helpers::{
    compute_sha1, dat_revision, nintendo_region_dat, nintendo_region_display, non_empty,
};
use crate::systems::gameboy::lookup_new_licensee;
use crate::traits::{
    error::ParseError,
    rom_hash::RomHash,
    rominfo::{DatMeta, RomInfo},
    stored_checksum::StoredChecksum,
    title::Title,
};

const HEADER_SIZE: usize = 0xC0;
const TITLE_START: usize = 0xA0;
const TITLE_END: usize = 0xAC;
const GAME_CODE_START: usize = 0xAC;
const GAME_CODE_END: usize = 0xB0;
const MAKER_CODE_START: usize = 0xB0;
const MAKER_CODE_END: usize = 0xB2;
const FIXED_VALUE_OFFSET: usize = 0xB2;
const FIXED_VALUE: u8 = 0x96;
const VERSION_OFFSET: usize = 0xBC;
const HEADER_CHECKSUM_OFFSET: usize = 0xBD;
const HEADER_CHECKSUM_SEED: u8 = 0xE7;

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct GbaRomInfo {
    pub title: String,
    pub game_code: [u8; 4],
    pub maker_code: [u8; 2],
    pub version: u8,
    pub stored_checksum: u8,
    pub calced_checksum: u8,
    pub rom_sha1: [u8; 20],
}

impl RomInfo for GbaRomInfo {
    fn console(&self) -> &'static str {
        "Game Boy Advance"
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

impl TryFrom<&[u8]> for GbaRomInfo {
    type Error = ParseError;

    fn try_from(buffer: &[u8]) -> Result<Self, Self::Error> {
        if !is_gba_rom(buffer) {
            return Err(ParseError::MagicNotFound);
        }
        if buffer.len() < HEADER_SIZE {
            return Err(ParseError::BufferTooSmall);
        }

        let rom_sha1 = compute_sha1(buffer);

        let title = String::from_utf8_lossy(&buffer[TITLE_START..TITLE_END])
            .trim_end_matches('\0')
            .to_string();

        let rom_info = GbaRomInfo {
            title,
            game_code: buffer[GAME_CODE_START..GAME_CODE_END]
                .try_into()
                .map_err(|_| ParseError::InvalidHeader)?,
            maker_code: buffer[MAKER_CODE_START..MAKER_CODE_END]
                .try_into()
                .map_err(|_| ParseError::InvalidHeader)?,
            version: buffer[VERSION_OFFSET],
            stored_checksum: buffer[HEADER_CHECKSUM_OFFSET],
            calced_checksum: buffer[TITLE_START..HEADER_CHECKSUM_OFFSET]
                .iter()
                .fold(HEADER_CHECKSUM_SEED, |acc, &b| acc.wrapping_sub(b)),
            rom_sha1,
        };
        Ok(rom_info)
    }
}

impl StoredChecksum<u8> for GbaRomInfo {
    fn stored_checksum(&self) -> u8 {
        self.stored_checksum
    }

    fn calculated_checksum(&self) -> u8 {
        self.calced_checksum
    }
}

impl RomHash for GbaRomInfo {
    fn sha1(&self) -> [u8; 20] {
        self.rom_sha1
    }
}

impl Title for GbaRomInfo {
    fn title(&self) -> &str {
        &self.title
    }
}

impl std::fmt::Display for GbaRomInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let region = nintendo_region_display(self.game_code[3]);

        write!(f, "{}", self as &dyn Title)?;
        writeln!(f, "Game Code: {}", String::from_utf8_lossy(&self.game_code))?;
        writeln!(f, "Region: {}", region)?;
        writeln!(f, "Version: {}", self.version)?;
        let maker_code_str = String::from_utf8_lossy(&self.maker_code);
        if let Some(name) = lookup_new_licensee(&self.maker_code) {
            writeln!(f, "Maker: {name} (\"{maker_code_str}\")")?;
        } else {
            writeln!(f, "Maker: \"{maker_code_str}\"")?;
        }
        writeln!(f, "{}", self as &dyn StoredChecksum<u8>)?;
        writeln!(f, "{}", self as &dyn RomHash)?;

        Ok(())
    }
}

pub fn is_gba_rom(buffer: &[u8]) -> bool {
    if buffer.len() < HEADER_SIZE || buffer[FIXED_VALUE_OFFSET] != FIXED_VALUE {
        return false;
    }
    let checksum = buffer[TITLE_START..HEADER_CHECKSUM_OFFSET]
        .iter()
        .fold(HEADER_CHECKSUM_SEED, |acc, &b| acc.wrapping_sub(b));
    checksum == buffer[HEADER_CHECKSUM_OFFSET]
}
