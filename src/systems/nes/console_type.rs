// References:
//   NES 2.0 console type field:
//     https://www.nesdev.org/wiki/NES_2.0#System_Type

use std::default::Default;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum ConsoleType {
    #[default]
    NES = 0,
    VsSystem = 1,
    Playchoice = 2,
    ExtendedConsoleType = 3,
}

impl From<u8> for ConsoleType {
    fn from(v: u8) -> Self {
        match v & 0b0000_0011 {
            0 => ConsoleType::NES,
            1 => ConsoleType::VsSystem,
            2 => ConsoleType::Playchoice,
            3 => ConsoleType::ExtendedConsoleType,
            _ => unreachable!(),
        }
    }
}

impl fmt::Display for ConsoleType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                ConsoleType::NES => "NES",
                ConsoleType::VsSystem => "Vs. System",
                ConsoleType::Playchoice => "PlayChoice 10",
                ConsoleType::ExtendedConsoleType => "Extended Console Type",
            }
        )
    }
}
