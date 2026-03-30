// References:
//   NES 2.0 CPU/PPU timing field:
//     https://www.nesdev.org/wiki/NES_2.0#CPU/PPU_Timing

use std::default::Default;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum Timing {
    #[default]
    NTSC = 0,
    PAL = 1,
    MultiRegion = 2,
    Dendy = 3,
}

impl From<u8> for Timing {
    fn from(v: u8) -> Self {
        match v & 0b0000_0011 {
            0 => Timing::NTSC,
            1 => Timing::PAL,
            2 => Timing::MultiRegion,
            3 => Timing::Dendy,
            _ => unreachable!(),
        }
    }
}

impl fmt::Display for Timing {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Timing::NTSC => "NTSC NES",
                Timing::PAL => "Licensed PAL NES",
                Timing::MultiRegion => "Multiple-region",
                Timing::Dendy => "Dendy",
            }
        )
    }
}
