// References:
//   Tiger Game.com cartridge header (community reverse-engineered):
//     https://problemkaputt.de/gamecomdocs.htm

use byteorder::{BigEndian, ByteOrder};

use crate::traits::error::ParseError;
use crate::traits::rominfo::RomInfo;

const CARTRIDGE_SIGNATURE: &[u8; 9] = b"TigerDMGC";
const CARTRIDGE_SIGNATURE_HEADER_DELTA: usize = 5;
const FLAGS_OFFSET: usize = 4;
const ICON_OFFSET: usize = 14;
const PROGRAM_STRING_START: usize = 17;
const PROGRAM_STRING_END: usize = 26;
const PROGRAM_ID_START: usize = 26;
const PROGRAM_ID_END: usize = 28;
const HEADER_MIN_BYTES: usize = PROGRAM_ID_END;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GamecomParseError {
    SignatureNotFound,
    InvalidSignaturePlacement,
    TruncatedHeader,
    InvalidProgramStringLength,
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct GamecomRomInfo {
    program_string: [u8; 9],
    program_id: u16,
    allowed_slot1: bool,
    allowed_slot2: bool,
    data_only: bool,
    icon_present: bool,
}

impl RomInfo for GamecomRomInfo {
    fn console(&self) -> &'static str {
        "game.com"
    }
}

/// Parse key game.com cartridge metadata from the cartridge signature block.
///
/// Research notes:
/// - `TigerDMGC` identifies game.com cartridge headers.
/// - The parser backs up 5 bytes from the marker to the start of the metadata
///   structure used for slot flags, icon flag, program string, and program ID.
#[allow(clippy::arithmetic_side_effects)]
fn parse_gamecom_info(buffer: &[u8]) -> Result<GamecomRomInfo, GamecomParseError> {
    let signature_location = buffer
        .windows(CARTRIDGE_SIGNATURE.len())
        .position(|window| window == CARTRIDGE_SIGNATURE)
        .ok_or(GamecomParseError::SignatureNotFound)?;
    let header_location = signature_location
        .checked_sub(CARTRIDGE_SIGNATURE_HEADER_DELTA)
        .ok_or(GamecomParseError::InvalidSignaturePlacement)?;

    if buffer.len() < header_location + HEADER_MIN_BYTES {
        return Err(GamecomParseError::TruncatedHeader);
    }

    let program_string: [u8; 9] = buffer
        [header_location + PROGRAM_STRING_START..header_location + PROGRAM_STRING_END]
        .try_into()
        .map_err(|_| GamecomParseError::InvalidProgramStringLength)?;
    let flags = buffer[header_location + FLAGS_OFFSET];

    Ok(GamecomRomInfo {
        program_string,
        program_id: BigEndian::read_u16(
            &buffer[header_location + PROGRAM_ID_START..header_location + PROGRAM_ID_END],
        ),
        allowed_slot1: flags & 0b0000_0001 != 0,
        allowed_slot2: flags & 0b0000_0010 != 0,
        data_only: flags & 0b0000_0100 != 0,
        icon_present: buffer[header_location + ICON_OFFSET] != 0,
    })
}

impl TryFrom<&[u8]> for GamecomRomInfo {
    type Error = ParseError;

    fn try_from(buffer: &[u8]) -> Result<Self, Self::Error> {
        parse_gamecom_info(buffer).map_err(|e| match e {
            GamecomParseError::SignatureNotFound => ParseError::MagicNotFound,
            GamecomParseError::InvalidSignaturePlacement | GamecomParseError::TruncatedHeader => {
                ParseError::BufferTooSmall
            }
            GamecomParseError::InvalidProgramStringLength => ParseError::InvalidHeader,
        })
    }
}

impl std::fmt::Display for GamecomRomInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let program_name_bytes = String::from_utf8_lossy(&self.program_string);
        let program_name = program_name_bytes.trim_end_matches('\0').trim();
        writeln!(
            f,
            "Name: {}",
            if program_name.is_empty() {
                "Unknown"
            } else {
                program_name
            }
        )?;
        writeln!(f, "Program Id: {:04X}", self.program_id)?;
        write!(
            f,
            "Allowed in Slot: {}",
            match [self.allowed_slot1, self.allowed_slot2] {
                [true, true] => "1 & 2",
                [true, false] => "1",
                [false, true] => "2",
                [false, false] => "?",
            }
        )?;
        if self.data_only {
            write!(f, "\nData-only Cartridge")?;
        }
        if !self.icon_present {
            write!(f, "\nNo icon present")?;
        }
        Ok(())
    }
}
