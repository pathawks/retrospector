pub mod atari;
pub mod cdi;
pub mod colecovision;
pub mod disc;
pub mod dreamcast;
pub mod fds;
pub mod gameboy;
pub mod gamecom;
pub mod gamecube;
pub mod gba;
pub mod genesis;
pub mod intellivision;
pub mod iso;
pub mod jaguar;
pub mod n64;
pub mod nds;
pub mod nes;
pub mod odyssey2;
pub mod playstation;
pub mod saturn;
pub mod segacd;
pub mod sms;
pub mod snes;
pub mod virtualboy;
pub mod wii;

mod helpers;
mod registry;

use crate::traits::rominfo::RomInfo;

pub type DetectorFn = registry::DetectorFn;

/// Try to identify the system from raw ROM/disc data.
///
/// Returns the first successful match using the same priority order as the
/// interactive inspector.  `IsoImage` is excluded because it is a generic
/// fallback that matches almost any ISO 9660 image and provides no
/// game-specific metadata.
pub fn detect_rom(data: &[u8]) -> Option<Box<dyn RomInfo>> {
    registry::detect_rom(data, false)
}

/// Try to identify the system from raw ROM/disc data including generic ISO fallback.
pub fn detect_rom_with_generic_fallback(data: &[u8]) -> Option<Box<dyn RomInfo>> {
    registry::detect_rom(data, true)
}

/// Return the detector for a system/alias name used by `--system`.
pub fn lookup_detector(name: &str) -> Option<DetectorFn> {
    registry::lookup_detector(name)
}

/// Return a list of `(primary_name, &[aliases])` for all registered systems.
pub fn system_names() -> Vec<(&'static str, &'static [&'static str])> {
    registry::system_names()
}
