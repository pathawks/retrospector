// References:
//   NES mapper capabilities and ROM size constraints:
//     https://www.nesdev.org/wiki/Mapper

use super::super::helpers::detect_unique_size;
use super::mapper::Mapper;
use std::fmt;

/// Result of analyzing an NES ROM for potential issues
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NesAnalysis {
    /// If PRG ROM appears overdumped, the likely actual size in bytes
    pub prg_unique_bytes: Option<usize>,
    /// If CHR ROM appears overdumped, the likely actual size in bytes
    pub chr_unique_bytes: Option<usize>,
    /// Warnings about mapper/ROM incompatibilities
    pub warnings: Vec<AnalysisWarning>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AnalysisWarning {
    /// PRG ROM is larger than the mapper typically supports
    PrgTooLarge { mapper_max_kb: u32, actual_kb: u32 },
    /// CHR ROM is larger than the mapper typically supports
    ChrTooLarge { mapper_max_kb: u32, actual_kb: u32 },
    /// CHR ROM is present but the mapper uses CHR-RAM only
    UnexpectedChrRom,
    /// PRG ROM is entirely a single byte (e.g., all 0x00 or 0xFF)
    BlankPrg { fill_byte: u8 },
    /// CHR ROM is entirely a single byte (e.g., all 0x00 or 0xFF)
    BlankChr { fill_byte: u8 },
}

impl fmt::Display for AnalysisWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AnalysisWarning::PrgTooLarge {
                mapper_max_kb,
                actual_kb,
            } => {
                write!(
                    f,
                    "PRG ROM size ({}kb) exceeds mapper maximum ({}kb)",
                    actual_kb, mapper_max_kb
                )
            }
            AnalysisWarning::ChrTooLarge {
                mapper_max_kb,
                actual_kb,
            } => {
                write!(
                    f,
                    "CHR ROM size ({}kb) exceeds mapper maximum ({}kb)",
                    actual_kb, mapper_max_kb
                )
            }
            AnalysisWarning::UnexpectedChrRom => {
                write!(f, "CHR ROM present but mapper does not support CHR ROM")
            }
            AnalysisWarning::BlankPrg { fill_byte } => {
                write!(f, "PRG ROM is blank (entirely 0x{:02X})", fill_byte)
            }
            AnalysisWarning::BlankChr { fill_byte } => {
                write!(f, "CHR ROM is blank (entirely 0x{:02X})", fill_byte)
            }
        }
    }
}

/// Check if data is entirely filled with a single byte value (e.g., all 0x00 or 0xFF).
fn blank_fill_byte(data: &[u8]) -> Option<u8> {
    let &first = data.first()?;
    if data.iter().all(|&b| b == first) {
        Some(first)
    } else {
        None
    }
}

/// Analyze PRG and CHR ROM data for overdumps and mapper compatibility issues.
pub fn analyze(prg_data: &[u8], chr_data: Option<&[u8]>, mapper: &Mapper) -> NesAnalysis {
    let mut analysis = NesAnalysis::default();

    const MIN_BLOCK: usize = 8192; // 8KB minimum unique block

    // Blank ROM detection — check before overdump to avoid false positives
    let prg_blank = blank_fill_byte(prg_data);
    if let Some(fill_byte) = prg_blank {
        analysis
            .warnings
            .push(AnalysisWarning::BlankPrg { fill_byte });
    }

    let chr_blank = chr_data.and_then(blank_fill_byte);
    if let Some(fill_byte) = chr_blank {
        analysis
            .warnings
            .push(AnalysisWarning::BlankChr { fill_byte });
    }

    // Overdump detection — skip if data is blank (uniform fill is not a meaningful overdump)
    if prg_blank.is_none()
        && prg_data.len() > MIN_BLOCK
        && let Some(unique) = detect_unique_size(prg_data, MIN_BLOCK)
    {
        analysis.prg_unique_bytes = Some(unique);
    }

    if let Some(chr) = chr_data
        && chr_blank.is_none()
        && chr.len() > MIN_BLOCK
        && let Some(unique) = detect_unique_size(chr, MIN_BLOCK)
    {
        analysis.chr_unique_bytes = Some(unique);
    }

    // Mapper compatibility checks
    if let Some(limits) = mapper_limits(mapper) {
        let prg_kb = (prg_data.len() / 1024) as u32;
        let chr_kb = chr_data.map(|c| (c.len() / 1024) as u32).unwrap_or(0);

        if let Some(max_prg) = limits.max_prg_kb
            && prg_kb > max_prg
        {
            analysis.warnings.push(AnalysisWarning::PrgTooLarge {
                mapper_max_kb: max_prg,
                actual_kb: prg_kb,
            });
        }

        if let Some(max_chr) = limits.max_chr_kb {
            if max_chr == 0 && chr_kb > 0 {
                analysis.warnings.push(AnalysisWarning::UnexpectedChrRom);
            } else if max_chr > 0 && chr_kb > max_chr {
                analysis.warnings.push(AnalysisWarning::ChrTooLarge {
                    mapper_max_kb: max_chr,
                    actual_kb: chr_kb,
                });
            }
        }
    }

    analysis
}

