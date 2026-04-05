// References:
//   Odyssey 2 / Videopac cartridge format:
//     http://www.yourspectrumprograms.co.uk/philips/tl.htm
//   Intel 8048 microcontroller datasheet:
//     https://datasheetspdf.com/pdf/509798/Intel/8048/1

use super::helpers::compute_sha1;
use crate::traits::{error::ParseError, rom_hash::RomHash, rominfo::RomInfo};

mod opcodes;
use opcodes::{AccEffect, acc_effect, decode_jmp, instruction_len, is_call, is_jmp};

// Odyssey² vector table geometry in cartridge space (byte offsets from cart base).
const ENTRY_VECTOR_BYTES: usize = 12; // bytes 0..11
const ENTRY_VECTOR_STRIDE: usize = 2; // vectors at 0,2,4,6,8,10
const IRQ_VECTOR_INDICES: [usize; 2] = [2, 4]; // ext IRQ and timer IRQ slots

// Heuristic scan windows used when classifying single-game vs multi-game carts.
const DISPATCH_ACC_SCAN_BYTES: usize = 16;
const DISPATCH_PATTERN_SCAN_BYTES: usize = 48;
const XRL_IMMEDIATE_OPCODE: u8 = 0xD3;
const JZ_OPCODE: u8 = 0xC6;
const GAME_SELECT_KEY_MIN: u8 = 1;
const GAME_SELECT_KEY_MAX: u8 = 9;

// Known Odyssey² dump sizes and banking geometry from cartridge specs.
const O2_ROM_SIZE_2K: usize = 2048;
const O2_ROM_SIZE_4K: usize = 4096;
const O2_ROM_SIZE_8K: usize = 8192;
const O2_BANK_SIZE: usize = 2048;

// Entry table candidate positions seen across clean dumps and wiring variants.
const STANDARD_CART_OFFSET: usize = 0x000;
const ENTRY_OFFSET_CANDIDATES: [usize; 4] = [
    STANDARD_CART_OFFSET,
    FULL_ADDRESS_SPACE_OFFSET,
    G7400_CODE_OFFSET,
    0x1800,
];
const G7400_CODE_OFFSET: usize = 0x800;
const FULL_ADDRESS_SPACE_OFFSET: usize = 0x400;
const G7400_MIN_PROBE_LEN: usize = 0x80C;

// Atari 2600 false-positive filter: STA/STX/STY zero-page store opcodes and
// TIA registers commonly touched by 2600 startup/video loops.
const TIA_STORE_OPCODE_MIN: u8 = 0x84;
const TIA_STORE_OPCODE_MAX: u8 = 0x86;
const TIA_VSYNC: u8 = 0x00;
const TIA_VBLANK: u8 = 0x01;
const TIA_WSYNC: u8 = 0x02;
const TIA_HMOVE: u8 = 0x2A;
const BLANK_BYTE_ZERO: u8 = 0x00;
const BLANK_BYTE_ERASED: u8 = 0xFF;

/// BIOS routine at 0x2C3 displays the "SELECT GAME" screen, waits for a key
/// press, then jumps to cartridge byte 8 (CPU 0x408) with the key code in A.
const BIOS_SELECT_GAME: u16 = 0x2C3;

/// Cartridge external ROM is mapped starting at CPU address 0x400.
const CART_BASE: u16 = 0x400;

