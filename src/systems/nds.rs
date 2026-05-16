// References:
//   Nintendo DS cartridge header:
//     https://problemkaputt.de/gbatek.htm#dscartridgeheader

use byteorder::{ByteOrder, LittleEndian};
use crc::{CRC_16_MODBUS, Crc};

use crate::traits::error::ParseError;
use crate::traits::rominfo::{DatMeta, RomInfo};
use crate::traits::stored_checksum::StoredChecksum;

use super::helpers::{dat_revision, nintendo_region_dat, nintendo_region_display, non_empty};

mod licensee;
use licensee::lookup_nds_licensee;

// Nintendo DS header offsets used for lightweight authenticity checks.
const NDS_HEADER_MIN_BYTES: usize = 0x160;
const NDS_TITLE_START: usize = 0x00;
const NDS_TITLE_END: usize = 0x0C;
const NDS_GAME_CODE_START: usize = 0x0C;
const NDS_GAME_CODE_END: usize = 0x10;
const NDS_MAKER_CODE_START: usize = 0x10;
const NDS_MAKER_CODE_END: usize = 0x12;
const NDS_CAPACITY_OFFSET: usize = 0x14;
const NDS_VERSION_OFFSET: usize = 0x01E;
const NDS_HEADER_SIZE_START: usize = 0x84;
const NDS_HEADER_SIZE_END: usize = 0x86;
const NDS_LOGO_CRC_START: usize = 0x15C;
const NDS_LOGO_CRC_END: usize = 0x15E;
const NDS_HEADER_CRC_START: usize = 0x15E;
const NDS_HEADER_CRC_END: usize = 0x160;
const NDS_EXPECTED_LOGO_CRC: u16 = 0xCF56;
const NDS_EXPECTED_HEADER_SIZE: u16 = 0x4000;

// Header CRC at 0x15E covers bytes 0x000..0x15D (everything up to the CRC itself).
const NDS_MODBUS: Crc<u16> = Crc::<u16>::new(&CRC_16_MODBUS);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NdsParseError {
    BufferTooSmall { minimum: usize },
    InvalidLogoChecksum { found: u16 },
    InvalidHeaderSize { found: u16 },
    InvalidTitleEncoding,
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct NdsRomInfo {
    title: String,
    game_code: [u8; 4],
    maker_code: [u8; 2],
    capacity: u8,
    version: u8,
    stored_header_crc: u16,
    calced_header_crc: u16,
}

impl NdsRomInfo {
    /// Device-declared ROM size in MiB, from the capacity byte at 0x14.
    /// GBATEK: chipsize = 128 KiB << cap; we surface it as MiB (cap-3) for retail-range values.
    fn rom_size_mib(&self) -> Option<u32> {
        self.capacity
            .checked_sub(3)
            .and_then(|shift| 1u32.checked_shl(shift.into()))
    }
}

impl RomInfo for NdsRomInfo {
    fn console(&self) -> &'static str {
        "Nintendo DS"
    }

    fn dat_meta(&self) -> DatMeta {
        let serial = String::from_utf8(self.game_code.to_vec()).ok();
        DatMeta {
            title: non_empty(&self.title),
            region: nintendo_region_dat(self.game_code[3]),
            version: dat_revision(self.version),
            manufacturer: lookup_nds_licensee(&self.maker_code).map(String::from),
            serial: serial.as_deref().and_then(non_empty),
            machine_id: serial.as_deref().and_then(non_empty),
            ..DatMeta::default()
        }
    }
}

impl StoredChecksum<u16> for NdsRomInfo {
    fn stored_checksum(&self) -> u16 {
        self.stored_header_crc
    }

    fn calculated_checksum(&self) -> u16 {
        self.calced_header_crc
    }
}

