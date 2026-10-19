//! CUE sheet enrichment module for retrospector
//!
//! This module provides functionality to parse CUE sheets, enrich them with
//! retrospector analysis data, and serialize them back to valid CUE format.

pub use crate::output::hash::format_sha1;
use crate::output::hash::{Hashes, compute_hashes};
use crate::systems::disc::{DiscAnalysis, detect_disc};
use rcue::cue::{Cue, CueFile, Track};
use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use std::time::Duration;

/// Analysis data for a single track
#[derive(Debug, Clone)]
pub struct TrackAnalysis {
    pub sha1: String,
    pub size: usize,
}

/// Information about a single track from a CUE sheet
pub struct TrackHash {
    pub number: String,
    pub format: String,
    pub hashes: Hashes,
    pub size: usize,
    pub isrc: Option<String>,
}

/// Hashes for one file referenced by a CUE sheet.
#[derive(Debug, Clone)]
pub struct CueFileHash {
    pub file: String,
    pub hashes: Hashes,
    pub size: usize,
}

/// Sector size based on track format
fn sector_size_for_format(format: &str) -> usize {
    match format.to_uppercase().as_str() {
        "AUDIO" => 2352,
        "MODE1/2048" => 2048,
        "MODE1/2352" => 2352,
        "MODE2/2048" => 2048,
        "MODE2/2324" => 2324,
        "MODE2/2336" => 2336,
        "MODE2/2352" => 2352,
        "CDG" => 2448,
        "CDI/2336" => 2336,
        "CDI/2352" => 2352,
        _ => 2352, // Default to raw
    }
}

/// Convert Duration to byte offset for a given sector size
#[allow(clippy::arithmetic_side_effects)]
fn duration_to_bytes(duration: &Duration, sector_size: usize) -> usize {
    // CD frames are 1/75th of a second
    let total_frames = (duration.as_secs_f64() * 75.0).round() as usize;
    total_frames * sector_size
}

/// Parse a CUE file and calculate SHA1 hashes for each track
/// Returns the combined data, track hashes, and the parsed CUE struct
#[allow(clippy::arithmetic_side_effects)]
pub fn parse_cue_and_hash(cue_path: &Path) -> io::Result<(Vec<u8>, Vec<TrackHash>, Cue)> {
    let mut track_hashes = Vec::new();
    let (all_data, cue) = read_cue_files(cue_path, |cue_file, bin_data| {
        // Calculate track boundaries and hashes
        for (i, track) in cue_file.tracks.iter().enumerate() {
            // Find INDEX 01 (the actual track start)
            let start_duration = track
                .indices
                .iter()
                .find(|(idx, _)| idx == "01" || idx == "1")
                .map(|(_, dur)| dur)
                .or_else(|| track.indices.first().map(|(_, dur)| dur));

            let Some(start_dur) = start_duration else {
                continue;
            };

            let track_sector_size = sector_size_for_format(&track.format);
            let start_byte = duration_to_bytes(start_dur, track_sector_size);

            // End is either the next track's INDEX 01/00, or end of file
            let end_byte = if i + 1 < cue_file.tracks.len() {
                let next_track = &cue_file.tracks[i + 1];
                // Try INDEX 00 (pregap) first, then INDEX 01
                let next_start = next_track
                    .indices
                    .iter()
                    .find(|(idx, _)| idx == "00" || idx == "0")
                    .or_else(|| {
                        next_track
                            .indices
                            .iter()
                            .find(|(idx, _)| idx == "01" || idx == "1")
                    })
                    .map(|(_, dur)| dur);

                next_start
                    .map(|d| duration_to_bytes(d, track_sector_size))
                    .unwrap_or(bin_data.len())
            } else {
                bin_data.len()
            };

            // Clamp to actual data bounds
            let start = start_byte.min(bin_data.len());
            let end = end_byte.min(bin_data.len());

            if start < end {
                let track_data = &bin_data[start..end];
                track_hashes.push(TrackHash {
                    number: track.no.clone(),
                    format: track.format.clone(),
                    hashes: compute_hashes(track_data),
                    size: end - start,
                    isrc: track.isrc.clone(),
                });
            }
        }
        Ok(())
    })?;

    Ok((all_data, track_hashes, cue))
}

/// Parse a CUE file, concatenate its referenced data, and hash each referenced file.
pub fn parse_cue_files_and_hash(cue_path: &Path) -> io::Result<(Vec<u8>, Vec<CueFileHash>)> {
    let mut file_hashes = Vec::new();
    let (all_data, _) = read_cue_files(cue_path, |cue_file, bin_data| {
        file_hashes.push(CueFileHash {
            file: cue_file.file.clone(),
            hashes: compute_hashes(bin_data),
            size: bin_data.len(),
        });
        Ok(())
    })?;

    Ok((all_data, file_hashes))
}

