// References:
//   Stella programmer's guide (TIA register addresses):
//     https://alienbill.com/2600/101/docs/stella.html
//   Atari 2600 TIA hardware manual:
//     https://problemkaputt.de/2k6specs.htm#televisioninterfaceadaptortia
//   6502 instruction set reference:
//     https://www.masswerk.at/6502/6502_instruction_set.html

use crate::systems::helpers::compute_sha1;

use crate::traits::{error::ParseError, rom_hash::RomHash, rominfo::RomInfo};

// Atari 2600 cartridge constraints and vector/TIA heuristics.
const A2600_ROM_MIN_BYTES: usize = 0x800; // 2 KiB
const A2600_ROM_MAX_BYTES: usize = 0x20000; // 128 KiB
const A2600_BANK_STRIDE_BYTES: usize = 0x1000; // 4 KiB banks
const A2600_VECTOR_A12_MASK: u16 = 0x1000; // cart ROM space bit
const A2600_TIA_STORE_OPCODE_MIN: u8 = 0x84;
const A2600_TIA_STORE_OPCODE_MAX: u8 = 0x86;
const A2600_TIA_VSYNC: u8 = 0x00;
const A2600_TIA_VBLANK: u8 = 0x01;
const A2600_TIA_WSYNC: u8 = 0x02;
const A2600_TIA_HMOVE: u8 = 0x2A;
const A2600_TIA_PATTERN_THRESHOLD: u8 = 3;

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct Atari2600Info {
    pub rom_sha1: [u8; 20],
}

impl RomInfo for Atari2600Info {
    fn console(&self) -> &'static str {
        "Atari 2600"
    }
}

impl RomHash for Atari2600Info {
    fn sha1(&self) -> [u8; 20] {
        self.rom_sha1
    }
}

/// Check whether any zero-page store instruction (STA/STX/STY) writes to the
/// given TIA register address.
fn has_tia_write(buffer: &[u8], reg: u8) -> bool {
    buffer.windows(2).any(|w| {
        matches!(
            w[0],
            A2600_TIA_STORE_OPCODE_MIN..=A2600_TIA_STORE_OPCODE_MAX
        ) && w[1] == reg
    })
}

/// Count TIA register write patterns typical of Atari 2600 games.
/// Returns the number of distinct TIA registers found (0-4).
#[allow(clippy::arithmetic_side_effects)]
fn count_tia_patterns(buffer: &[u8]) -> u8 {
    has_tia_write(buffer, A2600_TIA_VSYNC) as u8
        + has_tia_write(buffer, A2600_TIA_VBLANK) as u8
        + has_tia_write(buffer, A2600_TIA_WSYNC) as u8
        + has_tia_write(buffer, A2600_TIA_HMOVE) as u8
}

impl TryFrom<&[u8]> for Atari2600Info {
    type Error = ParseError;

    #[allow(clippy::arithmetic_side_effects)]
    fn try_from(buffer: &[u8]) -> Result<Self, Self::Error> {
        let len = buffer.len();

        // Size must be 2KB-128KB
        if !(A2600_ROM_MIN_BYTES..=A2600_ROM_MAX_BYTES).contains(&len) {
            return Err(ParseError::BufferTooSmall);
        }

        // Validate reset vector (6507 mirrors 0xFFFC to 0x1FFC).
        // The startup bank varies by bankswitching scheme — most schemes
        // start from the last bank, but FE starts from bank 0.  Check the
        // end of the file (covers 2 KB carts and the last 4 KB bank) plus
        // every 4 KB bank boundary to handle all layouts, including carts
        // with appended non-program data (e.g. Pitfall II DPC).
        let check_vector = |end: usize| -> bool {
            let lo = buffer[end - 4] as u16;
            let hi = buffer[end - 3] as u16;
            let vector = (hi << 8) | lo;
            vector & A2600_VECTOR_A12_MASK != 0 // A12 must be set (cartridge ROM space)
        };
        let has_valid_reset = check_vector(len)
            || (A2600_BANK_STRIDE_BYTES..=len)
                .step_by(A2600_BANK_STRIDE_BYTES)
                .any(check_vector);
        if !has_valid_reset {
            return Err(ParseError::MagicNotFound);
        }

        // Check for TIA register write patterns (STA zeropage to TIA addresses)
        // All 2600 games must write to these registers to produce video output
        // Require at least 3 distinct TIA patterns
        if count_tia_patterns(buffer) < A2600_TIA_PATTERN_THRESHOLD {
            return Err(ParseError::MagicNotFound);
        }

        // Calculate SHA1 of entire ROM
        let rom_sha1 = compute_sha1(buffer);

        Ok(Atari2600Info { rom_sha1 })
    }
}

impl std::fmt::Display for Atari2600Info {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}", self as &dyn RomHash)?;
        Ok(())
    }
}
