//! No-Intro/Logiqx XML DAT output module for retrospector

use crate::output::cue::parse_cue_and_hash;
use crate::output::hash::{Hashes, compute_hashes, hex_upper_spaced};
use crate::systems::detect_rom;
use crate::traits::rominfo::DatMeta;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

pub struct RomEntry {
    pub name: String,
    pub size: usize,
    pub hashes: Hashes,
    pub header: Option<[u8; 16]>,
    pub serial: Option<String>,
}

/// A `<release>` element: maps a game name to a specific regional release.
pub struct ReleaseEntry {
    pub region: String,
    pub date: Option<String>,
}

pub struct GameEntry {
    pub name: String,
    pub description: String,
    /// Year string for the `<year>` element (e.g. "1995").
    pub year: Option<String>,
    /// Publisher for the `<manufacturer>` element.
    pub manufacturer: Option<String>,
    /// Zero or more `<release>` elements.
    pub releases: Vec<ReleaseEntry>,
    pub roms: Vec<RomEntry>,
    /// Console/system name for the XML comment.
    pub system: Option<String>,
}

struct GameMetadata {
    name: String,
    description: String,
    year: Option<String>,
    manufacturer: Option<String>,
    releases: Vec<ReleaseEntry>,
    system: Option<String>,
    serial: Option<String>,
}

/// Build the canonical game name for `<game name="">`.
///
/// Format: `{title} ({region}) ({version})`
/// Any absent piece is simply omitted.  Falls back to `stem` when the
/// ROM header contains no usable title.
fn build_game_name(
    stem: &str,
    title: Option<&str>,
    region: Option<&str>,
    version: Option<&str>,
) -> String {
    let base = title.filter(|t| !t.is_empty()).unwrap_or(stem);
    let mut name = base.to_string();
    if let Some(r) = region {
        name.push_str(&format!(" ({})", r));
    }
    if let Some(v) = version {
        name.push_str(&format!(" ({})", v));
    }
    name
}

fn detect_game_metadata(stem: &str, data: &[u8]) -> GameMetadata {
    let (meta, system) = detect_rom(data)
        .map(|info| {
            let system = info.console().to_string();
            let meta = info.dat_meta();
            (meta, Some(system))
        })
        .unwrap_or_default();

    game_metadata_from_dat_meta(stem, meta, system)
}

fn game_metadata_from_dat_meta(stem: &str, meta: DatMeta, system: Option<String>) -> GameMetadata {
    let description = build_game_name(
        stem,
        meta.title.as_deref(),
        meta.region.as_deref(),
        meta.version.as_deref(),
    );
    let name = meta
        .machine_id
        .clone()
        .unwrap_or_else(|| description.clone());
    let releases = meta
        .region
        .iter()
        .map(|r| ReleaseEntry {
            region: r.clone(),
            date: meta.date.clone(),
        })
        .collect();
    let year = meta
        .date
        .as_deref()
        .and_then(|d| d.get(..4))
        .map(String::from);

    GameMetadata {
        name,
        description,
        year,
        manufacturer: meta.manufacturer,
        releases,
        system,
        serial: meta.serial,
    }
}

fn game_entry(metadata: GameMetadata, roms: Vec<RomEntry>) -> GameEntry {
    GameEntry {
        name: metadata.name,
        description: metadata.description,
        year: metadata.year,
        manufacturer: metadata.manufacturer,
        releases: metadata.releases,
        roms,
        system: metadata.system,
    }
}

fn nes_header(data: &[u8]) -> Option<[u8; 16]> {
    if data.len() >= 16 && data[..4] == [0x4E, 0x45, 0x53, 0x1A] {
        let mut header = [0u8; 16];
        header.copy_from_slice(&data[..16]);
        Some(header)
    } else {
        None
    }
}

fn rom_entry(name: String, data: &[u8], serial: Option<String>) -> RomEntry {
    RomEntry {
        name,
        size: data.len(),
        hashes: compute_hashes(data),
        header: nes_header(data),
        serial,
    }
}

