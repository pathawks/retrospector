// References:
//   Sega third-party publisher T-codes:
//     https://segaretro.org/Third-party_T-series_codes

/// Look up a Sega third-party publisher by its numeric T-code.
///
/// The copyright field in the ROM header uses the format `(C)T-XX YYYY.MMM`
/// where XX is the publisher code assigned by Sega.
pub fn lookup_publisher(code: u16) -> Option<&'static str> {
    match code {
        10 => Some("Takara"),
        11 => Some("Taito/Accolade"),
        12 => Some("Capcom"),
        13 => Some("Data East"),
        14 => Some("Namco/Tengen"),
        15 => Some("Sunsoft"),
        16 => Some("Bandai"),
        17 => Some("Dempa"),
        18 => Some("Technosoft"),
        19 => Some("Technosoft"),
        20 => Some("Asmik"),
        22 => Some("Micronet"),
        23 => Some("Vic Tokai"),
        24 => Some("American Sammy"),
        29 => Some("Kyugo"),
        32 => Some("Wolfteam"),
        33 => Some("Kaneko"),
        35 => Some("Toaplan"),
        36 => Some("Tecmo"),
        40 => Some("Toaplan"),
        42 => Some("UFL Company Limited"),
        43 => Some("Human"),
        45 => Some("Game Arts"),
        47 => Some("Sage's Creation"),
        48 => Some("Tengen"),
        49 => Some("Renovation/Telenet"),
        50 => Some("Electronic Arts"),
        56 => Some("Razorsoft"),
        58 => Some("Mentrix"),
        60 => Some("Victor Musical Industries"),
        69 => Some("Arena"),
        70 => Some("Virgin"),
        73 => Some("Soft Vision"),
        74 => Some("Palsoft"),
        76 => Some("Koei"),
        79 => Some("U.S. Gold"),
        81 => Some("Acclaim/Flying Edge"),
        83 => Some("Gametek"),
        86 => Some("Absolute"),
        93 => Some("Sony"),
        95 => Some("Konami"),
        97 => Some("Tradewest"),
        100 => Some("T*HQ"),
        101 => Some("Tecmagik"),
        112 => Some("Designer Software"),
        113 => Some("Psygnosis"),
        119 => Some("Accolade"),
        120 => Some("Codemasters"),
        125 => Some("Interplay"),
        130 => Some("Activision"),
        132 => Some("Shiny/Playmates"),
        144 => Some("Atlus"),
        151 => Some("Infogrames"),
        161 => Some("Fox Interactive"),
        239 => Some("Disney Interactive"),
        _ => None,
    }
}

/// Parse the copyright string from a Sega ROM header and look up the publisher.
///
/// Accepts formats like `(C)T-12 1993.SEP` or `(C)SEGA 1991.JUN`.
/// Returns the publisher name for numeric T-codes, the literal prefix for
/// alphabetic codes like `SEGA`, or `None` if parsing fails.
pub fn publisher_from_copyright(copyright: &str) -> Option<&'static str> {
    // Strip the "(C)" prefix if present.
    let rest = copyright
        .strip_prefix("(C)")
        .or_else(|| copyright.strip_prefix("(c)"))
        .unwrap_or(copyright)
        .trim_start();

    // Check for well-known alphabetic publisher codes.
    let alpha_prefix = rest.split_whitespace().next()?;
    match alpha_prefix {
        "SEGA" => return Some("Sega"),
        "ACLD" => return Some("Ballistic"),
        "ASCI" => return Some("Asciiware"),
        "RSI" => return Some("Razorsoft"),
        "TREC" => return Some("Treco"),
        "VRGN" => return Some("Virgin Games"),
        "WSTN" => return Some("Westone"),
        _ => {}
    }

    // Try to parse a numeric T-code: "T-XX" or "T-XXX".
    let t_str = rest.strip_prefix("T-")?;
    let num_str = t_str.split(|c: char| !c.is_ascii_digit()).next()?;
    let code: u16 = num_str.parse().ok()?;
    lookup_publisher(code)
}
