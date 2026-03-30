// References:
//   SNES ROM header location detection:
//     https://snes.nesdev.org/wiki/ROM_header
//   Copier header (512-byte preamble) detection:
//     https://problemkaputt.de/fullsnes.htm#snescartridgeromheader

use byteorder::{ByteOrder, LittleEndian};

// SNES ROM size limits used by this parser. The upper bound reflects the
// largest standard mapped cartridge image size handled by this tool.
pub(super) const SNES_ROM_MIN_BYTES: usize = 0x8000; // 32 KiB
pub(super) const SNES_ROM_MAX_BYTES: usize = 0x800000; // 8 MiB

// Some dumps include a 512-byte copier/trainer preamble before the ROM data.
// The marker at bytes 7..11 is commonly either "\0\xAA\xBB\x04" or all zeros.
pub(super) const TRAINER_BYTES: usize = 512;
const TRAINER_BLOCK_BYTES: usize = 1024;
const TRAINER_MAGIC_START: usize = 7;
const TRAINER_MAGIC_END: usize = 11;
const TRAINER_PADDING_START: usize = 11;
const TRAINER_MAGIC_COPIER: &[u8; 4] = b"\0\xaa\xbb\x04";
const TRAINER_MAGIC_ZERO: &[u8; 4] = b"\0\0\0\0";

// Header field layout (offsets relative to detected header base).
pub(super) const HEADER_TITLE_LEN: usize = 21;
pub(super) const HEADER_MAP_MODE_OFFSET: usize = 0x15;
pub(super) const HEADER_CART_TYPE_OFFSET: usize = 0x16;
pub(super) const HEADER_ROM_SIZE_OFFSET: usize = 0x17;
pub(super) const HEADER_RAM_SIZE_OFFSET: usize = 0x18;
pub(super) const HEADER_REGION_OFFSET: usize = 0x19;
pub(super) const HEADER_REVISION_OFFSET: usize = 0x1B;
pub(super) const HEADER_COMPLEMENT_OFFSET: usize = 0x1C;
pub(super) const HEADER_CHECKSUM_OFFSET: usize = 0x1E;
const HEADER_NMI_VECTOR_OFFSET: usize = 0x3A;
pub(super) const HEADER_RESET_VECTOR_OFFSET: usize = 0x3C;
pub(super) const HEADER_VECTOR_BYTES: usize = 2;
pub(super) const HEADER_GAME_CODE_OFFSET: usize = 0x0E;
const HEADER_SCAN_WINDOW_BYTES: usize = 0x200;

// Map-mode and vector validity heuristics used to rank candidate header bases.
pub(super) const MAP_MODE_MASK: u8 = 0x0F;
pub(super) const MAP_MODE_FASTROM_BIT: u8 = 0x10;
pub(super) const VALID_MAPPER_MODES: [u8; 5] = [0x00, 0x01, 0x02, 0x03, 0x05];
pub(super) const RESET_VECTOR_ROM_MIN: u16 = 0x8000;

// SNES ROM size encoding in header byte: size_bytes = 0x400 << value.
pub(super) const HEADER_ROM_SIZE_UNIT: usize = 0x400;

// Candidate canonical header bases observed in LoROM/HiROM/Ex* mappings and
// edge dumps. We probe all of them, then score candidates by checksum, vectors,
// mapper plausibility, and title sanity.
// TODO(research): document concrete ROM examples for the non-canonical bases
// below (0x4081C0, 0x0000, 0x0B17C0) so future refactors preserve intent.
const HEADER_BASE_CANDIDATES: [usize; 7] = [
    0x7FC0,   // LoROM
    0x40FFC0, // ExHiROM
    0x81C0,   // ExLoROM
    0x4081C0, // mirrored ExLoROM variant seen in some dumps
    0x0000,   // malformed dumps with header at file start
    0x0B17C0, // non-standard base observed in certain prototypes
    0xFFC0,   // HiROM
];

pub(super) fn has_copier_trainer(buffer: &[u8]) -> bool {
    buffer.len() > TRAINER_BLOCK_BYTES
        && buffer.len() % TRAINER_BLOCK_BYTES == TRAINER_BYTES
        && (&buffer[TRAINER_MAGIC_START..TRAINER_MAGIC_END] == TRAINER_MAGIC_COPIER
            || &buffer[TRAINER_MAGIC_START..TRAINER_MAGIC_END] == TRAINER_MAGIC_ZERO)
        && buffer
            .iter()
            .take(TRAINER_BYTES)
            .skip(TRAINER_PADDING_START)
            .all(|&b| b == 0)
}

