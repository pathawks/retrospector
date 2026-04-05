// References:
//   ColecoVision cartridge header and boot sequence:
//     https://www.smspower.org/Development/ColecoROMHeader
//   ColecoVision technical reference:
//     http://www.atarihq.com/danb/files/CV-Tech.txt

use super::helpers::compute_sha1;
use crate::traits::{error::ParseError, rom_hash::RomHash, rominfo::RomInfo};

// ColecoVision cartridge header layout within each 4KB ROM page.
const ROM_PAGE_BYTES: usize = 0x1000;
const BOOT_MAGIC_LEN: usize = 2;
const BOOT_MAGIC_TITLE_SCREEN: [u8; BOOT_MAGIC_LEN] = [0xAA, 0x55];
const BOOT_MAGIC_DIRECT_BOOT: [u8; BOOT_MAGIC_LEN] = [0x55, 0xAA];
const TITLE_START_OFFSET: usize = 0x24;
const TITLE_MAX_BYTES: usize = 96;
const ENTRY_POINT_LOW_OFFSET: usize = 0x0A;
const ENTRY_POINT_HIGH_OFFSET: usize = 0x0B;

// Character encodings seen in ColecoVision title strings.
const TITLE_TERMINATOR: u8 = 0x00;
const TITLE_COPYRIGHT_GLYPH: u8 = 0x1D;
const TITLE_TM_PREFIX: u8 = 0x1E;
const TITLE_TM_SUFFIX: u8 = 0x1F;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BootMode {
    TitleScreen, // AA 55 — BIOS shows title screen; title text at header+0x24
    DirectBoot,  // 55 AA — BIOS jumps straight to entry point
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ColecoVisionInfo {
    pub boot_mode: BootMode,
    pub title: Option<String>,
    pub copyright_year: Option<String>,
    pub entry_point: u16,
    pub size_kb: usize,
    pub rom_sha1: [u8; 20],
}

impl Default for ColecoVisionInfo {
    fn default() -> Self {
        Self {
            boot_mode: BootMode::DirectBoot,
            title: None,
            copyright_year: None,
            entry_point: 0,
            size_kb: 0,
            rom_sha1: [0u8; 20],
        }
    }
}

impl RomHash for ColecoVisionInfo {
    fn sha1(&self) -> [u8; 20] {
        self.rom_sha1
    }
}

impl RomInfo for ColecoVisionInfo {
    fn console(&self) -> &'static str {
        "ColecoVision"
    }
}

#[allow(clippy::arithmetic_side_effects)]
fn parse_title_lines(buffer: &[u8], header_offset: usize) -> Vec<String> {
    let start = header_offset + TITLE_START_OFFSET;
    let end = (start + TITLE_MAX_BYTES).min(buffer.len());
    if start >= end {
        return Vec::new();
    }

    let mut decoded = String::new();
    let title_bytes = &buffer[start..end];
    let mut i = 0;
    while i < title_bytes.len() {
        let b = title_bytes[i];
        match b {
            TITLE_TERMINATOR => break,
            TITLE_COPYRIGHT_GLYPH => decoded.push('©'),
            TITLE_TM_PREFIX => {
                decoded.push('™');
                if i + 1 < title_bytes.len() && title_bytes[i + 1] == TITLE_TM_SUFFIX {
                    i += 1;
                }
            }
            // Standalone continuation byte from the TM two-glyph pair.
            TITLE_TM_SUFFIX => {}
            b if b.is_ascii_graphic() || b == b' ' => decoded.push(b as char),
            _ => {
                if decoded.is_empty() {
                    i += 1;
                    continue;
                }
                break;
            }
        }
        i += 1;
    }

    decoded
        .split('/')
        .take(3)
        .map(str::trim)
        .map(|line| {
            line.strip_prefix("© ")
                .map_or_else(|| line.to_string(), |rest| format!("©{}", rest))
        })
        .filter(|line| !line.is_empty())
        .collect()
}

fn parse_copyright_year(line: &str) -> Option<String> {
    let trimmed = line.trim().trim_start_matches('©').trim();
    let year = trimmed.get(0..4)?;
    if year.chars().all(|c| c.is_ascii_digit())
        && (year.starts_with("19") || year.starts_with("20"))
    {
        Some(year.to_string())
    } else {
        None
    }
}

impl TryFrom<&[u8]> for ColecoVisionInfo {
    type Error = ParseError;

