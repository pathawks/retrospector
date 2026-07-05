use std::cmp;

pub trait StoredChecksum<T>
where
    T: cmp::Eq,
{
    fn stored_checksum(&self) -> T;
    fn calculated_checksum(&self) -> T;

    fn checksum_matches(&self) -> bool {
        self.stored_checksum() == self.calculated_checksum()
    }
}

fn fmt_hex_checksum<T>(
    f: &mut std::fmt::Formatter<'_>,
    stored: T,
    calculated: T,
    width: usize,
) -> std::fmt::Result
where
    T: Eq + std::fmt::UpperHex,
{
    if stored == calculated {
        write!(f, "Stored Checksum: {:0width$X} is valid", stored)
    } else {
        writeln!(f, "Stored Checksum:     {:0width$X}", stored)?;
        write!(f, "Calculated Checksum: {:0width$X}", calculated)
    }
}

impl std::fmt::Display for dyn StoredChecksum<u8> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fmt_hex_checksum(f, self.stored_checksum(), self.calculated_checksum(), 2)
    }
}

impl std::fmt::Display for dyn StoredChecksum<u16> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fmt_hex_checksum(f, self.stored_checksum(), self.calculated_checksum(), 4)
    }
}

impl std::fmt::Display for dyn StoredChecksum<u32> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fmt_hex_checksum(f, self.stored_checksum(), self.calculated_checksum(), 8)
    }
}

impl std::fmt::Display for dyn StoredChecksum<(u32, u32)> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (crc1, crc2) = self.stored_checksum();
        let (calc1, calc2) = self.calculated_checksum();
        if (crc1, crc2) == (calc1, calc2) {
            write!(f, "Stored Checksum: ({:08X},{:08X}) is valid", crc1, crc2)
        } else {
            writeln!(f, "Stored Checksum:     ({:08X},{:08X})", crc1, crc2)?;
            write!(f, "Calculated Checksum: ({:08X},{:08X})", calc1, calc2)
        }
    }
}
