use crc::{CRC_32_ISO_HDLC, Crc};
use md5::Md5;
use sha1::{Digest, Sha1};
use sha2::Sha256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Hashes {
    pub crc32: u32,
    pub md5: [u8; 16],
    pub sha1: [u8; 20],
    pub sha256: [u8; 32],
}

impl Hashes {
    pub fn crc32_hex(&self) -> String {
        format!("{:08X}", self.crc32)
    }

    pub fn crc32_hex_lower(&self) -> String {
        format!("{:08x}", self.crc32)
    }

    pub fn md5_hex_lower(&self) -> String {
        hex_lower(&self.md5)
    }

    pub fn sha1_hex(&self) -> String {
        hex_upper(&self.sha1)
    }

    pub fn sha1_hex_lower(&self) -> String {
        hex_lower(&self.sha1)
    }

    pub fn sha256_hex_lower(&self) -> String {
        hex_lower(&self.sha256)
    }
}

pub fn compute_hashes(data: &[u8]) -> Hashes {
    let crc_algo = Crc::<u32>::new(&CRC_32_ISO_HDLC);
    let crc32 = crc_algo.checksum(data);

    let md5: [u8; 16] = {
        let mut h = Md5::new();
        h.update(data);
        h.finalize().into()
    };

    let sha1: [u8; 20] = {
        let mut h = Sha1::new();
        h.update(data);
        h.finalize().into()
    };

    let sha256: [u8; 32] = {
        let mut h = Sha256::new();
        h.update(data);
        h.finalize().into()
    };

    Hashes {
        crc32,
        md5,
        sha1,
        sha256,
    }
}

pub fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

pub fn hex_upper(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02X}", b)).collect()
}

pub fn hex_upper_spaced(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn format_sha1(hash: &[u8; 20]) -> String {
    hex_upper(hash)
}