/// Count the number of games by analyzing the dispatch code reached from
/// cartridge byte 8.
///
/// Research intent:
/// - This logic is trying to answer the same question the BIOS "SELECT GAME"
///   prompt asks the user: how many selectable entries exist on this cart?
/// - We infer that count by decoding jump-table/dispatch behavior around the
///   entry vectors instead of relying on filename/catalog metadata.
///
/// The BIOS "SELECT GAME" routine passes the pressed key code in the
/// accumulator and jumps to cartridge byte 8 (CPU 0x408).  Byte 8 is
/// always a JMP to the real dispatch code.
///
/// At the JMP target, single-game carts overwrite the accumulator (CALL a
/// setup routine, MOV A, #imm, etc.).  Multi-game carts test it using one
/// of two patterns:
///
///   • XRL pattern:  `XRL A, #key` + `JZ`/`JNZ` per game
///   • DEC pattern:  repeated `DEC A` + `JZ` per game
///
/// Returns `Some(n)` if the game count can be determined, `None` otherwise.
#[allow(clippy::arithmetic_side_effects)]
fn count_games(buffer: &[u8]) -> Option<u8> {
    if !is_jmp(buffer[8]) {
        return None;
    }

    // Follow the JMP at byte 8 to the dispatch target.
    let target = decode_jmp(buffer[8], buffer[9]);
    let start = (target as usize).checked_sub(CART_BASE as usize)?;
    if start >= buffer.len() {
        return None;
    }

    // Forward-scan to find the first instruction that reads or overwrites A.
    // Skip past neutral instructions (e.g. MOV Rn, #imm) that don't affect A.
    let mut pc = start;
    let scan_limit = (start + DISPATCH_ACC_SCAN_BYTES).min(buffer.len());
    let mut a_is_used = false;

    while pc < scan_limit {
        let op = buffer[pc];
        match acc_effect(op) {
            AccEffect::Reads => {
                a_is_used = true;
                break;
            }
            AccEffect::Overwrites => {
                break;
            }
            AccEffect::Neutral => {
                pc += instruction_len(op);
            }
        }
    }

    if !a_is_used {
        return Some(1);
    }

    // A is used — this is a multi-game cartridge.  Count game entries
    // using both the XRL and DEC dispatch patterns.
    let window_end = (start + DISPATCH_PATTERN_SCAN_BYTES).min(buffer.len().saturating_sub(1));

    // Pattern 1: XRL A, #key (0xD3 xx) with key in 1..=9.
    // Each distinct key code represents one game.
    let mut xrl_keys = 0u16;
    let mut i = start;
    while i < window_end {
        if buffer[i] == XRL_IMMEDIATE_OPCODE {
            let key = buffer[i + 1];
            if (GAME_SELECT_KEY_MIN..=GAME_SELECT_KEY_MAX).contains(&key) {
                xrl_keys |= 1 << key;
            }
        }
        i += 1;
    }
    let xrl_count = xrl_keys.count_ones() as u8;

    if xrl_count > 0 {
        return Some(xrl_count);
    }

    // Pattern 2: DEC A (0x07) sequences followed by JZ (0xC6).
    // Each JZ after one or more DEC A instructions is one game entry.
    let mut jz_count = 0u8;
    i = start;
    while i < window_end {
        if buffer[i] == JZ_OPCODE {
            jz_count += 1;
        }
        i += 1;
    }

    if jz_count > 0 {
        return Some(jz_count);
    }

    // A is used but no recognizable dispatch pattern — we know it's
    // multi-game but can't determine the count.
    None
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MagnavoxOdyssey2Info {
    pub size_kb: usize,
    /// CPU address of the reset handler, decoded from the JMP at cartridge byte 0.
    /// `None` when byte 0 is not a JMP instruction (e.g. NOP slide).
    pub reset_entry: Option<u16>,
    /// Whether the cartridge's reset entry jumps to the BIOS "SELECT GAME"
    /// screen at 0x2C3.  When true, the BIOS waits for a key press and then
    /// dispatches to cartridge byte 8 (CPU 0x408) with the key code in A.
    pub uses_bios_game_select: bool,
    /// Number of game-select entries inferred from dispatch/jump-table patterns.
    /// `None` means we detected a multi-game style flow but could not confidently
    /// decode an exact count from the observed opcode pattern window.
    pub game_count: Option<u8>,
    /// True when the ROM appears to be a G7400-only cartridge (0xFF at both
    /// standard entry-point regions, code only at 0x800+).
    pub g7400_only: bool,
    /// Index of the 2KB bank containing entry vectors (0-based from file start).
    pub boot_bank: u8,
    /// Total number of 2KB banks in the ROM.
    pub bank_count: u8,
    /// Per-bank status: true if the bank is all 0x00 or all 0xFF.
    pub blank_banks: Vec<bool>,
    /// For 2-bank ROMs only: SHA1 of the file with the two halves swapped.
    /// Useful when a dump has reversed bank order relative to a known-good hash.
    pub swapped_sha1: Option<[u8; 20]>,
    pub rom_sha1: [u8; 20],
}

impl RomHash for MagnavoxOdyssey2Info {
    fn sha1(&self) -> [u8; 20] {
        self.rom_sha1
    }
}

impl RomInfo for MagnavoxOdyssey2Info {
    fn console(&self) -> &'static str {
        if self.g7400_only {
            "Philips Videopac+ G7400"
        } else {
            "Magnavox Odyssey²"
        }
    }
}

