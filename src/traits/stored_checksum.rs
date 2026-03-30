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

impl std::fmt::Display for dyn StoredChecksum<u8> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.stored_checksum() == self.calculated_checksum() {
            write!(
                f,
                "Stored Checksum: {:02X} is valid",
                self.stored_checksum()
            )
        } else {
            writeln!(f, "Stored Checksum:     {:02X}", self.stored_checksum())?;
            write!(f, "Calculated Checksum: {:02X}", self.calculated_checksum())
        }
    }
}

impl std::fmt::Display for dyn StoredChecksum<u16> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.stored_checksum() == self.calculated_checksum() {
            write!(
                f,
                "Stored Checksum: {:04X} is valid",
                self.stored_checksum()
            )
        } else {
            writeln!(f, "Stored Checksum:     {:04X}", self.stored_checksum())?;
            write!(f, "Calculated Checksum: {:04X}", self.calculated_checksum())
        }
    }
}

impl std::fmt::Display for dyn StoredChecksum<u32> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.stored_checksum() == self.calculated_checksum() {
            writeln!(
                f,
                "Stored Checksum: {:08X} is valid",
                self.stored_checksum()
            )
        } else {
            writeln!(f, "Stored Checksum: {:08X}", self.stored_checksum())?;
            write!(f, "Calculated Checksum: {:08X}", self.calculated_checksum())
        }
    }
}

impl std::fmt::Display for dyn StoredChecksum<(u32, u32)> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (crc1, crc2) = self.stored_checksum();
        if self.stored_checksum() == self.calculated_checksum() {
            write!(f, "Stored Checksum: ({:04X},{:04X}) is valid", crc1, crc2)
        } else {
            writeln!(f, "Stored Checksum:     ({:04X},{:04X})", crc1, crc2)?;
            let (calc1, calc2) = self.calculated_checksum();
            write!(f, "Calculated Checksum: ({:04X},{:04X})", calc1, calc2)
        }
    }
}