pub(super) fn decoded_rom_size(size_byte: u8) -> usize {
    HEADER_ROM_SIZE_UNIT.wrapping_shl(size_byte.into())
}

#[allow(clippy::arithmetic_side_effects)]
pub(super) fn detect_header_offset(buffer: &[u8]) -> Result<usize, &'static str> {
    // Early rejection: file too small for any SNES ROM
    if buffer.len() < SNES_ROM_MIN_BYTES {
        return Err("File too small for SNES ROM");
    }

    // Early rejection: file too large (> 8 MB) for standard SNES ROM
    // ExHiROM can be up to 8 MB, but not 600 MB CD images
    if buffer.len() > SNES_ROM_MAX_BYTES {
        return Err("File too large for SNES ROM");
    }

    let header_offset = if has_copier_trainer(buffer) {
        TRAINER_BYTES
    } else {
        0
    };

    // Pass 1: strongest match (mapper-derived base + valid checksum pair + sane title).
    for base in HEADER_BASE_CANDIDATES
        .iter()
        .filter(|&h| buffer.len() > h + HEADER_SCAN_WINDOW_BYTES)
        .copied()
    {
        let offset = base + header_offset;
        if offset + HEADER_CHECKSUM_OFFSET + HEADER_VECTOR_BYTES <= buffer.len() {
            // Read stored checksum and complement from the ROM header
            let complement_offset = offset + HEADER_COMPLEMENT_OFFSET;
            let checksum_offset = offset + HEADER_CHECKSUM_OFFSET;

            let stored_checksum =
                LittleEndian::read_u16(&buffer[checksum_offset..checksum_offset + 2]);

            let stored_complement =
                LittleEndian::read_u16(&buffer[complement_offset..complement_offset + 2]);

            let mapper_header_location =
                match buffer[offset + HEADER_MAP_MODE_OFFSET] & MAP_MODE_MASK {
                    0 => 0x7FC0, // LoRom
                    1 => 0xFFC0, // HiROM
                    3 => 0x7FC0,
                    5 => 0x40FFC0, // ExHiRom
                    _ => 0,
                };

            let rom_size = decoded_rom_size(buffer[offset + HEADER_ROM_SIZE_OFFSET]);

            let checksum_checksout = stored_checksum == !stored_complement
                && stored_checksum != 0
                && stored_complement != 0;
            let mapper_header_matches = base == mapper_header_location;
            let effective_size = buffer.len() - header_offset;
            let rom_sizes_match =
                rom_size.next_power_of_two() == effective_size.next_power_of_two();
            // Also accept ROMs padded to a larger power-of-2 (e.g. 1 MB ROM in a 2 MB file).
            // The file must itself be a valid SNES power-of-2 size and large enough to contain
            // the ROM declared in the header.
            let rom_fits_in_padded_file = rom_size > 0
                && rom_size <= effective_size
                && effective_size.count_ones() == 1
                && (SNES_ROM_MIN_BYTES..=SNES_ROM_MAX_BYTES).contains(&effective_size);
            // Also accept when the file is a valid SNES size and the reset vector points to
            // ROM space, even if the ROM size byte in the header is garbage (e.g. prototypes).
            let file_is_valid_snes_size = effective_size.count_ones() == 1
                && (SNES_ROM_MIN_BYTES..=SNES_ROM_MAX_BYTES).contains(&effective_size);
            let valid_reset_vector = offset + HEADER_RESET_VECTOR_OFFSET + HEADER_VECTOR_BYTES
                <= buffer.len()
                && LittleEndian::read_u16(
                    &buffer[offset + HEADER_RESET_VECTOR_OFFSET
                        ..offset + HEADER_RESET_VECTOR_OFFSET + HEADER_VECTOR_BYTES],
                ) >= RESET_VECTOR_ROM_MIN;
            let title_is_ascii = !buffer
                .iter()
                .skip(offset)
                .take(HEADER_TITLE_LEN)
                .map(|&c| c as char)
                .take_while(char::is_ascii_alphanumeric)
                .collect::<String>()
                .is_empty();

            if mapper_header_matches
                && checksum_checksout
                && (rom_sizes_match
                    || rom_fits_in_padded_file
                    || (file_is_valid_snes_size && valid_reset_vector))
                && title_is_ascii
            {
                return Ok(offset);
            }
        }
    }

    // Pass 2: keep checksum+size+title checks but drop mapper/base consistency.
    // TODO(research): explain historical false positives/negatives that required
    // separating this pass from pass 1 instead of using a weighted score.
    for base in HEADER_BASE_CANDIDATES
        .iter()
        .filter(|&h| buffer.len() > h + HEADER_SCAN_WINDOW_BYTES)
        .copied()
    {
        let offset = base + header_offset;
        if offset + HEADER_CHECKSUM_OFFSET + HEADER_VECTOR_BYTES <= buffer.len() {
            let complement_offset = offset + HEADER_COMPLEMENT_OFFSET;
            let checksum_offset = offset + HEADER_CHECKSUM_OFFSET;

            let stored_checksum =
                LittleEndian::read_u16(&buffer[checksum_offset..checksum_offset + 2]);

            let stored_complement =
                LittleEndian::read_u16(&buffer[complement_offset..complement_offset + 2]);

            let checksum_checksout = stored_checksum == !stored_complement
                && stored_checksum != 0
                && stored_complement != 0;

            let rom_size = decoded_rom_size(buffer[offset + HEADER_ROM_SIZE_OFFSET]);
            let rom_sizes_match =
                rom_size.next_power_of_two() == (buffer.len() - header_offset).next_power_of_two();
            let title_is_ascii = !buffer
                .iter()
                .skip(offset)
                .take(HEADER_TITLE_LEN)
                .map(|&c| c as char)
                .take_while(char::is_ascii_alphanumeric)
                .collect::<String>()
                .is_empty();

            if rom_sizes_match && title_is_ascii && checksum_checksout {
                return Ok(offset);
            }
        }
    }

    // Pass 3: No checksum match - stricter requirements
    for base in HEADER_BASE_CANDIDATES
        .iter()
        .filter(|&h| buffer.len() > h + HEADER_SCAN_WINDOW_BYTES)
        .copied()
    {
        let offset = base + header_offset;
        if offset + HEADER_RESET_VECTOR_OFFSET + HEADER_VECTOR_BYTES <= buffer.len() {
            let rom_size_byte = buffer[offset + HEADER_ROM_SIZE_OFFSET];
            let rom_size = decoded_rom_size(rom_size_byte);
            let rom_sizes_match =
                rom_size.next_power_of_two() == (buffer.len() - header_offset).next_power_of_two();

            // Require title to be at least 4 ASCII alphanumeric characters
            let title: String = buffer
                .iter()
                .skip(offset)
                .take(HEADER_TITLE_LEN)
                .map(|&c| c as char)
                .take_while(char::is_ascii_alphanumeric)
                .collect();
            let title_long_enough = title.len() >= 4;

            // Mapper byte must be valid (0x00-0x03 or 0x05, with optional FastROM bit 0x10)
            let mapper_byte = buffer[offset + HEADER_MAP_MODE_OFFSET] & MAP_MODE_MASK;
            let valid_mapper = VALID_MAPPER_MODES.contains(&mapper_byte);

            // ROM size byte must indicate at least 32 KB (value >= 0x05)
            let valid_rom_size_byte = (0x05..=0x0D).contains(&rom_size_byte);

            // Accept if file size is exactly a power of 2 in valid SNES range
            // This catches ROMs with zeroed header ROM size bytes
            let effective_size = buffer.len() - header_offset;
            let file_is_valid_snes_size = effective_size.count_ones() == 1
                && (SNES_ROM_MIN_BYTES..=SNES_ROM_MAX_BYTES).contains(&effective_size);

            // Reset vector must point to ROM space (>= 0x8000) when using zeroed header fallback
            let reset_vector = LittleEndian::read_u16(
                &buffer[offset + HEADER_RESET_VECTOR_OFFSET
                    ..offset + HEADER_RESET_VECTOR_OFFSET + HEADER_VECTOR_BYTES],
            );
            let valid_reset_vector = reset_vector >= RESET_VECTOR_ROM_MIN;

            // Accept if:
            // 1. Title and mapper are valid, AND
            // 2. Either:
            //    a. ROM size byte is valid AND ROM sizes match, OR
            //    b. ROM size byte is zero AND file is valid SNES size AND reset vector is valid
            if title_long_enough
                && valid_mapper
                && ((valid_rom_size_byte && rom_sizes_match)
                    || (rom_size_byte == 0 && file_is_valid_snes_size && valid_reset_vector))
            {
                return Ok(offset);
            }
        }
    }

    // Pass 4: ROMs with empty titles but valid interrupt vectors
    // Some ROMs have null/empty titles but are still valid SNES ROMs
    // Key insight: SNES interrupt vectors point to ROM space (>= 0x8000),
    // while other formats (like Game Gear) have unrelated data at these offsets
    for base in HEADER_BASE_CANDIDATES
        .iter()
        .filter(|&h| buffer.len() > h + HEADER_SCAN_WINDOW_BYTES)
        .copied()
    {
        let offset = base + header_offset;
        if offset + HEADER_RESET_VECTOR_OFFSET + HEADER_VECTOR_BYTES <= buffer.len() {
            let empty_title = buffer[offset] == 0;

            // Reset vector is at offset 0x3C-0x3D from header
            // Must point to valid ROM address space (0x8000-0xFFFF)
            let reset_vector = LittleEndian::read_u16(
                &buffer[offset + HEADER_RESET_VECTOR_OFFSET
                    ..offset + HEADER_RESET_VECTOR_OFFSET + HEADER_VECTOR_BYTES],
            );
            let valid_reset_vector = reset_vector >= RESET_VECTOR_ROM_MIN;

            // NMI vector at offset 0x3A-0x3B should also be in ROM space
            // (or be 0x0000 for ROMs that don't use NMI)
            let nmi_vector = LittleEndian::read_u16(
                &buffer[offset + HEADER_NMI_VECTOR_OFFSET
                    ..offset + HEADER_NMI_VECTOR_OFFSET + HEADER_VECTOR_BYTES],
            );
            let valid_nmi_vector = nmi_vector >= RESET_VECTOR_ROM_MIN || nmi_vector == 0;

            // Mapper byte must be valid
            let mapper_byte = buffer[offset + HEADER_MAP_MODE_OFFSET] & MAP_MODE_MASK;
            let valid_mapper = VALID_MAPPER_MODES.contains(&mapper_byte);

            // File must be valid SNES size
            let effective_size = buffer.len() - header_offset;
            let valid_file_size =
                (SNES_ROM_MIN_BYTES..=SNES_ROM_MAX_BYTES).contains(&effective_size);

            if empty_title
                && valid_reset_vector
                && valid_nmi_vector
                && valid_mapper
                && valid_file_size
            {
                return Ok(offset);
            }
        }
    }

    Err("Could not detect SNES header")
}