/// Parse a CUE sheet without reading its referenced files.
pub fn parse_cue(cue_path: &Path) -> io::Result<Cue> {
    let cue_path_str = cue_path
        .to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Invalid path encoding"))?;
    rcue::parser::parse_from_file(cue_path_str, false).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("CUE parse error: {:?}", e),
        )
    })
}

fn read_cue_files(
    cue_path: &Path,
    mut visit_file: impl FnMut(&CueFile, &[u8]) -> io::Result<()>,
) -> io::Result<(Vec<u8>, Cue)> {
    let cue = parse_cue(cue_path)?;
    let mut all_data = Vec::new();

    for cue_file in &cue.files {
        // Resolve the referenced data path relative to the CUE file.
        let bin_path = cue_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(&cue_file.file);

        let mut bin_data = Vec::new();
        File::open(&bin_path)?.read_to_end(&mut bin_data)?;
        visit_file(cue_file, &bin_data)?;
        all_data.extend(bin_data);
    }

    Ok((all_data, cue))
}

/// An enriched CUE sheet containing both original data and retrospector analysis
#[derive(Debug)]
pub struct EnrichedCue {
    pub cue: Cue,
    pub disc_analysis: DiscAnalysis,
    /// Map from track number (as string) to analysis
    pub track_analyses: HashMap<String, TrackAnalysis>,
}

impl EnrichedCue {
    pub fn new(cue: Cue) -> Self {
        Self {
            cue,
            disc_analysis: DiscAnalysis::default(),
            track_analyses: HashMap::new(),
        }
    }

    /// Serialize the enriched CUE to valid CUE format
    pub fn serialize(&self) -> String {
        let mut output = String::new();

        // 1. Disc-level analysis as REM comments
        if let Some(ref console) = self.disc_analysis.console {
            writeln!(output, "REM CONSOLE {console}").expect("writing to a String cannot fail");
        }
        if let Some(ref region) = self.disc_analysis.region {
            writeln!(output, "REM REGION {region}").expect("writing to a String cannot fail");
        }
        if let Some(ref product_code) = self.disc_analysis.product_code {
            writeln!(output, "REM SERIAL \"{}\"", escape_quotes(product_code))
                .expect("writing to a String cannot fail");
        }
        if let Some(ref disc_sha1) = self.disc_analysis.disc_sha1 {
            writeln!(output, "REM SHA1 {disc_sha1}").expect("writing to a String cannot fail");
        }

        // 3. Disc-level unknown lines
        for unknown in &self.cue.unknown {
            output.push_str(unknown);
            if !unknown.ends_with('\n') {
                output.push('\n');
            }
        }

        // 4. Disc-level structural commands
        if let Some(ref catalog) = self.cue.catalog {
            writeln!(output, "CATALOG {catalog}").expect("writing to a String cannot fail");
        }
        if let Some(ref cd_text_file) = self.cue.cd_text_file {
            writeln!(output, "CDTEXTFILE \"{}\"", escape_quotes(cd_text_file))
                .expect("writing to a String cannot fail");
        }
        if let Some(ref performer) = self.cue.performer {
            writeln!(output, "PERFORMER \"{}\"", escape_quotes(performer))
                .expect("writing to a String cannot fail");
        }
        if let Some(ref songwriter) = self.cue.songwriter {
            writeln!(output, "SONGWRITER \"{}\"", escape_quotes(songwriter))
                .expect("writing to a String cannot fail");
        }
        // Use original title if present, otherwise use detected title
        if let Some(ref title) = self.cue.title {
            writeln!(output, "TITLE \"{}\"", escape_quotes(title))
                .expect("writing to a String cannot fail");
        } else if let Some(ref title) = self.disc_analysis.title {
            writeln!(output, "TITLE \"{}\"", escape_quotes(title))
                .expect("writing to a String cannot fail");
        }

        // 5. FILE entries
        for file in &self.cue.files {
            self.serialize_file(file, &mut output);
        }

        output
    }

    fn serialize_file(&self, file: &CueFile, output: &mut String) {
        // FILE command
        writeln!(
            output,
            "FILE \"{}\" {}",
            escape_quotes(&file.file),
            file.format
        )
        .expect("writing to a String cannot fail");

        // TRACKs
        for track in &file.tracks {
            self.serialize_track(track, output);
        }
    }

