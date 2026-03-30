// Intellivision cartridge ROM detection and metadata extraction.
//
// References:
//   EXEC header layout & cart.mac:
//     http://wiki.intellivision.us/index.php?title=Cart.mac
//   Hello World tutorial (walks through the EXEC header fields):
//     http://wiki.intellivision.us/index.php/Hello_World_Tutorial
//   Executive ROM (boot sequence, 0x4800/0x7000 bypass):
//     https://wiki.intellivision.us/index.php/Executive_ROM
//   Memory map (address ranges for cartridge ROM):
//     http://wiki.intellivision.us/index.php/Memory_Map
//   ROM file formats and 10-bit width:
//     http://nerdlypleasures.blogspot.com/2020/07/batty-over-bits-complexity-of.html
//
// File format
// -----------
// Flat .int/.bin files store each 10-bit ROM word as a big-endian 16-bit
// value (high byte first).  Because commercial Intellivision cartridges
// use 10-bit mask ROMs, the upper 6 bits of each stored word are zero —
// meaning every high byte is 0x00 or at most 0x01.  Later cartridges and
// homebrew may use wider ROM data (high bytes up to 0x03), but the EXEC
// header region at 0x5000-0x500C always uses 10-bit values.
//
// EXEC header (standard boot)
// ---------------------------
// The EXEC ROM reads 6 BIDECLEs + 1 DECLE from the start of cartridge
// ROM (0x5000) during its initialization sequence:
//
//   0x5000-0x5001  BIDECLE 0  MOB (sprite) picture base
//   0x5002-0x5003  BIDECLE 1  Process table pointer
//   0x5004-0x5005  BIDECLE 2  Program start address (entry point)
//   0x5006-0x5007  BIDECLE 3  Background graphics base
//   0x5008-0x5009  BIDECLE 4  GRAM card data base
//   0x500A-0x500B  BIDECLE 5  Title/date string pointer
//   0x500C        DECLE      Flags (INTV2 compat, ECS, clicking, etc.)
//
// A BIDECLE encodes a 16-bit value across two consecutive 10-bit ROM
// words: decoded = (word0 & 0xFF) | ((word1 & 0xFF) << 8).
//
// EXEC bypass (direct boot)
// -------------------------
// Before reading the header, the EXEC checks addresses 0x4800 and 0x7000
// for the presence of ROM.  If ROM exists at either address, the EXEC
// skips its entire initialization sequence (no title screen, no header
// parsing) and jumps directly into the cartridge code.  Games that use
// this bypass — such as Hover Force, Spiker, and Stadium Mud Buggies —
// have program_start = 0 in their BIDECLE 2 position.  The bytes at
// 0x5000-0x500C in these ROMs are ordinary code/data, not EXEC pointers,
// so we do not attempt to extract title, year, or flags from them.

use super::helpers::compute_sha1;
use crate::traits::{
    error::ParseError,
    rom_hash::RomHash,
    rominfo::{DatMeta, RomInfo},
};

const BIDECLE_COUNT: usize = 6;
const BIDECLE_BYTES: usize = 4; // 2 words × 2 bytes each
const WORD_BYTES: usize = 2;
const HEADER_WORDS: usize = BIDECLE_COUNT * 2; // 12 words in 6 BIDECLEs
const MIN_ROM_SIZE: usize = 2048;

// Standard cartridge ROM occupies 0x5000-0x7FFF in the Intellivision
// address space.  Extended cartridges also map ROM at 0xD000-0xDFFF,
// 0xF000-0xFFFF, 0x9000-0xBFFF, etc., but 0x5000-0x7FFF is always present.
// http://wiki.intellivision.us/index.php/Memory_Map
const CART_ROM_BASE: u16 = 0x5000;
const CART_ROM_END: u16 = 0x7FFF;

const BIDECLE_PROGRAM_START: usize = 2;
const BIDECLE_TITLE_PTR: usize = 5;