/// Calculate the SNES internal checksum (16-bit sum of all mapped bytes).
///
/// When the declared `rom_size` exceeds the file data length, the ROM is
/// non-power-of-2 and the SNES mirrors the upper portion of the data to fill
/// the address space. The standard algorithm: split at the largest power-of-2
/// boundary, then repeat the remainder until the declared size is filled.
#[allow(clippy::arithmetic_side_effects)]
pub(super) fn snes_checksum(data: &[u8], rom_size: usize) -> u16 {
    let file_size = data.len();

    if rom_size <= file_size {
        // Power-of-2 or smaller: just sum the declared region
        return data
            .iter()
            .take(rom_size)
            .fold(0u16, |sum, &b| sum.wrapping_add(b as u16));
    }

    // Non-power-of-2: mirror the remainder to fill declared size
    let base_size = file_size.next_power_of_two() >> 1; // largest pow2 <= file_size
    let remainder_size = file_size - base_size;

    if remainder_size == 0 {
        return data.iter().fold(0u16, |sum, &b| sum.wrapping_add(b as u16));
    }

    let base_sum: u32 = data[..base_size].iter().map(|&b| b as u32).sum();
    let remainder_sum: u32 = data[base_size..].iter().map(|&b| b as u32).sum();
    let mirror_count = (rom_size - base_size) / remainder_size;

    (base_sum.wrapping_add(remainder_sum.wrapping_mul(mirror_count as u32))) as u16
}

/// Calculate effective ROM size for display.
/// Uses header value if valid, otherwise falls back to file size.
pub(super) fn effective_rom_size(header_rom_size: usize, file_rom_size: usize) -> usize {
    // Header ROM size is valid if:
    // - At least 32 KB (minimum valid SNES ROM)
    // - At most 8 MB (maximum for extended mappings)
    // - Matches file size (within power of 2)
    if (SNES_ROM_MIN_BYTES..=SNES_ROM_MAX_BYTES).contains(&header_rom_size)
        && header_rom_size.next_power_of_two() == file_rom_size.next_power_of_two()
    {
        header_rom_size
    } else {
        file_rom_size
    }
}