    fn serialize_track(&self, track: &Track, output: &mut String) {
        // TRACK command
        writeln!(output, "  TRACK {} {}", track.no, track.format)
            .expect("writing to a String cannot fail");

        // Track metadata
        if let Some(ref title) = track.title {
            writeln!(output, "    TITLE \"{}\"", escape_quotes(title))
                .expect("writing to a String cannot fail");
        }
        if let Some(ref performer) = track.performer {
            writeln!(output, "    PERFORMER \"{}\"", escape_quotes(performer))
                .expect("writing to a String cannot fail");
        }
        if let Some(ref songwriter) = track.songwriter {
            writeln!(output, "    SONGWRITER \"{}\"", escape_quotes(songwriter))
                .expect("writing to a String cannot fail");
        }
        if let Some(ref isrc) = track.isrc {
            writeln!(output, "    ISRC {isrc}").expect("writing to a String cannot fail");
        }
        if !track.flags.is_empty() {
            writeln!(output, "    FLAGS {}", track.flags.join(" "))
                .expect("writing to a String cannot fail");
        }

        // Track analysis
        if let Some(analysis) = self.track_analyses.get(&track.no) {
            writeln!(output, "    REM SHA1 {}", analysis.sha1)
                .expect("writing to a String cannot fail");
            writeln!(output, "    REM SIZE {}", analysis.size)
                .expect("writing to a String cannot fail");
        }

        // Track unknown lines
        for unknown in &track.unknown {
            output.push_str("    ");
            output.push_str(unknown);
            if !unknown.ends_with('\n') {
                output.push('\n');
            }
        }

        // PREGAP
        if let Some(ref pregap) = track.pregap {
            writeln!(output, "    PREGAP {}", duration_to_timestamp(pregap))
                .expect("writing to a String cannot fail");
        }

        // INDEX commands
        for (idx, duration) in &track.indices {
            writeln!(
                output,
                "    INDEX {} {}",
                idx,
                duration_to_timestamp(duration)
            )
            .expect("writing to a String cannot fail");
        }

        // POSTGAP
        if let Some(ref postgap) = track.postgap {
            writeln!(output, "    POSTGAP {}", duration_to_timestamp(postgap))
                .expect("writing to a String cannot fail");
        }
    }
}

/// Convert Duration to MM:SS:FF format (where FF is frames, 75 frames = 1 second)
#[allow(clippy::arithmetic_side_effects)]
pub fn duration_to_timestamp(duration: &Duration) -> String {
    let total_frames = (duration.as_secs_f64() * 75.0).round() as u64;
    let frames = total_frames % 75;
    let total_seconds = total_frames / 75;
    let seconds = total_seconds % 60;
    let minutes = total_seconds / 60;
    format!("{:02}:{:02}:{:02}", minutes, seconds, frames)
}

/// Escape quotes in a string for CUE format
fn escape_quotes(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .trim()
        .to_string()
}

/// Process a CUE file in cuesheet mode - output enriched CUE
pub fn process_cuesheet(path: &Path) -> io::Result<()> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase());

    if ext.as_deref() != Some("cue") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "The --cuesheet flag requires a .cue file as input",
        ));
    }

    let (buffer, track_hashes, cue) = parse_cue_and_hash(path)?;
    let disc_analysis = detect_disc(&buffer);

    let mut enriched = EnrichedCue::new(cue);
    enriched.disc_analysis = disc_analysis;

    for track in &track_hashes {
        enriched.track_analyses.insert(
            track.number.clone(),
            TrackAnalysis {
                sha1: track.hashes.sha1_hex(),
                size: track.size,
            },
        );
    }

    print!("{}", enriched.serialize());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_duration_to_timestamp() {
        // 0 seconds
        assert_eq!(duration_to_timestamp(&Duration::from_secs(0)), "00:00:00");

        // 1 second = 75 frames
        assert_eq!(duration_to_timestamp(&Duration::from_secs(1)), "00:01:00");

        // 4 minutes, 2 seconds, 33 frames (from the example)
        // 4*60 + 2 = 242 seconds, plus 33/75 seconds
        let dur = Duration::from_secs_f64(242.0 + 33.0 / 75.0);
        assert_eq!(duration_to_timestamp(&dur), "04:02:33");
    }

    #[test]
    fn test_escape_quotes() {
        assert_eq!(escape_quotes("hello"), "hello");
        assert_eq!(escape_quotes("hello \"world\""), "hello \\\"world\\\"");
        assert_eq!(escape_quotes("path\\to\\file"), "path\\\\to\\\\file");
    }

    #[test]
    fn serialize_writes_cue_commands() {
        let mut cue = Cue::new();
        cue.title = Some("Album".into());

        let mut file = CueFile::new("disc.bin", "BINARY");
        let mut track = Track::new("01", "AUDIO");
        track.title = Some("Song".into());
        track.indices.push(("01".into(), Duration::from_secs(1)));
        file.tracks.push(track);
        cue.files.push(file);

        assert_eq!(
            EnrichedCue::new(cue).serialize(),
            "TITLE \"Album\"\nFILE \"disc.bin\" BINARY\n  TRACK 01 AUDIO\n    TITLE \"Song\"\n    INDEX 01 00:01:00\n"
        );
    }
}
