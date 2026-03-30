//! CUE sheet enrichment module for retrospector
//!
//! This module provides functionality to parse CUE sheets, enrich them with
//! retrospector analysis data, and serialize them back to valid CUE format.

use crate::systems::disc::{detect_disc, DiscAnalysis};
use crc::{Crc, CRC_32_ISO_HDLC};
use md5::Md5;
use rcue::cue::{Cue, CueFile, Track};
use sha1::{Digest, Sha1};
use sha2::Sha256;
use std::collections::HashMap;
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
    pub crc32: u32,
    pub md5: [u8; 16],
    pub sha1: [u8; 20],
    pub sha256: [u8; 32],
    pub size: usize,
    pub isrc: Option<String>,
}

/// Format SHA1 hash as hex string
pub fn format_sha1(hash: &[u8; 20]) -> String {
    hash.iter().map(|b| format!("{:02X}", b)).collect()
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
    let cue_path_str = cue_path
        .to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Invalid path encoding"))?;
    let cue = rcue::parser::parse_from_file(cue_path_str, false).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("CUE parse error: {:?}", e),
        )
    })?;

    let mut all_data = Vec::new();
    let mut track_hashes = Vec::new();

    for cue_file in &cue.files {
        // Resolve the BIN path relative to the CUE file
        let bin_path = cue_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(&cue_file.file);

        let mut bin_data = Vec::new();
        File::open(&bin_path)?.read_to_end(&mut bin_data)?;

        // Determine sector size from first track (usually consistent within a file)
        let _sector_size = cue_file
            .tracks
            .first()
            .map(|t| sector_size_for_format(&t.format))
            .unwrap_or(2352);

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

                let crc_algo = Crc::<u32>::new(&CRC_32_ISO_HDLC);
                let crc32 = crc_algo.checksum(track_data);

                let md5: [u8; 16] = {
                    let mut h = Md5::new();
                    h.update(track_data);
                    h.finalize().into()
                };

                let sha1: [u8; 20] = {
                    let mut h = Sha1::new();
                    h.update(track_data);
                    h.finalize().into()
                };

                let sha256: [u8; 32] = {
                    let mut h = Sha256::new();
                    h.update(track_data);
                    h.finalize().into()
                };

                track_hashes.push(TrackHash {
                    number: track.no.clone(),
                    format: track.format.clone(),
                    crc32,
                    md5,
                    sha1,
                    sha256,
                    size: end - start,
                    isrc: track.isrc.clone(),
                });
            }
        }

        all_data.extend(bin_data);
    }

    Ok((all_data, track_hashes, cue))
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
            output.push_str(&format!("REM CONSOLE {}\n", console));
        }
        if let Some(ref region) = self.disc_analysis.region {
            output.push_str(&format!("REM REGION {}\n", region));
        }
        if let Some(ref product_code) = self.disc_analysis.product_code {
            output.push_str(&format!("REM SERIAL \"{}\"\n", escape_quotes(product_code)));
        }
        if let Some(ref disc_sha1) = self.disc_analysis.disc_sha1 {
            output.push_str(&format!("REM SHA1 {}\n", disc_sha1));
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
            output.push_str(&format!("CATALOG {}\n", catalog));
        }
        if let Some(ref cd_text_file) = self.cue.cd_text_file {
            output.push_str(&format!("CDTEXTFILE \"{}\"\n", escape_quotes(cd_text_file)));
        }
        if let Some(ref performer) = self.cue.performer {
            output.push_str(&format!("PERFORMER \"{}\"\n", escape_quotes(performer)));
        }
        if let Some(ref songwriter) = self.cue.songwriter {
            output.push_str(&format!("SONGWRITER \"{}\"\n", escape_quotes(songwriter)));
        }
        // Use original title if present, otherwise use detected title
        if let Some(ref title) = self.cue.title {
            output.push_str(&format!("TITLE \"{}\"\n", escape_quotes(title)));
        } else if let Some(ref title) = self.disc_analysis.title {
            output.push_str(&format!("TITLE \"{}\"\n", escape_quotes(title)));
        }

        // 5. FILE entries
        for file in &self.cue.files {
            self.serialize_file(file, &mut output);
        }

        output
    }

    fn serialize_file(&self, file: &CueFile, output: &mut String) {
        // FILE command
        output.push_str(&format!(
            "FILE \"{}\" {}\n",
            escape_quotes(&file.file),
            file.format
        ));

        // TRACKs
        for track in &file.tracks {
            self.serialize_track(track, output);
        }
    }

    fn serialize_track(&self, track: &Track, output: &mut String) {
        // TRACK command
        output.push_str(&format!("  TRACK {} {}\n", track.no, track.format));

        // Track metadata
        if let Some(ref title) = track.title {
            output.push_str(&format!("    TITLE \"{}\"\n", escape_quotes(title)));
        }
        if let Some(ref performer) = track.performer {
            output.push_str(&format!("    PERFORMER \"{}\"\n", escape_quotes(performer)));
        }
        if let Some(ref songwriter) = track.songwriter {
            output.push_str(&format!(
                "    SONGWRITER \"{}\"\n",
                escape_quotes(songwriter)
            ));
        }
        if let Some(ref isrc) = track.isrc {
            output.push_str(&format!("    ISRC {}\n", isrc));
        }
        if !track.flags.is_empty() {
            output.push_str(&format!("    FLAGS {}\n", track.flags.join(" ")));
        }

        // Track analysis
        if let Some(analysis) = self.track_analyses.get(&track.no) {
            output.push_str(&format!("    REM SHA1 {}\n", analysis.sha1));
            output.push_str(&format!("    REM SIZE {}\n", analysis.size));
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
            output.push_str(&format!("    PREGAP {}\n", duration_to_timestamp(pregap)));
        }

        // INDEX commands
        for (idx, duration) in &track.indices {
            output.push_str(&format!(
                "    INDEX {} {}\n",
                idx,
                duration_to_timestamp(duration)
            ));
        }

        // POSTGAP
        if let Some(ref postgap) = track.postgap {
            output.push_str(&format!("    POSTGAP {}\n", duration_to_timestamp(postgap)));
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
                sha1: format_sha1(&track.sha1),
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
}