// Flags DECLE at word 0x500C (byte offset 24 in a flat ROM file).
// http://wiki.intellivision.us/index.php/Hello_World_Tutorial
//
//   Bit 6:    Intellivision 2 compatibility.  When clear, the INTV2
//             EXEC displays a "NOT COMPATIBLE" warning at boot.  Most
//             early titles (pre-1983) predate the INTV2 and lack this.
//   Bit 7:    Run code immediately following the title string data.
//   Bits 8-9: When non-zero, skip the ECS (Entertainment Computer
//             System) title/menu screen.
//   Bits 5-0: Controller input click sounds (not parsed here).
const FLAGS_OFFSET: usize = BIDECLE_COUNT * BIDECLE_BYTES; // 24
const FLAG_INTV2_COMPAT: u16 = 1 << 6;
const FLAG_POST_TITLE_CODE: u16 = 1 << 7;
const FLAG_SKIP_ECS_TITLE: u16 = 0x0300; // bits 8-9

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BootMode {
    /// Standard EXEC boot — BIOS reads the header at 0x5000, shows the
    /// title screen, then jumps to program start (BIDECLE 2).
    Exec,
    /// ROM present at 0x4800 or 0x7000 causes the EXEC to skip header
    /// parsing and jump directly into cartridge code.  The data at
    /// 0x5000-0x500C is not an EXEC header in this case.
    /// https://wiki.intellivision.us/index.php/Executive_ROM
    ExecBypass,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntellivisionInfo {
    pub boot_mode: BootMode,
    pub title: Option<String>,
    pub year: Option<u16>,
    pub program_start: u16,
    pub intv2_compat: bool,
    pub skips_ecs_title: bool,
    pub runs_post_title_code: bool,
    pub size_kb: usize,
    pub rom_sha1: [u8; 20],
}

/// Decode a BIDECLE from two consecutive big-endian 16-bit words.
///
/// Each ROM word is stored as 2 bytes (big-endian) in the file.  A
/// BIDECLE spans two such words and encodes a 16-bit value:
///   decoded = (word0 & 0xFF) | ((word1 & 0xFF) << 8)
///
/// http://wiki.intellivision.us/index.php/Hello_World_Tutorial
#[allow(clippy::arithmetic_side_effects)]
fn decode_bidecle(data: &[u8], index: usize) -> u16 {
    let off = index * BIDECLE_BYTES;
    let lo = u16::from_be_bytes([data[off], data[off + 1]]);
    let hi = u16::from_be_bytes([data[off + 2], data[off + 3]]);
    (lo & 0xFF) | ((hi & 0xFF) << 8)
}

/// Extract title string and 2-digit year from the ROM at the address
/// given by BIDECLE 5 (title/date pointer).
///
/// The EXEC title string format is:
///   Word 0:    2-digit year (e.g. 81 → 1981)
///   Words 1-N: ASCII characters in the low byte, null-terminated
///
/// The pointer is a ROM address (0x5000+).  Each ROM address maps to
/// 2 file bytes, so file_offset = (ptr - 0x5000) * 2.
#[allow(clippy::arithmetic_side_effects)]
fn extract_title_year(buffer: &[u8], ptr: u16) -> (Option<String>, Option<u16>) {
    let file_offset = ((ptr - CART_ROM_BASE) as usize) * 2;
    if file_offset + WORD_BYTES > buffer.len() {
        return (None, None);
    }

    let year_word = u16::from_be_bytes([buffer[file_offset], buffer[file_offset + 1]]);
    let year = if (1..=99).contains(&year_word) {
        Some(year_word + 1900)
    } else {
        None
    };

    let mut title = String::new();
    let mut offset = file_offset + WORD_BYTES;
    while offset + 1 < buffer.len() {
        let ch = buffer[offset + 1]; // low byte of big-endian word
        if ch == 0 {
            break;
        }
        if ch.is_ascii_graphic() || ch == b' ' {
            title.push(ch as char);
        } else {
            break;
        }
        offset += WORD_BYTES;
    }

    let title = if title.is_empty() { None } else { Some(title) };
    (title, year)
}

impl RomHash for IntellivisionInfo {
    fn sha1(&self) -> [u8; 20] {
        self.rom_sha1
    }
}

impl RomInfo for IntellivisionInfo {
    fn console(&self) -> &'static str {
        "Intellivision"
    }

    fn dat_meta(&self) -> DatMeta {
        DatMeta {
            title: self.title.clone(),
            date: self.year.map(|y| y.to_string()),
            ..DatMeta::default()
        }
    }
}

impl TryFrom<&[u8]> for IntellivisionInfo {
    type Error = ParseError;

