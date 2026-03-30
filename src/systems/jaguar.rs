// References:
//   Atari Jaguar cartridge ROM layout:
//     https://www.mulle-kybernetik.com/jagdox/gpu.html
//   Jaguar ROM header and entry point:
//     https://problemkaputt.de/jagspecs.htm

use super::helpers::compute_sha1;

use crate::traits::{error::ParseError, rom_hash::RomHash, rominfo::RomInfo};

// Jaguar cartridge probe constants.
const JAGUAR_ROM_SIZE_MIN: usize = 0x80000; // 512 KiB
const JAGUAR_ROM_SIZE_MAX: usize = 0x400000; // 4 MiB
const START_ADDRESS_OFFSET: usize = 0x404;
const START_ADDRESS_END: usize = 0x408;
const EXPECTED_START_ADDRESS_BYTES: [u8; 4] = [0x00, 0x80, 0x20, 0x00]; // 0x00802000

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct AtariJaguarInfo {
    pub start_address: u32,
    pub rom_sha1: [u8; 20],
}

impl RomHash for AtariJaguarInfo {
    fn sha1(&self) -> [u8; 20] {
        self.rom_sha1
    }
}

impl RomInfo for AtariJaguarInfo {
    fn console(&self) -> &'static str {
        "Atari Jaguar"
    }
}

impl TryFrom<&[u8]> for AtariJaguarInfo {
    type Error = ParseError;

    fn try_from(buffer: &[u8]) -> Result<Self, Self::Error> {
        let len = buffer.len();

        // Valid Jaguar ROM sizes: 512KB, 1MB, 2MB, 4MB
        if !(JAGUAR_ROM_SIZE_MIN..=JAGUAR_ROM_SIZE_MAX).contains(&len) || !len.is_power_of_two() {
            return Err(ParseError::BufferTooSmall);
        }

        // Buffer must be long enough to read the start address at 0x404
        if len < START_ADDRESS_END {
            return Err(ParseError::BufferTooSmall);
        }

        // Standard Jaguar cartridge execution address 0x802000 stored big-endian at offset 0x404
        if buffer[START_ADDRESS_OFFSET..START_ADDRESS_END] != EXPECTED_START_ADDRESS_BYTES {
            return Err(ParseError::MagicNotFound);
        }

        let start_address = u32::from_be_bytes([
            buffer[START_ADDRESS_OFFSET],
            buffer[START_ADDRESS_OFFSET + 1],
            buffer[START_ADDRESS_OFFSET + 2],
            buffer[START_ADDRESS_OFFSET + 3],
        ]);

        let rom_sha1 = compute_sha1(buffer);

        Ok(AtariJaguarInfo {
            start_address,
            rom_sha1,
        })
    }
}

impl std::fmt::Display for AtariJaguarInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}", self as &dyn RomHash)?;
        Ok(())
    }
}