pub fn collect_games(path: &Path) -> io::Result<Vec<GameEntry>> {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Unknown");

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase());

    if ext.as_deref() == Some("cue") {
        let (data, track_hashes, _cue) = parse_cue_and_hash(path)?;
        let metadata = detect_game_metadata(stem, &data);
        let serial = metadata.serial.clone();

        let roms = track_hashes
            .iter()
            .enumerate()
            .map(|(i, track)| {
                let track_num: u32 = track.number.parse().unwrap_or(0);
                RomEntry {
                    name: format!("Track {:02}.bin", track_num),
                    size: track.size,
                    hashes: track.hashes,
                    header: None,
                    serial: if i == 0 { serial.clone() } else { None },
                }
            })
            .collect();

        Ok(vec![game_entry(metadata, roms)])
    } else {
        let mut data = Vec::new();
        File::open(path)?.read_to_end(&mut data)?;

        let file_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown")
            .to_string();

        let metadata = detect_game_metadata(stem, &data);
        let rom = rom_entry(file_name, &data, metadata.serial.clone());

        Ok(vec![game_entry(metadata, vec![rom])])
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub fn serialize_dat(games: &[GameEntry]) -> String {
    let mut out = String::new();

    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<datafile xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xsi:schemaLocation=\"https://www.logiqx.com/Dats/datafile.xsd\">\n");
    out.push_str("\t<header>\n");
    out.push_str("\t\t<name>retrospector</name>\n");
    out.push_str(concat!(
        "\t\t<description>",
        "Generated by ",
        env!("CARGO_PKG_NAME"),
        "</description>\n"
    ));
    out.push_str(concat!(
        "\t\t<version>",
        env!("CARGO_PKG_VERSION"),
        "</version>\n"
    ));
    out.push_str("\t</header>\n");

    for game in games {
        out.push_str(&format!("\t<game name=\"{}\">\n", xml_escape(&game.name)));
        if let Some(ref sys) = game.system {
            out.push_str(&format!("\t\t<!-- System: {} -->\n", xml_escape(sys)));
        }
        out.push_str("\t\t<category>Games</category>\n");
        out.push_str(&format!(
            "\t\t<description>{}</description>\n",
            xml_escape(&game.description)
        ));

        if let Some(ref year) = game.year {
            out.push_str(&format!("\t\t<year>{}</year>\n", xml_escape(year)));
        }
        if let Some(ref mfr) = game.manufacturer {
            out.push_str(&format!(
                "\t\t<manufacturer>{}</manufacturer>\n",
                xml_escape(mfr)
            ));
        }

        for release in &game.releases {
            let date_attr = release
                .date
                .as_deref()
                .map(|d| format!(" date=\"{}\"", xml_escape(d)))
                .unwrap_or_default();
            out.push_str(&format!(
                "\t\t<release name=\"{}\" region=\"{}\"{}/>\n",
                xml_escape(&game.name),
                xml_escape(&release.region),
                date_attr,
            ));
        }

        for rom in &game.roms {
            out.push_str(&format!(
                "\t\t<rom name=\"{}\" size=\"{}\" crc=\"{}\" md5=\"{}\" sha1=\"{}\" sha256=\"{}\"{}{}/>\n",
                xml_escape(&rom.name),
                rom.size,
                rom.hashes.crc32_hex_lower(),
                rom.hashes.md5_hex_lower(),
                rom.hashes.sha1_hex_lower(),
                rom.hashes.sha256_hex_lower(),
                header_attr(rom),
                serial_attr(rom),
            ));
        }

        out.push_str("\t</game>\n");
    }

    out.push_str("</datafile>\n");
    out
}

pub fn serialize_mamedat(games: &[GameEntry]) -> String {
    let mut out = String::new();

    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<!DOCTYPE datafile PUBLIC \"-//Logiqx//DTD ROM Management Datafile//EN\" \"http://www.logiqx.com/Dats/datafile.dtd\">\n\n");
    out.push_str("<datafile>\n");
    out.push_str("\t<header>\n");
    out.push_str("\t\t<name>retrospector</name>\n");
    out.push_str(concat!(
        "\t\t<description>",
        "Generated by ",
        env!("CARGO_PKG_NAME"),
        "</description>\n"
    ));
    out.push_str(concat!(
        "\t\t<version>",
        env!("CARGO_PKG_VERSION"),
        "</version>\n"
    ));
    out.push_str("\t\t<category></category>\n");
    out.push_str("\t\t<date></date>\n");
    out.push_str("\t\t<author></author>\n");
    out.push_str("\t\t<email></email>\n");
    out.push_str("\t\t<homepage></homepage>\n");
    out.push_str("\t\t<url></url>\n");
    out.push_str("\t\t<comment></comment>\n");
    out.push_str("\t\t<clrmamepro/>\n");
    out.push_str("\t</header>\n");

    for game in games {
        out.push_str(&format!(
            "\t<machine name=\"{}\">\n",
            xml_escape(&game.name)
        ));
        if let Some(ref sys) = game.system {
            out.push_str(&format!("\t\t<!-- System: {} -->\n", xml_escape(sys)));
        }
        out.push_str(&format!(
            "\t\t<description>{}</description>\n",
            xml_escape(&game.description)
        ));

        for rom in &game.roms {
            out.push_str(&format!(
                "\t\t<rom name=\"{}\" size=\"{}\" crc=\"{}\" md5=\"{}\" sha1=\"{}\"{}/>\n",
                xml_escape(&rom.name),
                rom.size,
                rom.hashes.crc32_hex_lower(),
                rom.hashes.md5_hex_lower(),
                rom.hashes.sha1_hex_lower(),
                header_attr(rom),
            ));
        }

        out.push_str("\t</machine>\n");
    }

    out.push_str("</datafile>\n");
    out
}

fn header_attr(rom: &RomEntry) -> String {
    rom.header
        .map(|header| format!(" header=\"{}\"", hex_upper_spaced(&header)))
        .unwrap_or_default()
}

fn serial_attr(rom: &RomEntry) -> String {
    rom.serial
        .as_deref()
        .map(|serial| format!(" serial=\"{}\"", xml_escape(serial)))
        .unwrap_or_default()
}
