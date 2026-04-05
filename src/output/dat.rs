//! No-Intro/Logiqx XML DAT output module for retrospector

use crate::output::cue::parse_cue_and_hash;
use crate::systems::detect_rom;
use crc::{CRC_32_ISO_HDLC, Crc};
use md5::Md5;
use sha1::{Digest, Sha1};
use sha2::Sha256;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

pub struct RomEntry {
    pub name: String,
    pub size: usize,
    pub crc32: u32,
    pub md5: [u8; 16],
    pub sha1: [u8; 20],
    pub sha256: [u8; 32],
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

fn compute_hashes(data: &[u8]) -> (u32, [u8; 16], [u8; 20], [u8; 32]) {
    let crc_algo = Crc::<u32>::new(&CRC_32_ISO_HDLC);
    let crc32 = crc_algo.checksum(data);

    let md5: [u8; 16] = {
        let mut h = Md5::new();
        h.update(data);
        h.finalize().into()
    };

    let sha1: [u8; 20] = {
        let mut h = Sha1::new();
        h.update(data);
        h.finalize().into()
    };

    let sha256: [u8; 32] = {
        let mut h = Sha256::new();
        h.update(data);
        h.finalize().into()
    };

    (crc32, md5, sha1, sha256)
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

        // Attempt system detection on the combined disc image data
        let (meta, system) = detect_rom(&data)
            .map(|info| {
                let system = info.console().to_string();
                let meta = info.dat_meta();
                (meta, Some(system))
            })
            .unwrap_or_default();

        let title_name = build_game_name(
            stem,
            meta.title.as_deref(),
            meta.region.as_deref(),
            meta.version.as_deref(),
        );
        let name = meta
            .machine_id
            .clone()
            .unwrap_or_else(|| title_name.clone());

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

        let serial = meta.serial;

        let roms = track_hashes
            .iter()
            .enumerate()
            .map(|(i, track)| {
                let track_num: u32 = track.number.parse().unwrap_or(0);
                RomEntry {
                    name: format!("Track {:02}.bin", track_num),
                    size: track.size,
                    crc32: track.crc32,
                    md5: track.md5,
                    sha1: track.sha1,
                    sha256: track.sha256,
                    header: None,
                    serial: if i == 0 { serial.clone() } else { None },
                }
            })
            .collect();

        Ok(vec![GameEntry {
            name,
            description: title_name,
            year,
            manufacturer: meta.manufacturer,
            releases,
            roms,
            system,
        }])
    } else {
        let mut data = Vec::new();
        File::open(path)?.read_to_end(&mut data)?;

        let file_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown")
            .to_string();

        // Attempt system detection
        let (meta, system) = detect_rom(&data)
            .map(|info| {
                let system = info.console().to_string();
                let meta = info.dat_meta();
                (meta, Some(system))
            })
            .unwrap_or_default();

        let title_name = build_game_name(
            stem,
            meta.title.as_deref(),
            meta.region.as_deref(),
            meta.version.as_deref(),
        );
        let name = meta
            .machine_id
            .clone()
            .unwrap_or_else(|| title_name.clone());

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

        let (crc32, md5, sha1, sha256) = compute_hashes(&data);

        let header = if data.len() >= 16 && data[..4] == [0x4E, 0x45, 0x53, 0x1A] {
            let mut h = [0u8; 16];
            h.copy_from_slice(&data[..16]);
            Some(h)
        } else {
            None
        };

        let rom = RomEntry {
            name: file_name,
            size: data.len(),
            crc32,
            md5,
            sha1,
            sha256,
            header,
            serial: meta.serial,
        };

        Ok(vec![GameEntry {
            name,
            description: title_name,
            year,
            manufacturer: meta.manufacturer,
            releases,
            roms: vec![rom],
            system,
        }])
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
    out.push_str("\t\t<description>Generated by retrospector</description>\n");
    out.push_str("\t\t<version>0.1.0</version>\n");
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
            let crc_hex = format!("{:08x}", rom.crc32);
            let md5_hex: String = rom.md5.iter().map(|b| format!("{:02x}", b)).collect();
            let sha1_hex: String = rom.sha1.iter().map(|b| format!("{:02x}", b)).collect();
            let sha256_hex: String = rom.sha256.iter().map(|b| format!("{:02x}", b)).collect();

            let header_attr = rom
                .header
                .map(|h| {
                    let hex: String = h
                        .iter()
                        .map(|b| format!("{:02X}", b))
                        .collect::<Vec<_>>()
                        .join(" ");
                    format!(" header=\"{}\"", hex)
                })
                .unwrap_or_default();

            let serial_attr = rom
                .serial
                .as_deref()
                .map(|s| format!(" serial=\"{}\"", xml_escape(s)))
                .unwrap_or_default();

            out.push_str(&format!(
                "\t\t<rom name=\"{}\" size=\"{}\" crc=\"{}\" md5=\"{}\" sha1=\"{}\" sha256=\"{}\"{}{}/>\n",
                xml_escape(&rom.name),
                rom.size,
                crc_hex,
                md5_hex,
                sha1_hex,
                sha256_hex,
                header_attr,
                serial_attr,
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
    out.push_str("\t\t<description>Generated by retrospector</description>\n");
    out.push_str("\t\t<category></category>\n");
    out.push_str("\t\t<version></version>\n");
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
            let crc_hex = format!("{:08x}", rom.crc32);
            let md5_hex: String = rom.md5.iter().map(|b| format!("{:02x}", b)).collect();
            let sha1_hex: String = rom.sha1.iter().map(|b| format!("{:02x}", b)).collect();

            let header_attr = rom
                .header
                .map(|h| {
                    let hex: String = h
                        .iter()
                        .map(|b| format!("{:02X}", b))
                        .collect::<Vec<_>>()
                        .join(" ");
                    format!(" header=\"{}\"", hex)
                })
                .unwrap_or_default();

            out.push_str(&format!(
                "\t\t<rom name=\"{}\" size=\"{}\" crc=\"{}\" md5=\"{}\" sha1=\"{}\"{}/>\n",
                xml_escape(&rom.name),
                rom.size,
                crc_hex,
                md5_hex,
                sha1_hex,
                header_attr,
            ));
        }

        out.push_str("\t</machine>\n");
    }

    out.push_str("</datafile>\n");
    out
}