fn parse_nds_header(buffer: &[u8]) -> Result<NdsRomInfo, NdsParseError> {
    if buffer.len() < NDS_HEADER_MIN_BYTES {
        return Err(NdsParseError::BufferTooSmall {
            minimum: NDS_HEADER_MIN_BYTES,
        });
    }

    let logo_checksum = LittleEndian::read_u16(&buffer[NDS_LOGO_CRC_START..NDS_LOGO_CRC_END]);
    if logo_checksum != NDS_EXPECTED_LOGO_CRC {
        return Err(NdsParseError::InvalidLogoChecksum {
            found: logo_checksum,
        });
    }

    let header_size = LittleEndian::read_u16(&buffer[NDS_HEADER_SIZE_START..NDS_HEADER_SIZE_END]);
    if header_size != NDS_EXPECTED_HEADER_SIZE {
        return Err(NdsParseError::InvalidHeaderSize { found: header_size });
    }

    let title = String::from_utf8(buffer[NDS_TITLE_START..NDS_TITLE_END].to_vec())
        .map_err(|_| NdsParseError::InvalidTitleEncoding)?
        .trim_end_matches(|c: char| c == '\0' || c.is_ascii_whitespace())
        .to_string();
    let game_code: [u8; 4] = buffer[NDS_GAME_CODE_START..NDS_GAME_CODE_END]
        .try_into()
        .expect("slice length is 4");
    let maker_code: [u8; 2] = buffer[NDS_MAKER_CODE_START..NDS_MAKER_CODE_END]
        .try_into()
        .expect("slice length is 2");
    let stored_header_crc =
        LittleEndian::read_u16(&buffer[NDS_HEADER_CRC_START..NDS_HEADER_CRC_END]);
    let calced_header_crc = NDS_MODBUS.checksum(&buffer[..NDS_HEADER_CRC_START]);

    Ok(NdsRomInfo {
        title,
        game_code,
        maker_code,
        capacity: buffer[NDS_CAPACITY_OFFSET],
        version: buffer[NDS_VERSION_OFFSET],
        stored_header_crc,
        calced_header_crc,
    })
}

impl TryFrom<&[u8]> for NdsRomInfo {
    type Error = ParseError;

    fn try_from(buffer: &[u8]) -> Result<Self, Self::Error> {
        parse_nds_header(buffer).map_err(|e| match e {
            NdsParseError::BufferTooSmall { .. } => ParseError::BufferTooSmall,
            NdsParseError::InvalidLogoChecksum { .. } | NdsParseError::InvalidHeaderSize { .. } => {
                ParseError::MagicNotFound
            }
            NdsParseError::InvalidTitleEncoding => ParseError::InvalidHeader,
        })
    }
}