    #[allow(clippy::arithmetic_side_effects)]
    fn try_from(buffer: &[u8]) -> Result<Self, Self::Error> {
        // 4 KB is the fundamental ColecoVision ROM chip unit.
        // Valid cart sizes: 8, 12, 16, 24, 32 KB — all multiples of 4 KB.
        if buffer.len() < ROM_PAGE_BYTES || !buffer.len().is_multiple_of(ROM_PAGE_BYTES) {
            return Err(ParseError::BufferTooSmall);
        }

        // Scan each 4 KB boundary for the AA55/55AA magic.
        // Because len % page == 0, every `off` satisfies off + page <= len,
        // so the slice is always in bounds.
        //
        // The C3 (JP nn) at 0x0C is NOT required: it is the RST08 dispatch
        // slot, which the AA55 (title-screen) boot path uses. The 55AA
        // (direct-boot) variant skips that path, so those slots are
        // legitimately zeroed in many commercial games (e.g. Frantic Freddy).
        let header_offset = (0..buffer.len())
            .step_by(ROM_PAGE_BYTES)
            .find(|&off| {
                let page = &buffer[off..off + ROM_PAGE_BYTES];
                page[0..BOOT_MAGIC_LEN] == BOOT_MAGIC_TITLE_SCREEN
                    || page[0..BOOT_MAGIC_LEN] == BOOT_MAGIC_DIRECT_BOOT
            })
            .ok_or(ParseError::MagicNotFound)?;

        let boot_mode = if buffer[header_offset] == BOOT_MAGIC_TITLE_SCREEN[0] {
            BootMode::TitleScreen
        } else {
            BootMode::DirectBoot
        };

        // Title text is stored at header+0x24 as slash-delimited lines in
        // bottom-to-top order (LINE3/LINE2/YEAR).
        let mut title_lines = parse_title_lines(buffer, header_offset);
        let copyright_year = title_lines
            .last()
            .and_then(|line| parse_copyright_year(line));

        let title = if boot_mode == BootMode::TitleScreen {
            if copyright_year.is_some() {
                title_lines.pop();
            }
            let t = title_lines
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join(" / ");
            if t.is_empty() { None } else { Some(t) }
        } else {
            None
        };

        // Extract entry point (little-endian u16) from the matched bank header
        let entry_point = u16::from_le_bytes([
            buffer[header_offset + ENTRY_POINT_LOW_OFFSET],
            buffer[header_offset + ENTRY_POINT_HIGH_OFFSET],
        ]);

        let size_kb = buffer.len() / 1024;

        let rom_sha1 = compute_sha1(buffer);

        Ok(ColecoVisionInfo {
            boot_mode,
            title,
            copyright_year,
            entry_point,
            size_kb,
            rom_sha1,
        })
    }
}

impl std::fmt::Display for ColecoVisionInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mode = match self.boot_mode {
            BootMode::TitleScreen => "Title Screen (AA 55)",
            BootMode::DirectBoot => "Direct Boot (55 AA)",
        };
        writeln!(f, "Boot Mode: {}", mode)?;
        if let Some(title) = &self.title {
            writeln!(f, "Title: {}", title)?;
        }
        if let Some(copyright_year) = &self.copyright_year {
            writeln!(f, "Copyright Year: {}", copyright_year)?;
        }
        writeln!(f, "Size: {} KB", self.size_kb)?;
        writeln!(f, "Entry Point: {:#06X}", self.entry_point)?;
        writeln!(f, "{}", self as &dyn RomHash)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::arithmetic_side_effects)]
    fn parse_info(raw_title: &[u8], boot_magic: [u8; 2]) -> ColecoVisionInfo {
        let mut buffer = vec![0u8; 0x1000];
        buffer[0] = boot_magic[0];
        buffer[1] = boot_magic[1];
        let start = 0x24;
        buffer[start..start + raw_title.len()].copy_from_slice(raw_title);
        match ColecoVisionInfo::try_from(buffer.as_slice()) {
            Ok(info) => info,
            Err(e) => panic!("valid test ROM should parse: {e:?}"),
        }
    }

    #[test]
    fn title_screen_keeps_up_to_three_title_lines() {
        let info = parse_info(b"LINE ONE/LINE TWO/LINE THREE\0", [0xAA, 0x55]);
        assert_eq!(
            info.title,
            Some("LINE THREE / LINE TWO / LINE ONE".to_string())
        );
        assert_eq!(info.copyright_year, None);
    }

    #[test]
    fn title_screen_handles_single_line_titles() {
        let info = parse_info(b"JUST ONE\0", [0xAA, 0x55]);
        assert_eq!(info.title, Some("JUST ONE".to_string()));
        assert_eq!(info.copyright_year, None);
    }

    #[test]
    fn title_screen_skips_leading_control_bytes() {
        let info = parse_info(
            b"\x1D 1982 DAN GORLIN/BR0DERBUND'S CHOPLIFTER\x1E\x1F",
            [0xAA, 0x55],
        );
        assert_eq!(
            info.title,
            Some("BR0DERBUND'S CHOPLIFTER™ / ©1982 DAN GORLIN".to_string())
        );
        assert_eq!(info.copyright_year, None);
    }

    #[test]
    fn title_screen_allows_tm_pair_before_delimiter() {
        let info = parse_info(b"LINE\x1E\x1F/SECOND/2026\0", [0xAA, 0x55]);
        assert_eq!(info.title, Some("SECOND / LINE™".to_string()));
        assert_eq!(info.copyright_year, Some("2026".to_string()));
    }

    #[test]
    fn direct_boot_still_attempts_year_extraction() {
        let info = parse_info(b"LINE3/LINE2/1987\0", [0x55, 0xAA]);
        assert_eq!(info.title, None);
        assert_eq!(info.copyright_year, Some("1987".to_string()));
    }
}