/// Count JMP and CALL opcodes at the 6 entry-point positions
/// (bytes 0, 2, 4, 6, 8, 10).
fn count_control_transfers(buf: &[u8]) -> u8 {
    (0..ENTRY_VECTOR_BYTES)
        .step_by(ENTRY_VECTOR_STRIDE)
        .filter(|&i| i < buf.len() && (is_jmp(buf[i]) || is_call(buf[i])))
        .count() as u8
}

/// Check if the entry-point area (first 12 bytes) is all 0x00.
/// The 8048 NOP opcode is 0x00, so this is a valid "fall-through"
/// entry that reaches the real code after the vector table.
fn is_nop_slide(buf: &[u8]) -> bool {
    buf.len() >= ENTRY_VECTOR_BYTES
        && buf[..ENTRY_VECTOR_BYTES]
            .iter()
            .all(|&b| b == BLANK_BYTE_ZERO)
}

/// Check if the entry-point area (first 12 bytes) is all 0xFF.
/// Unprogrammed EPROM cells read as 0xFF; some bankswitched ROMs
/// have blank space in the first bank with code elsewhere.
fn is_ff_slide(buf: &[u8]) -> bool {
    buf.len() >= ENTRY_VECTOR_BYTES
        && buf[..ENTRY_VECTOR_BYTES]
            .iter()
            .all(|&b| b == BLANK_BYTE_ERASED)
}

/// Check whether position 2 (ext IRQ) or position 4 (timer IRQ) holds
/// a JMP or CALL.  These are the most reliable entry points because the
/// BIOS dispatches to them unconditionally on interrupt.
fn has_irq_vector(buf: &[u8]) -> bool {
    IRQ_VECTOR_INDICES
        .iter()
        .any(|&i| i < buf.len() && (is_jmp(buf[i]) || is_call(buf[i])))
}

impl TryFrom<&[u8]> for MagnavoxOdyssey2Info {
    type Error = ParseError;

