/// Trait for ROM types that can calculate a hash of their data.
///
/// Unlike `StoredChecksum` which compares a stored value against a calculated one,
/// this trait is for calculating hashes that can be used for identification
/// (e.g., database lookups).
pub trait RomHash {
    /// Returns the SHA1 hash of the ROM data (excluding any file headers).
    fn sha1(&self) -> [u8; 20];
}

impl std::fmt::Display for dyn RomHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SHA1: ")?;
        for byte in self.sha1() {
            write!(f, "{:02X}", byte)?;
        }
        Ok(())
    }
}