    #[allow(clippy::arithmetic_side_effects)]
    fn try_from(buffer: &[u8]) -> Result<Self, Self::Error> {
        if buffer.len() < MIN_ROM_SIZE {
            return Err(ParseError::BufferTooSmall);
        }

        // The first 24 bytes are 12 big-endian 16-bit words forming
        // 6 BIDECLEs.  In any Intellivision ROM — whether EXEC or
        // bypass — these positions contain 10-bit data, so every high
        // byte must be 0x00 or 0x01.  This rejects non-Intellivision
        // formats immediately: 6502 (Atari 2600), Z80 (ColecoVision),
        // and 8048 (Odyssey 2) code all use full 8-bit opcodes that
        // fail this check within the first few bytes.
        for i in 0..HEADER_WORDS {
            if buffer[i * WORD_BYTES] > 0x01 {
                return Err(ParseError::MagicNotFound);
            }
        }

        let mut bidecles = [0u16; BIDECLE_COUNT];
        for (i, bidecle) in bidecles.iter_mut().enumerate() {
            *bidecle = decode_bidecle(buffer, i);
        }

        // Require at least one decoded BIDECLE in 0x5000-0x7FFF.  Both
        // EXEC and bypass ROMs have pointers into this range — EXEC
        // ROMs for their program start and title pointer, and bypass
        // ROMs because 0x5000 is still the primary cartridge ROM region.
        // This prevents false positives on files that merely start
        // with low-valued bytes (e.g. all zeros).
        //
        // We intentionally do NOT restrict individual BIDECLEs to
        // 0x0000 or 0x5000-0x7FFF, because some EXEC ROMs (e.g. Pinball)
        // use extended address ranges like 0xD000-0xDFFF for background
        // graphics or GRAM data.
        if !bidecles
            .iter()
            .any(|&b| (CART_ROM_BASE..=CART_ROM_END).contains(&b))
        {
            return Err(ParseError::MagicNotFound);
        }

        // BIDECLE 2 is the program start address.  A non-zero value
        // indicates an EXEC ROM (the EXEC will jump here after showing
        // the title screen).  Zero means the ROM bypasses EXEC via
        // code at 0x4800 or 0x7000.
        let program_start = bidecles[BIDECLE_PROGRAM_START];
        let uses_exec = program_start != 0;

        // Title/year extraction only makes sense for EXEC ROMs.
        // For bypass ROMs, BIDECLE 5 is not a title pointer — it is
        // whatever code or data the game has at 0x500A-0x500B, and
        // following it produces garbage (e.g. "HF" from Hover Force
        // or "V" from Spiker).
        let title_ptr = bidecles[BIDECLE_TITLE_PTR];
        let (title, year) = if uses_exec && title_ptr >= CART_ROM_BASE {
            extract_title_year(buffer, title_ptr)
        } else {
            (None, None)
        };

        // The flags DECLE at 0x500C is read by the EXEC during its
        // initialization sequence.  For bypass ROMs the EXEC never
        // reads this word, so the value at file offset 24 is
        // unrelated ROM content — we don't parse it.
        let (boot_mode, intv2_compat, skips_ecs_title, runs_post_title_code) = if uses_exec {
            let flags = u16::from_be_bytes([buffer[FLAGS_OFFSET], buffer[FLAGS_OFFSET + 1]]);
            (
                BootMode::Exec,
                flags & FLAG_INTV2_COMPAT != 0,
                flags & FLAG_SKIP_ECS_TITLE != 0,
                flags & FLAG_POST_TITLE_CODE != 0,
            )
        } else {
            (BootMode::ExecBypass, false, false, false)
        };

        let size_kb = buffer.len() / 1024;
        let rom_sha1 = compute_sha1(buffer);

        Ok(IntellivisionInfo {
            boot_mode,
            title,
            year,
            program_start,
            intv2_compat,
            skips_ecs_title,
            runs_post_title_code,
            size_kb,
            rom_sha1,
        })
    }
}

