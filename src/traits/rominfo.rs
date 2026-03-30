/// Metadata extracted for No-Intro/Logiqx DAT output.
/// Fields map to `<game>` and `<release>` elements.  All are optional;
/// systems that don't expose a given piece of information leave it `None`.
#[derive(Default)]
pub struct DatMeta {
    /// Raw title from the ROM/disc header (used as `<game name="">`).
    pub title: Option<String>,
    /// Canonical No-Intro region string, e.g. "USA", "Japan", "Europe", "World".
    pub region: Option<String>,
    /// Version qualifier, e.g. "Rev 1" or "v1.001".
    pub version: Option<String>,
    /// Release date in YYYY-MM-DD format (used in `<release date="">`).
    pub date: Option<String>,
    /// Publisher / manufacturer string (used in `<manufacturer>`).
    pub manufacturer: Option<String>,
    /// Product / game code (used in `<rom serial="">`).
    pub serial: Option<String>,
    /// Short machine identifier (4-byte game code) used as `<machine name="">`.
    pub machine_id: Option<String>,
}

pub trait RomInfo: std::fmt::Display {
    fn console(&self) -> &'static str;

    /// Return DAT metadata for this ROM.  The default implementation
    /// returns an all-`None` struct; systems that implement `Title` (or
    /// have region/version data) override this method.
    fn dat_meta(&self) -> DatMeta {
        DatMeta::default()
    }
}