struct MapperLimits {
    /// Maximum PRG ROM size in KB. None = unknown (don't warn).
    max_prg_kb: Option<u32>,
    /// Maximum CHR ROM size in KB. None = unknown. Some(0) = CHR-RAM only.
    max_chr_kb: Option<u32>,
}

fn mapper_limits(mapper: &Mapper) -> Option<MapperLimits> {
    Some(match mapper {
        Mapper::NROM => MapperLimits {
            max_prg_kb: Some(32),
            max_chr_kb: Some(8),
        },
        Mapper::MMC1 | Mapper::SxROM => MapperLimits {
            max_prg_kb: Some(512),
            max_chr_kb: Some(128),
        },
        Mapper::UxROM => MapperLimits {
            max_prg_kb: Some(256),
            max_chr_kb: Some(0),
        },
        Mapper::CNROM => MapperLimits {
            max_prg_kb: Some(32),
            max_chr_kb: Some(32),
        },
        Mapper::MMC3 | Mapper::TxSROM => MapperLimits {
            max_prg_kb: Some(512),
            max_chr_kb: Some(256),
        },
        Mapper::MMC5 => MapperLimits {
            max_prg_kb: Some(1024),
            max_chr_kb: Some(1024),
        },
        Mapper::AxROM => MapperLimits {
            max_prg_kb: Some(256),
            max_chr_kb: Some(0),
        },
        Mapper::MMC2 | Mapper::PxROM => MapperLimits {
            max_prg_kb: Some(128),
            max_chr_kb: Some(128),
        },
        Mapper::MMC4 => MapperLimits {
            max_prg_kb: Some(256),
            max_chr_kb: Some(128),
        },
        Mapper::ColorDreams => MapperLimits {
            max_prg_kb: Some(128),
            max_chr_kb: Some(128),
        },
        Mapper::CPROM => MapperLimits {
            max_prg_kb: Some(32),
            max_chr_kb: Some(0),
        },
        Mapper::ContraFunction16 => MapperLimits {
            max_prg_kb: Some(1024),
            max_chr_kb: Some(0),
        },
        Mapper::BandaiEPROM | Mapper::BandaiEPROM24C01 => MapperLimits {
            max_prg_kb: Some(256),
            max_chr_kb: Some(256),
        },
        Mapper::UNROM512 => MapperLimits {
            max_prg_kb: Some(512),
            max_chr_kb: Some(0),
        },
        Mapper::JalecoSS8806 => MapperLimits {
            max_prg_kb: Some(256),
            max_chr_kb: Some(256),
        },
        Mapper::Namco163 => MapperLimits {
            max_prg_kb: Some(512),
            max_chr_kb: Some(256),
        },
        Mapper::VRC2a => MapperLimits {
            max_prg_kb: Some(256),
            max_chr_kb: Some(128),
        },
        Mapper::VRC2b | Mapper::VRC4e => MapperLimits {
            max_prg_kb: Some(256),
            max_chr_kb: Some(256),
        },
        Mapper::VRC4a | Mapper::VRC4c => MapperLimits {
            max_prg_kb: Some(256),
            max_chr_kb: Some(256),
        },
        Mapper::VRC4b | Mapper::VRC4d => MapperLimits {
            max_prg_kb: Some(256),
            max_chr_kb: Some(256),
        },
        Mapper::VRC6a => MapperLimits {
            max_prg_kb: Some(256),
            max_chr_kb: Some(256),
        },
        Mapper::VRC6b => MapperLimits {
            max_prg_kb: Some(256),
            max_chr_kb: Some(256),
        },
        Mapper::BNROM | Mapper::NINA001 => MapperLimits {
            max_prg_kb: Some(128),
            max_chr_kb: Some(0),
        },
        Mapper::RAMBO1 => MapperLimits {
            max_prg_kb: Some(512),
            max_chr_kb: Some(256),
        },
        Mapper::GxROM | Mapper::MxROM => MapperLimits {
            max_prg_kb: Some(128),
            max_chr_kb: Some(32),
        },
        Mapper::AfterBurner => MapperLimits {
            max_prg_kb: Some(128),
            max_chr_kb: Some(256),
        },
        Mapper::FME7 => MapperLimits {
            max_prg_kb: Some(512),
            max_chr_kb: Some(256),
        },
        Mapper::Camerica => MapperLimits {
            max_prg_kb: Some(256),
            max_chr_kb: Some(0),
        },
        Mapper::VRC3 => MapperLimits {
            max_prg_kb: Some(128),
            max_chr_kb: Some(0),
        },
        Mapper::PirateMMC3 | Mapper::PirateMMC3_4k => MapperLimits {
            max_prg_kb: Some(512),
            max_chr_kb: Some(256),
        },
        Mapper::VRC1 => MapperLimits {
            max_prg_kb: Some(128),
            max_chr_kb: Some(128),
        },
        Mapper::Namco109 => MapperLimits {
            max_prg_kb: Some(128),
            max_chr_kb: Some(64),
        },
        Mapper::NINA03 | Mapper::NINA06 => MapperLimits {
            max_prg_kb: Some(64),
            max_chr_kb: Some(64),
        },
        Mapper::VRC7 => MapperLimits {
            max_prg_kb: Some(512),
            max_chr_kb: Some(128),
        },
        Mapper::JALECOJF13 => MapperLimits {
            max_prg_kb: Some(128),
            max_chr_kb: Some(64),
        },
        Mapper::SenjouNoOokami => MapperLimits {
            max_prg_kb: Some(128),
            max_chr_kb: Some(0),
        },
        Mapper::NESEVENT => MapperLimits {
            max_prg_kb: Some(256),
            max_chr_kb: Some(0),
        },
        Mapper::TQROM => MapperLimits {
            max_prg_kb: Some(128),
            max_chr_kb: Some(64),
        },
        Mapper::SUBOR166 | Mapper::SUBOR167 => MapperLimits {
            max_prg_kb: Some(1024),
            max_chr_kb: Some(0),
        },
        Mapper::CrazyClimber => MapperLimits {
            max_prg_kb: Some(128),
            max_chr_kb: Some(0),
        },
        Mapper::CNROMWithProtection => MapperLimits {
            max_prg_kb: Some(32),
            max_chr_kb: Some(8),
        },
        Mapper::DxROM => MapperLimits {
            max_prg_kb: Some(128),
            max_chr_kb: Some(64),
        },
        Mapper::Namco175 => MapperLimits {
            max_prg_kb: Some(512),
            max_chr_kb: Some(256),
        },
        Mapper::Action52 => MapperLimits {
            max_prg_kb: Some(1536),
            max_chr_kb: Some(512),
        },
        Mapper::CodemastersQuattro => MapperLimits {
            max_prg_kb: Some(256),
            max_chr_kb: Some(0),
        },
        Mapper::SuperRussianRoulette => MapperLimits {
            max_prg_kb: Some(512),
            max_chr_kb: Some(0),
        },
        Mapper::Unif158B => MapperLimits {
            max_prg_kb: Some(512),
            max_chr_kb: Some(0),
        },
        Mapper::Unrecognized | Mapper::Unknown { .. } => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_overdump_in_unique_data() {
        // Use a hash-like pattern that won't repeat at block boundaries
        let mut data = vec![0u8; 32768];
        for (i, byte) in data.iter_mut().enumerate() {
            *byte = (i.wrapping_mul(2654435761) >> 16) as u8; // Knuth multiplicative hash
        }
        assert_eq!(detect_unique_size(&data, 8192), None);
    }

    #[test]
    fn detects_2x_overdump() {
        let half: Vec<u8> = (0..16384).map(|i| (i % 251) as u8).collect();
        let mut data = half.clone();
        data.extend_from_slice(&half);
        assert_eq!(detect_unique_size(&data, 8192), Some(16384));
    }

    #[test]
    fn detects_4x_overdump() {
        let quarter: Vec<u8> = (0..8192).map(|i| (i % 251) as u8).collect();
        let mut data = Vec::new();
        for _ in 0..4 {
            data.extend_from_slice(&quarter);
        }
        assert_eq!(detect_unique_size(&data, 8192), Some(8192));
    }

    #[test]
    fn detects_non_power_of_2_overdump() {
        // 32KB = [A][B][C][A] where each block is 8KB
        // Unique data is 24KB (A, B, C), last 8KB mirrors the first
        let block_a: Vec<u8> = (0..8192).map(|i| (i % 251) as u8).collect();
        let block_b: Vec<u8> = (0..8192).map(|i| (i % 241) as u8).collect();
        let block_c: Vec<u8> = (0..8192).map(|i| (i % 239) as u8).collect();
        let mut data = Vec::new();
        data.extend_from_slice(&block_a);
        data.extend_from_slice(&block_b);
        data.extend_from_slice(&block_c);
        data.extend_from_slice(&block_a); // mirror of first block
        assert_eq!(detect_unique_size(&data, 8192), Some(24576)); // 24KB
    }

    #[test]
    fn does_not_go_below_min_block() {
        // 8KB of identical 4KB halves - should NOT detect since 8KB is min_block
        let quarter: Vec<u8> = (0..4096).map(|_| 0xAA).collect();
        let mut data = quarter.clone();
        data.extend_from_slice(&quarter);
        assert_eq!(detect_unique_size(&data, 8192), None);
    }

    #[test]
    fn skips_small_data() {
        let data = vec![0u8; 8192];
        assert_eq!(detect_unique_size(&data, 8192), None);
    }

    #[test]
    fn mapper_chr_ram_only_warning() {
        let prg = vec![0u8; 32768];
        let chr = vec![0u8; 8192];
        let result = analyze(&prg, Some(&chr), &Mapper::UxROM);
        assert!(
            result
                .warnings
                .iter()
                .any(|w| matches!(w, AnalysisWarning::UnexpectedChrRom))
        );
    }

    #[test]
    fn mapper_prg_too_large_warning() {
        let prg = vec![0u8; 64 * 1024]; // 64KB, NROM max is 32KB
        let result = analyze(&prg, None, &Mapper::NROM);
        assert!(
            result
                .warnings
                .iter()
                .any(|w| matches!(w, AnalysisWarning::PrgTooLarge { .. }))
        );
    }

    #[test]
    fn no_warnings_for_valid_rom() {
        let prg: Vec<u8> = (0..32768).map(|i| (i % 251) as u8).collect();
        let chr: Vec<u8> = (0..8192).map(|i| (i % 239) as u8).collect();
        let result = analyze(&prg, Some(&chr), &Mapper::NROM);
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn unknown_mapper_no_warnings() {
        let prg: Vec<u8> = (0..1024 * 1024).map(|i| (i % 251) as u8).collect();
        let chr: Vec<u8> = (0..512 * 1024).map(|i| (i % 239) as u8).collect();
        let result = analyze(
            &prg,
            Some(&chr),
            &Mapper::Unknown {
                ines_mapper_number: 999,
            },
        );
        assert!(
            result
                .warnings
                .iter()
                .all(|w| !matches!(w, AnalysisWarning::PrgTooLarge { .. }))
        );
    }

    #[test]
    fn detects_blank_prg_all_ff() {
        let prg = vec![0xFFu8; 256 * 1024];
        let chr: Vec<u8> = (0..8192).map(|i| (i % 251) as u8).collect();
        let result = analyze(&prg, Some(&chr), &Mapper::MMC3);
        assert!(
            result
                .warnings
                .iter()
                .any(|w| matches!(w, AnalysisWarning::BlankPrg { fill_byte: 0xFF }))
        );
        // Should NOT report overdump for blank data
        assert_eq!(result.prg_unique_bytes, None);
    }

    #[test]
    fn detects_blank_chr_all_00() {
        let prg: Vec<u8> = (0..32768).map(|i| (i % 251) as u8).collect();
        let chr = vec![0x00u8; 256 * 1024];
        let result = analyze(&prg, Some(&chr), &Mapper::MMC3);
        assert!(
            result
                .warnings
                .iter()
                .any(|w| matches!(w, AnalysisWarning::BlankChr { fill_byte: 0x00 }))
        );
        assert_eq!(result.chr_unique_bytes, None);
    }

    #[test]
    fn blank_prg_suppresses_overdump() {
        // All-0xFF PRG should be flagged as blank, not overdump
        let prg = vec![0xFFu8; 32 * 1024];
        let result = analyze(&prg, None, &Mapper::NROM);
        assert!(
            result
                .warnings
                .iter()
                .any(|w| matches!(w, AnalysisWarning::BlankPrg { .. }))
        );
        assert_eq!(result.prg_unique_bytes, None);
    }
}