impl std::fmt::Display for NdsRomInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Name: {}", self.title)?;
        writeln!(f, "Code: {}", String::from_utf8_lossy(&self.game_code))?;
        writeln!(f, "Region: {}", nintendo_region_display(self.game_code[3]))?;
        writeln!(f, "Version: {}", self.version)?;
        let maker_str = String::from_utf8_lossy(&self.maker_code);
        match lookup_nds_licensee(&self.maker_code) {
            Some(name) => writeln!(f, "Maker: {name} (\"{maker_str}\")")?,
            None => writeln!(f, "Maker: \"{maker_str}\"")?,
        }
        match self.rom_size_mib() {
            Some(mib) => writeln!(f, "ROM Size: {} MiB", mib)?,
            None => writeln!(
                f,
                "ROM Size: unknown (capacity byte {:#04X})",
                self.capacity
            )?,
        }
        writeln!(f, "{}", self as &dyn StoredChecksum<u16>)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a synthetic NDS header buffer with all magic fields and the
    /// header CRC set to valid values.
    fn make_header() -> Vec<u8> {
        let mut buf = vec![0u8; 0x200];
        buf[NDS_TITLE_START..NDS_TITLE_START + 9].copy_from_slice(b"TEST GAME");
        buf[NDS_GAME_CODE_START..NDS_GAME_CODE_END].copy_from_slice(b"ATSE");
        buf[NDS_MAKER_CODE_START..NDS_MAKER_CODE_END].copy_from_slice(b"01");
        buf[NDS_CAPACITY_OFFSET] = 9;
        buf[NDS_VERSION_OFFSET] = 2;
        buf[NDS_HEADER_SIZE_START..NDS_HEADER_SIZE_END]
            .copy_from_slice(&NDS_EXPECTED_HEADER_SIZE.to_le_bytes());
        buf[NDS_LOGO_CRC_START..NDS_LOGO_CRC_END]
            .copy_from_slice(&NDS_EXPECTED_LOGO_CRC.to_le_bytes());
        let crc = NDS_MODBUS.checksum(&buf[..NDS_HEADER_CRC_START]);
        buf[NDS_HEADER_CRC_START..NDS_HEADER_CRC_END].copy_from_slice(&crc.to_le_bytes());
        buf
    }

    #[test]
    fn parses_valid_header() {
        let buf = make_header();
        let info = NdsRomInfo::try_from(buf.as_slice()).expect("valid header should parse");
        assert_eq!(info.title, "TEST GAME");
        assert_eq!(&info.game_code, b"ATSE");
        assert_eq!(&info.maker_code, b"01");
        assert_eq!(info.capacity, 9);
        assert_eq!(info.version, 2);
        assert_eq!(info.rom_size_mib(), Some(64));
        assert!(info.checksum_matches());
    }

    #[test]
    fn rejects_buffer_too_small() {
        let buf = vec![0u8; 0x100];
        assert_eq!(
            NdsRomInfo::try_from(buf.as_slice()),
            Err(ParseError::BufferTooSmall)
        );
    }

    #[test]
    fn rejects_invalid_logo_crc() {
        let mut buf = make_header();
        buf[NDS_LOGO_CRC_START] ^= 0xFF;
        assert_eq!(
            NdsRomInfo::try_from(buf.as_slice()),
            Err(ParseError::MagicNotFound)
        );
    }

    #[test]
    fn rejects_invalid_header_size() {
        let mut buf = make_header();
        buf[NDS_HEADER_SIZE_START..NDS_HEADER_SIZE_END].copy_from_slice(&0x1234u16.to_le_bytes());
        assert_eq!(
            NdsRomInfo::try_from(buf.as_slice()),
            Err(ParseError::MagicNotFound)
        );
    }

    #[test]
    fn detects_header_crc_mismatch_without_failing_parse() {
        let mut buf = make_header();
        buf[NDS_VERSION_OFFSET] = 0xAA;
        let info = NdsRomInfo::try_from(buf.as_slice()).expect("parse should still succeed");
        assert!(!info.checksum_matches());
    }

    #[test]
    fn rom_size_mib_handles_retail_capacities() {
        let mut info = NdsRomInfo::default();
        for (cap, expected) in [(3u8, 1u32), (6, 8), (9, 64), (12, 512)] {
            info.capacity = cap;
            assert_eq!(info.rom_size_mib(), Some(expected), "capacity {cap}");
        }
    }

    #[test]
    fn rom_size_mib_rejects_out_of_range() {
        let mut info = NdsRomInfo::default();
        for cap in [0u8, 2, 35, 0xFF] {
            info.capacity = cap;
            assert_eq!(info.rom_size_mib(), None, "capacity {cap}");
        }
    }

    #[test]
    fn dat_meta_populates_expected_fields() {
        let buf = make_header();
        let meta = NdsRomInfo::try_from(buf.as_slice()).unwrap().dat_meta();
        assert_eq!(meta.title.as_deref(), Some("TEST GAME"));
        assert_eq!(meta.serial.as_deref(), Some("ATSE"));
        assert_eq!(meta.machine_id.as_deref(), Some("ATSE"));
        assert_eq!(meta.region.as_deref(), Some("USA"));
        assert_eq!(meta.manufacturer.as_deref(), Some("Nintendo"));
        assert_eq!(meta.version.as_deref(), Some("Rev 2"));
    }
}
