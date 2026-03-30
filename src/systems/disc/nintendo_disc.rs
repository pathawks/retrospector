// References:
//   GameCube disc format and header:
//     https://wiibrew.org/wiki/Wii_disc#Header
//   Wii disc format:
//     https://wiibrew.org/wiki/Wii_disc

//! Nintendo disc header parser for GameCube and Wii
//! Header is located at offset 0x0000

use byteorder::{BigEndian, ByteOrder};

// Nintendo optical-disc header layout (sector 0, user data).
const MIN_MAGIC_PROBE_BYTES: usize = 0x20;
const HEADER_MIN_BYTES: usize = 0x60;
const WII_MAGIC_OFFSET: usize = 0x18;
const WII_MAGIC_END: usize = 0x1C;
const GAMECUBE_MAGIC_OFFSET: usize = 0x1C;
const GAMECUBE_MAGIC_END: usize = 0x20;
const GAME_ID_START: usize = 0x00;
const GAME_ID_END: usize = 0x06;
const DISC_NUMBER_OFFSET: usize = 0x06;
const VERSION_OFFSET: usize = 0x07;
const AUDIO_STREAMING_OFFSET: usize = 0x08;
const TITLE_START: usize = 0x20;
const TITLE_END: usize = 0x60;
const REGION_CHAR_INDEX: usize = 3;

const WII_MAGIC: u32 = 0x5D1C9EA3;
const GAMECUBE_MAGIC: u32 = 0xC2339F3D;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NintendoDiscType {
    GameCube,
    Wii,
}

#[derive(Debug, Clone, Default)]
pub struct NintendoDiscHeader {
    pub disc_type: Option<NintendoDiscType>,
    pub game_id: String,
    pub disc_number: u8,
    pub version: u8,
    pub audio_streaming: bool,
    pub title: String,
    pub region_code: char,
}

/// Detect the type of Nintendo disc
/// Returns Wii if Wii magic is present (Wii discs have both magics, so check Wii first)
pub fn detect_nintendo_disc(buffer: &[u8]) -> Option<NintendoDiscType> {
    if buffer.len() < MIN_MAGIC_PROBE_BYTES {
        return None;
    }

    // Check Wii magic at 0x18 FIRST (Wii discs have both magics)
    let wii_magic = BigEndian::read_u32(&buffer[WII_MAGIC_OFFSET..WII_MAGIC_END]);
    if wii_magic == WII_MAGIC {
        return Some(NintendoDiscType::Wii);
    }

    // Check GameCube magic at 0x1C
    let gc_magic = BigEndian::read_u32(&buffer[GAMECUBE_MAGIC_OFFSET..GAMECUBE_MAGIC_END]);
    if gc_magic == GAMECUBE_MAGIC {
        return Some(NintendoDiscType::GameCube);
    }

    None
}

/// Parse Nintendo disc header
pub fn parse_nintendo_disc_header(buffer: &[u8]) -> Option<NintendoDiscHeader> {
    let disc_type = detect_nintendo_disc(buffer)?;

    if buffer.len() < HEADER_MIN_BYTES {
        return None;
    }

    let game_id = String::from_utf8_lossy(&buffer[GAME_ID_START..GAME_ID_END])
        .trim()
        .to_string();

    let region_code = if game_id.len() > REGION_CHAR_INDEX {
        game_id.chars().nth(REGION_CHAR_INDEX).unwrap_or('?')
    } else {
        '?'
    };

    Some(NintendoDiscHeader {
        disc_type: Some(disc_type),
        game_id,
        disc_number: buffer[DISC_NUMBER_OFFSET],
        version: buffer[VERSION_OFFSET],
        audio_streaming: buffer[AUDIO_STREAMING_OFFSET] != 0,
        title: String::from_utf8_lossy(&buffer[TITLE_START..TITLE_END])
            .trim_end_matches('\0')
            .trim()
            .to_string(),
        region_code,
    })
}

/// Decode region code character to region name
pub fn decode_region(region_code: char) -> &'static str {
    match region_code {
        'J' => "Japan",
        'E' => "North America",
        'P' => "Europe (PAL)",
        'D' => "Germany",
        'F' => "France",
        'I' => "Italy",
        'S' => "Spain",
        'H' => "Netherlands",
        'K' => "Korea",
        'W' => "Taiwan",
        'A' => "Australia",
        'R' => "Russia",
        _ => "Unknown",
    }
}

/// Map a region code character to a canonical No-Intro region string.
/// Returns `None` for unknown codes.
pub fn dat_region(region_code: char) -> Option<&'static str> {
    match region_code {
        'J' => Some("Japan"),
        'E' => Some("USA"),
        'P' => Some("Europe"),
        'D' => Some("Germany"),
        'F' => Some("France"),
        'I' => Some("Italy"),
        'S' => Some("Spain"),
        'H' => Some("Netherlands"),
        'K' => Some("Korea"),
        'W' => Some("Taiwan"),
        'A' => Some("Australia"),
        'R' => Some("Russia"),
        _ => None,
    }
}

impl std::fmt::Display for NintendoDiscHeader {
    #[allow(clippy::arithmetic_side_effects)]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Title: {}", self.title)?;
        writeln!(f, "Game ID: {}", self.game_id)?;
        writeln!(f, "Region: {}", decode_region(self.region_code))?;
        writeln!(f, "Version: 1.{:02}", self.version)?;
        writeln!(f, "Disc Number: {}", self.disc_number + 1)?;
        if self.audio_streaming {
            writeln!(f, "Audio Streaming: Enabled")?;
        }
        Ok(())
    }
}