    #[allow(clippy::arithmetic_side_effects)]
    fn try_from(buffer: &[u8]) -> Result<Self, Self::Error> {
        // Valid Odyssey² cartridge sizes are 2 KB, 4 KB, and 8 KB (bankswitched).
        if !matches!(
            buffer.len(),
            O2_ROM_SIZE_2K | O2_ROM_SIZE_4K | O2_ROM_SIZE_8K
        ) {
            return Err(ParseError::BufferTooSmall);
        }

        // Reject ROMs that match all 4 Atari 2600 TIA register write patterns.
        // Every A2600 ROM at O2-valid sizes has TIA=4; no O2 ROM exceeds TIA=3.
        let has_tia_write = |reg: u8| {
            buffer
                .windows(2)
                .any(|w| matches!(w[0], TIA_STORE_OPCODE_MIN..=TIA_STORE_OPCODE_MAX) && w[1] == reg)
        };
        if has_tia_write(TIA_VSYNC)
            && has_tia_write(TIA_VBLANK)
            && has_tia_write(TIA_WSYNC)
            && has_tia_write(TIA_HMOVE)
        {
            return Err(ParseError::MagicNotFound);
        }

        // The Odyssey² BIOS dispatches to six fixed entry points in the
        // cartridge external ROM (mapped from CPU address 0x400):
        //
        //   byte  0 (CPU 0x400) — reset
        //   byte  2 (CPU 0x402) — external interrupt
        //   byte  4 (CPU 0x404) — timer interrupt
        //   byte  6 (CPU 0x406) — keyboard handler (from IRQ)
        //   byte  8 (CPU 0x408) — game dispatch (after SELECT GAME)
        //   byte 10 (CPU 0x40A) — game-loop entry
        //
        // Most carts have JMP/CALL at these positions.  Some have all-NOP
        // or all-0xFF entry areas.  ROM dumps may place entry points at
        // file offset 0x400 (full address space) or 0x800 (non-standard
        // ROM chip wiring / interleaved dumps).

        let has_real_entries = |buf: &[u8]| -> bool {
            let ct = count_control_transfers(buf);
            ct >= 3 || (ct >= 2 && has_irq_vector(buf))
        };

        let valid_offsets = || {
            ENTRY_OFFSET_CANDIDATES
                .iter()
                .copied()
                .filter(|&o| o + ENTRY_VECTOR_BYTES <= buffer.len())
        };

        // Prefer offsets with real JMP/CALL entry vectors; fall back to
        // NOP/0xFF slides (which indicate valid but blank entry areas).
        let cart_offset = valid_offsets()
            .find(|&o| has_real_entries(&buffer[o..]))
            .or_else(|| {
                valid_offsets().find(|&o| is_nop_slide(&buffer[o..]) || is_ff_slide(&buffer[o..]))
            })
            .ok_or(ParseError::MagicNotFound)?;

        let cart = &buffer[cart_offset..];

        let reset_entry = if is_jmp(cart[0]) {
            Some(decode_jmp(cart[0], cart[1]))
        } else {
            None
        };
        let uses_bios_game_select = reset_entry == Some(BIOS_SELECT_GAME);

        let game_count = if uses_bios_game_select {
            count_games(cart)
        } else {
            None
        };

        // G7400-only cartridges have unprogrammed (0xFF) entry areas at both
        // the standard offset (0x000) and the full-address-space offset (0x400),
        // with real code starting at 0x800.
        let g7400_only = cart_offset == G7400_CODE_OFFSET
            && buffer.len() >= G7400_MIN_PROBE_LEN
            && is_ff_slide(&buffer[STANDARD_CART_OFFSET..])
            && is_ff_slide(&buffer[FULL_ADDRESS_SPACE_OFFSET..]);

        let size_kb = buffer.len() / 1024;

        let bank_size = O2_BANK_SIZE;
        let bank_count = (buffer.len() / bank_size) as u8;

        // Determine which bank holds the entry vectors.  The o2em emulator
        // loads banks in reverse file order, so the *last* 2KB bank in the
        // file becomes the boot bank.  Check the last bank first — even a
        // single JMP/CALL at an entry position is enough to consider it a
        // boot bank (the strict `has_real_entries` threshold is for ROM
        // identification, not bank-order diagnosis).
        let last_bank_offset = (bank_count as usize - 1) * bank_size;
        let boot_bank =
            if bank_count > 1 && count_control_transfers(&buffer[last_bank_offset..]) > 0 {
                bank_count - 1
            } else {
                (cart_offset / bank_size) as u8
            };

        // Check each bank for blank content (all 0x00 or all 0xFF).
        let blank_banks: Vec<bool> = (0..bank_count as usize)
            .map(|b| {
                let start = b * bank_size;
                let end = start + bank_size;
                let bank = &buffer[start..end];
                bank.iter().all(|&x| x == BLANK_BYTE_ZERO)
                    || bank.iter().all(|&x| x == BLANK_BYTE_ERASED)
            })
            .collect();

        let rom_sha1 = compute_sha1(buffer);

        // For 2-bank ROMs, compute the hash of the swapped version.
        let swapped_sha1 = if bank_count == 2 {
            let mut swapped = Vec::with_capacity(buffer.len());
            swapped.extend_from_slice(&buffer[bank_size..]);
            swapped.extend_from_slice(&buffer[..bank_size]);
            Some(compute_sha1(&swapped))
        } else {
            None
        };

        Ok(MagnavoxOdyssey2Info {
            size_kb,
            reset_entry,
            uses_bios_game_select,
            game_count,
            g7400_only,
            boot_bank,
            bank_count,
            blank_banks,
            swapped_sha1,
            rom_sha1,
        })
    }
}

impl std::fmt::Display for MagnavoxOdyssey2Info {
    #[allow(clippy::arithmetic_side_effects)]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Size: {} KB", self.size_kb)?;
        if self.uses_bios_game_select {
            match self.game_count {
                Some(n) => writeln!(f, "Games: {}", n)?,
                None => writeln!(f, "Games: multiple")?,
            }
        }
        if let Some(entry) = self.reset_entry
            && !self.uses_bios_game_select
        {
            writeln!(f, "Reset Entry: {:#05X}", entry)?;
        }

        // Bank analysis (only for multi-bank ROMs)
        if self.bank_count > 1 {
            let expected_boot = self.bank_count - 1;
            if self.boot_bank != expected_boot {
                writeln!(
                    f,
                    "Banks: boot bank at {}, expected at {} (non-standard order)",
                    self.boot_bank, expected_boot
                )?;
            }

            // Flag blank banks (possible dump errors)
            for (i, &blank) in self.blank_banks.iter().enumerate() {
                if blank && !self.g7400_only {
                    writeln!(f, "Bank {}: blank", i)?;
                }
            }

            // Show swapped hash for 2-bank ROMs with non-standard order
            if let Some(sha1) = &self.swapped_sha1
                && self.boot_bank != expected_boot
            {
                let hex: String = sha1.iter().map(|b| format!("{:02X}", b)).collect();
                writeln!(f, "Swapped SHA1: {}", hex)?;
            }
        }

        writeln!(f, "{}", self as &dyn RomHash)?;
        Ok(())
    }
}