impl std::fmt::Display for IntellivisionInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.boot_mode {
            BootMode::Exec => writeln!(f, "Boot: Title Screen (EXEC)")?,
            BootMode::ExecBypass => writeln!(f, "Boot: Direct (EXEC bypass)")?,
        }
        if let Some(title) = &self.title {
            writeln!(f, "Title: {}", title)?;
        }
        if let Some(year) = self.year {
            writeln!(f, "Year: {}", year)?;
        }
        if self.boot_mode == BootMode::Exec {
            if self.intv2_compat {
                writeln!(f, "Intellivision 2: Compatible")?;
            } else {
                writeln!(f, "Intellivision 2: Not Compatible")?;
            }
            if self.skips_ecs_title {
                writeln!(f, "ECS Title Screen: Skipped")?;
            }
        }
        writeln!(f, "Size: {} KB", self.size_kb)?;
        if self.program_start != 0 {
            writeln!(f, "Program Start: {:#06X}", self.program_start)?;
        }
        writeln!(f, "{}", self as &dyn RomHash)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal valid Intellivision ROM with the given BIDECLE values
    /// and optional flags DECLE at byte offset 24.
    #[allow(clippy::arithmetic_side_effects)]
    fn build_rom(bidecles: &[u16; 6], flags: u16, extra: &[(usize, &[u8])]) -> Vec<u8> {
        let mut buf = vec![0u8; MIN_ROM_SIZE];
        for (i, &val) in bidecles.iter().enumerate() {
            let lo = (val & 0xFF) as u8;
            let hi = ((val >> 8) & 0xFF) as u8;
            let off = i * BIDECLE_BYTES;
            buf[off] = 0x00;
            buf[off + 1] = lo;
            buf[off + 2] = 0x00;
            buf[off + 3] = hi;
        }
        let flags_bytes = flags.to_be_bytes();
        buf[FLAGS_OFFSET] = flags_bytes[0];
        buf[FLAGS_OFFSET + 1] = flags_bytes[1];
        for &(offset, data) in extra {
            buf[offset..offset + data.len()].copy_from_slice(data);
        }
        buf
    }

    #[test]
    fn detects_exec_rom() {
        let rom = build_rom(
            &[0x5000, 0x5100, 0x5200, 0x5300, 0x5400, 0x0000],
            0x0000,
            &[],
        );
        let info = IntellivisionInfo::try_from(rom.as_slice()).unwrap();
        assert_eq!(info.boot_mode, BootMode::Exec);
        assert_eq!(info.program_start, 0x5200);
        assert_eq!(info.size_kb, 2);
        assert!(!info.intv2_compat);
    }

    #[test]
    fn detects_exec_rom_with_flags() {
        let rom = build_rom(
            &[0x5000, 0x5100, 0x5200, 0x5300, 0x5400, 0x0000],
            0x03C0, // INTV2 + post-title + ECS skip
            &[],
        );
        let info = IntellivisionInfo::try_from(rom.as_slice()).unwrap();
        assert_eq!(info.boot_mode, BootMode::Exec);
        assert!(info.intv2_compat);
        assert!(info.skips_ecs_title);
        assert!(info.runs_post_title_code);
    }

    #[test]
    fn detects_non_exec_rom() {
        // Bypass ROMs have program_start = 0 because they enter via
        // 0x4800/0x7000, not through the EXEC header.
        let rom = build_rom(
            &[0x0000, 0x0000, 0x0000, 0x0000, 0x500D, 0x500E],
            0x0000,
            &[],
        );
        let info = IntellivisionInfo::try_from(rom.as_slice()).unwrap();
        assert_eq!(info.boot_mode, BootMode::ExecBypass);
        assert_eq!(info.program_start, 0);
        assert!(!info.intv2_compat);
        assert!(info.title.is_none());
    }

    #[test]
    fn accepts_extended_address_ranges() {
        // Pinball uses BIDECLEs pointing to 0xD000-0xDFFF for background
        // and GRAM data alongside standard 0x5000-0x7FFF pointers.
        let rom = build_rom(
            &[0x0000, 0x6E1B, 0x5048, 0xD5D4, 0xDE24, 0x6E2D],
            0x0180, // post-title + ECS skip
            &[],
        );
        let info = IntellivisionInfo::try_from(rom.as_slice()).unwrap();
        assert_eq!(info.boot_mode, BootMode::Exec);
        assert_eq!(info.program_start, 0x5048);
        assert!(!info.intv2_compat);
        assert!(info.skips_ecs_title);
    }

    #[test]
    fn rejects_too_small() {
        let rom = vec![0u8; 1024];
        assert!(IntellivisionInfo::try_from(rom.as_slice()).is_err());
    }

    #[test]
    fn rejects_high_byte_above_one() {
        let mut rom = build_rom(
            &[0x0000, 0x0000, 0x5200, 0x0000, 0x0000, 0x0000],
            0x0000,
            &[],
        );
        rom[0] = 0x02;
        assert!(IntellivisionInfo::try_from(rom.as_slice()).is_err());
    }

    #[test]
    fn rejects_no_cart_rom_pointer() {
        // All BIDECLEs zero — no pointer into 0x5000-0x7FFF.
        let rom = build_rom(
            &[0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000],
            0x0000,
            &[],
        );
        assert!(IntellivisionInfo::try_from(rom.as_slice()).is_err());
    }

    #[test]
    fn extracts_title_and_year() {
        // Title pointer 0x5100 → file offset (0x100 * 2) = 0x200
        let title_data: &[u8] = &[
            0x00, 81, // year = 81 → 1981
            0x00, b'H', 0x00, b'i', 0x00, 0x00, // "Hi" + null
        ];
        let rom = build_rom(
            &[0x0000, 0x0000, 0x5200, 0x0000, 0x0000, 0x5100],
            0x0000,
            &[(0x200, title_data)],
        );
        let info = IntellivisionInfo::try_from(rom.as_slice()).unwrap();
        assert_eq!(info.title, Some("Hi".to_string()));
        assert_eq!(info.year, Some(1981));
    }

    #[test]
    fn no_title_when_pointer_zero() {
        let rom = build_rom(
            &[0x0000, 0x0000, 0x5200, 0x0000, 0x0000, 0x0000],
            0x0000,
            &[],
        );
        let info = IntellivisionInfo::try_from(rom.as_slice()).unwrap();
        assert_eq!(info.title, None);
        assert_eq!(info.year, None);
    }
}
