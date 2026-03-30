// References:
//   NES nametable mirroring:
//     https://www.nesdev.org/wiki/Mirroring#Nametable_Mirroring

use std::default::Default;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum MirrorType {
    #[default]
    HorizontalOrMapperControlled,
    Vertical,
}

impl From<u8> for MirrorType {
    fn from(flags: u8) -> Self {
        match flags & 0b0000_0001 {
            0 => MirrorType::HorizontalOrMapperControlled,
            1 => MirrorType::Vertical,
            _ => unreachable!(),
        }
    }
}

impl fmt::Display for MirrorType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                MirrorType::HorizontalOrMapperControlled => "horizontal or mapper controlled",
                MirrorType::Vertical => "vertical",
            }
        )
    }
}
