// References:
//   Nintendo region code conventions (J/E/P/D/F/I/S/H/K):
//     https://wiibrew.org/wiki/Country_Codes

use sha1::{Digest, Sha1};

pub fn compute_sha1(buffer: &[u8]) -> [u8; 20] {
    let mut hasher = Sha1::new();
    hasher.update(buffer);
    hasher.finalize().into()
}

pub fn non_empty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_string())
}

pub fn first_non_empty(values: &[&str]) -> Option<String> {
    values
        .iter()
        .copied()
        .find(|value| !value.is_empty())
        .map(String::from)
}

pub fn dat_revision(version: u8) -> Option<String> {
    (version != 0).then(|| format!("Rev {}", version))
}

pub fn nintendo_region_dat(region_code: u8) -> Option<String> {
    match region_code {
        b'J' => Some("Japan"),
        b'E' => Some("USA"),
        b'P' => Some("Europe"),
        b'D' => Some("Germany"),
        b'F' => Some("France"),
        b'I' => Some("Italy"),
        b'S' => Some("Spain"),
        b'H' => Some("Netherlands"),
        b'K' => Some("Korea"),
        b'A' => Some("Asia"),
        b'O' => Some("World"),
        _ => None,
    }
    .map(String::from)
}

/// Find the smallest prefix of the data that, when repeated, reproduces the full data.
/// Checks every candidate size that is a multiple of `block_size`.
#[allow(clippy::arithmetic_side_effects)]
pub fn find_unique_size(data: &[u8], block_size: usize) -> usize {
    let len = data.len();
    if len <= block_size {
        return len;
    }
    // Try each candidate true size from smallest to largest
    let mut size = block_size;
    while size < len {
        let is_mirror = data[size..]
            .iter()
            .enumerate()
            .all(|(i, &byte)| byte == data[i % size]);
        if is_mirror {
            return size;
        }
        size += block_size;
    }
    len
}

/// Detect if data contains repeated blocks indicating an overdump.
/// Returns the size of the unique data if overdump is detected, None otherwise.
pub fn detect_unique_size(data: &[u8], min_block: usize) -> Option<usize> {
    let unique = find_unique_size(data, min_block);
    if unique < data.len() {
        Some(unique)
    } else {
        None
    }
}

/// Detect if data has trailing padding of `0x00` or `0xFF` bytes.
/// Returns the unique data size (rounded up to `block_size`) if trailing padding
/// is at least `block_size` bytes. Returns `None` if no significant padding.
pub fn detect_trailing_padding(data: &[u8], block_size: usize) -> Option<usize> {
    if data.len() <= block_size {
        return None;
    }

    let last = *data.last()?;
    if last != 0x00 && last != 0xFF {
        return None;
    }

    // Scan backwards to find where the padding starts.
    // `pos` is a valid index, so `pos + 1 <= data.len()` — saturating is safe.
    let pad_start = data
        .iter()
        .rposition(|&b| b != last)
        .map_or(0, |pos| pos.saturating_add(1));

    // Round up pad_start to the next block_size boundary, then compute
    // how much of data.len() is padding. All steps use checked arithmetic
    // so overflow or underflow yields None rather than a wrong answer.
    let unique_size = pad_start
        .checked_add(block_size.saturating_sub(1))?
        .checked_div(block_size)?
        .checked_mul(block_size)?;
    let pad_len = data.len().checked_sub(unique_size)?;

    if pad_len >= block_size {
        Some(unique_size)
    } else {
        None
    }
}

pub fn nintendo_region_display(region_code: u8) -> &'static str {
    match region_code {
        b'J' => "Japan",
        b'E' => "North America",
        b'P' => "Europe",
        b'D' => "Germany",
        b'F' => "France",
        b'I' => "Italy",
        b'S' => "Spain",
        b'H' => "Netherlands",
        b'K' => "Korea",
        b'A' => "Asia",
        b'O' => "Global",
        _ => "Unknown",
    }
}
