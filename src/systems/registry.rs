use crate::traits::{error::ParseError, rominfo::RomInfo};

use super::{
    atari::{Atari2600Info, Atari5200Info, Atari7800Info, AtariLynxInfo},
    cdi::CdiDisc,
    colecovision::ColecoVisionInfo,
    dreamcast::DreamcastDisc,
    fds::FdsRomInfo,
    gameboy::GameboyInfo,
    gamecom::GamecomRomInfo,
    gamecube::GameCubeDisc,
    gba::GbaRomInfo,
    genesis::SegaRomInfo,
    intellivision::IntellivisionInfo,
    iso::IsoImage,
    jaguar::AtariJaguarInfo,
    n64::N64RomInfo,
    nds::NdsRomInfo,
    nes::NesRomInfo,
    odyssey2::MagnavoxOdyssey2Info,
    playstation::PlaystationDisc,
    saturn::SaturnDisc,
    segacd::SegaCdDisc,
    sms::SmsRomInfo,
    snes::SNESRomInfo,
    virtualboy::VirtualBoyRomInfo,
    wii::WiiDisc,
};

pub type DetectorFn = fn(&[u8]) -> Result<Box<dyn RomInfo>, ParseError>;

// Largest plausible ROM size for a system, used to skip auto-detection for
// files far too big to belong to it. This avoids scanning huge disc images for
// small-cartridge magic bytes — and the false positives that risk — while
// leaving explicit `--system` selection unaffected.
const GAMECOM_MAX_SIZE: usize = 0x40_0000; // Tiger Game.com carts top out ~4 MB
const COLECOVISION_MAX_SIZE: usize = 0x10_0000; // ColecoVision megacarts top out ~1 MB

#[derive(Clone, Copy)]
struct DetectorRegistration {
    aliases: &'static [&'static str],
    detector: DetectorFn,
    generic_fallback: bool,
    max_size: Option<usize>,
}

impl DetectorRegistration {
    const fn new(aliases: &'static [&'static str], detector: DetectorFn) -> Self {
        Self {
            aliases,
            detector,
            generic_fallback: false,
            max_size: None,
        }
    }

    const fn generic(mut self) -> Self {
        self.generic_fallback = true;
        self
    }

    const fn with_max_size(mut self, max_size: usize) -> Self {
        self.max_size = Some(max_size);
        self
    }

    fn accepts_len(&self, len: usize) -> bool {
        match self.max_size {
            Some(max) => len <= max,
            None => true,
        }
    }
}

fn detect<T>(buffer: &[u8]) -> Result<Box<dyn RomInfo>, ParseError>
where
    for<'a> T: RomInfo + TryFrom<&'a [u8], Error = ParseError> + 'static,
{
    T::try_from(buffer).map(|info| Box::new(info) as Box<dyn RomInfo>)
}

const DETECTOR_REGISTRY: &[DetectorRegistration] = &[
    // Disc formats with O(1) magic checks (fast path for large disc images)
    DetectorRegistration::new(&["wii"], detect::<WiiDisc>),
    DetectorRegistration::new(&["gamecube", "gc"], detect::<GameCubeDisc>),
    DetectorRegistration::new(&["saturn"], detect::<SaturnDisc>),
    DetectorRegistration::new(&["dreamcast", "dc"], detect::<DreamcastDisc>),
    // Cartridge formats
    DetectorRegistration::new(&["n64"], detect::<N64RomInfo>),
    DetectorRegistration::new(&["gamecom"], detect::<GamecomRomInfo>)
        .with_max_size(GAMECOM_MAX_SIZE),
    DetectorRegistration::new(&["gba"], detect::<GbaRomInfo>),
    DetectorRegistration::new(&["gameboy", "gb", "gbc"], detect::<GameboyInfo>),
    DetectorRegistration::new(&["genesis", "megadrive"], detect::<SegaRomInfo>),
    DetectorRegistration::new(&["sms"], detect::<SmsRomInfo>),
    DetectorRegistration::new(&["nds", "ds"], detect::<NdsRomInfo>),
    DetectorRegistration::new(&["fds"], detect::<FdsRomInfo>),
    DetectorRegistration::new(&["nes"], detect::<NesRomInfo>),
    DetectorRegistration::new(&["virtualboy", "vb"], detect::<VirtualBoyRomInfo>),
    DetectorRegistration::new(&["snes"], detect::<SNESRomInfo>),
    DetectorRegistration::new(&["jaguar"], detect::<AtariJaguarInfo>),
    DetectorRegistration::new(&["intellivision", "intv"], detect::<IntellivisionInfo>),
    DetectorRegistration::new(&["colecovision", "cv"], detect::<ColecoVisionInfo>)
        .with_max_size(COLECOVISION_MAX_SIZE),
    DetectorRegistration::new(&["odyssey2", "o2"], detect::<MagnavoxOdyssey2Info>),
    DetectorRegistration::new(&["atari7800", "7800"], detect::<Atari7800Info>),
    DetectorRegistration::new(&["lynx"], detect::<AtariLynxInfo>),
    DetectorRegistration::new(&["atari5200", "5200"], detect::<Atari5200Info>),
    DetectorRegistration::new(&["atari2600", "2600"], detect::<Atari2600Info>),
    // Slower disc formats (sliding-window / PVD search)
    DetectorRegistration::new(&["segacd", "scd"], detect::<SegaCdDisc>),
    DetectorRegistration::new(&["playstation", "psx"], detect::<PlaystationDisc>),
    DetectorRegistration::new(&["cdi"], detect::<CdiDisc>),
    DetectorRegistration::new(&["iso"], detect::<IsoImage>).generic(),
];

pub fn lookup_detector(name: &str) -> Option<DetectorFn> {
    DETECTOR_REGISTRY
        .iter()
        .find(|registration| {
            registration
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(name))
        })
        .map(|registration| registration.detector)
}

/// Return a list of `(primary_name, &[aliases])` for all registered systems.
///
/// The primary name is the first alias in each registration entry.
/// Used to build `--help` text for the `--system` flag.
pub fn system_names() -> Vec<(&'static str, &'static [&'static str])> {
    DETECTOR_REGISTRY
        .iter()
        .filter(|r| !r.generic_fallback)
        .map(|r| (r.aliases[0], r.aliases))
        .collect()
}

pub fn detect_rom(data: &[u8], include_generic_fallback: bool) -> Option<Box<dyn RomInfo>> {
    for registration in DETECTOR_REGISTRY {
        if !include_generic_fallback && registration.generic_fallback {
            continue;
        }
        if !registration.accepts_len(data.len()) {
            continue;
        }
        if let Ok(info) = (registration.detector)(data) {
            return Some(info);
        }
    }
    None
}
