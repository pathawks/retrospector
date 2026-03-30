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

#[derive(Clone, Copy)]
struct DetectorRegistration {
    aliases: &'static [&'static str],
    detector: DetectorFn,
    generic_fallback: bool,
}

fn detect<T>(buffer: &[u8]) -> Result<Box<dyn RomInfo>, ParseError>
where
    for<'a> T: RomInfo + TryFrom<&'a [u8], Error = ParseError> + 'static,
{
    T::try_from(buffer).map(|info| Box::new(info) as Box<dyn RomInfo>)
}

const DETECTOR_REGISTRY: &[DetectorRegistration] = &[
    // Disc formats with O(1) magic checks (fast path for large disc images)
    DetectorRegistration {
        aliases: &["wii"],
        detector: detect::<WiiDisc>,
        generic_fallback: false,
    },
    DetectorRegistration {
        aliases: &["gamecube", "gc"],
        detector: detect::<GameCubeDisc>,
        generic_fallback: false,
    },
    DetectorRegistration {
        aliases: &["saturn"],
        detector: detect::<SaturnDisc>,
        generic_fallback: false,
    },
    DetectorRegistration {
        aliases: &["dreamcast", "dc"],
        detector: detect::<DreamcastDisc>,
        generic_fallback: false,
    },
    // Cartridge formats
    DetectorRegistration {
        aliases: &["n64"],
        detector: detect::<N64RomInfo>,
        generic_fallback: false,
    },
    DetectorRegistration {
        aliases: &["gamecom"],
        detector: detect::<GamecomRomInfo>,
        generic_fallback: false,
    },
    DetectorRegistration {
        aliases: &["gba"],
        detector: detect::<GbaRomInfo>,
        generic_fallback: false,
    },
    DetectorRegistration {
        aliases: &["gameboy", "gb", "gbc"],
        detector: detect::<GameboyInfo>,
        generic_fallback: false,
    },
    DetectorRegistration {
        aliases: &["genesis", "megadrive"],
        detector: detect::<SegaRomInfo>,
        generic_fallback: false,
    },
    DetectorRegistration {
        aliases: &["sms"],
        detector: detect::<SmsRomInfo>,
        generic_fallback: false,
    },
    DetectorRegistration {
        aliases: &["nds", "ds"],
        detector: detect::<NdsRomInfo>,
        generic_fallback: false,
    },
    DetectorRegistration {
        aliases: &["fds"],
        detector: detect::<FdsRomInfo>,
        generic_fallback: false,
    },
    DetectorRegistration {
        aliases: &["nes"],
        detector: detect::<NesRomInfo>,
        generic_fallback: false,
    },
    DetectorRegistration {
        aliases: &["virtualboy", "vb"],
        detector: detect::<VirtualBoyRomInfo>,
        generic_fallback: false,
    },
    DetectorRegistration {
        aliases: &["snes"],
        detector: detect::<SNESRomInfo>,
        generic_fallback: false,
    },
    DetectorRegistration {
        aliases: &["jaguar"],
        detector: detect::<AtariJaguarInfo>,
        generic_fallback: false,
    },
    DetectorRegistration {
        aliases: &["intellivision", "intv"],
        detector: detect::<IntellivisionInfo>,
        generic_fallback: false,
    },
    DetectorRegistration {
        aliases: &["colecovision", "cv"],
        detector: detect::<ColecoVisionInfo>,
        generic_fallback: false,
    },
    DetectorRegistration {
        aliases: &["odyssey2", "o2"],
        detector: detect::<MagnavoxOdyssey2Info>,
        generic_fallback: false,
    },
    DetectorRegistration {
        aliases: &["atari7800", "7800"],
        detector: detect::<Atari7800Info>,
        generic_fallback: false,
    },
    DetectorRegistration {
        aliases: &["lynx"],
        detector: detect::<AtariLynxInfo>,
        generic_fallback: false,
    },
    DetectorRegistration {
        aliases: &["atari5200", "5200"],
        detector: detect::<Atari5200Info>,
        generic_fallback: false,
    },
    DetectorRegistration {
        aliases: &["atari2600", "2600"],
        detector: detect::<Atari2600Info>,
        generic_fallback: false,
    },
    // Slower disc formats (sliding-window / PVD search)
    DetectorRegistration {
        aliases: &["segacd", "scd"],
        detector: detect::<SegaCdDisc>,
        generic_fallback: false,
    },
    DetectorRegistration {
        aliases: &["playstation", "psx"],
        detector: detect::<PlaystationDisc>,
        generic_fallback: false,
    },
    DetectorRegistration {
        aliases: &["cdi"],
        detector: detect::<CdiDisc>,
        generic_fallback: false,
    },
    DetectorRegistration {
        aliases: &["iso"],
        detector: detect::<IsoImage>,
        generic_fallback: true,
    },
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
        if let Ok(info) = (registration.detector)(data) {
            return Some(info);
        }
    }
    None
}
