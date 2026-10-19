use clap::{Parser, ValueEnum};
use retrospector::output::cue::{parse_cue_and_hash, process_cuesheet};
use retrospector::output::dat;
use retrospector::systems;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Eq, PartialEq, ValueEnum)]
enum OutputFormat {
    Dat,
    #[value(name = "mamedat")]
    MameDat,
    Cuesheet,
}

/// Build the long help text for --system by listing all registered system names.
fn system_help() -> String {
    let mut help = String::from(
        "Force a particular system instead of auto-detecting.\n\nSupported systems:\n",
    );
    for (primary, aliases) in systems::system_names() {
        if aliases.len() > 1 {
            let alt: Vec<&str> = aliases.iter().skip(1).copied().collect();
            help.push_str(&format!("  {:<16} (also: {})\n", primary, alt.join(", ")));
        } else {
            help.push_str(&format!("  {}\n", primary));
        }
    }
    help
}

#[derive(Parser, Debug)]
#[command(
    name = "retrospector",
    about = "Retro Game ROM Inspector",
    version,
    author = "Pat Hawks <pat@pathawks.com>"
)]
struct Cli {
    /// Paths to the ROM files
    paths: Vec<PathBuf>,

    /// Output format: dat, mamedat, or cuesheet
    #[arg(long = "output", value_enum, ignore_case = true)]
    output: Option<OutputFormat>,

    /// Force a particular system instead of auto-detecting
    #[arg(long = "system", long_help = system_help())]
    system: Option<String>,
}

fn main() -> io::Result<()> {
    let args = Cli::parse();

    if args.paths.is_empty() {
        eprintln!("No ROM files provided. Please specify one or more files.");
        return Ok(());
    }

    let multiple_files = args.paths.len() > 1;

    match args.output {
        Some(OutputFormat::Dat) => {
            let all_games = collect_dat_games(&args.paths);
            print!("{}", dat::serialize_dat(&all_games));
        }
        Some(OutputFormat::MameDat) => {
            let all_games = collect_dat_games(&args.paths);
            print!("{}", dat::serialize_mamedat(&all_games));
        }
        Some(OutputFormat::Cuesheet) => {
            for path in &args.paths {
                if multiple_files {
                    println!("; ========================================");
                    println!("; Processing file: {}", path.display());
                    println!("; ----------------------------------------");
                }
                if let Err(e) = process_cuesheet(path) {
                    eprintln!("; Error processing file {}: {}\n", path.display(), e);
                }
            }
        }
        None => {
            for path in &args.paths {
                if multiple_files {
                    println!("========================================");
                    println!("Processing file: {}", path.display());
                    println!("----------------------------------------");
                }
                if let Err(e) = process_file(path, args.system.as_deref()) {
                    eprintln!("Error processing file {}: {}\n", path.display(), e);
                }
            }
        }
    }

    Ok(())
}

fn collect_dat_games(paths: &[PathBuf]) -> Vec<dat::GameEntry> {
    let (games, errors) = dat::collect_games_for_paths(paths);
    for (path, error) in errors {
        eprintln!("Error processing file {}: {}", path.display(), error);
    }
    games
}

fn process_file(path: &Path, system: Option<&str>) -> io::Result<()> {
    let mut buffer = Vec::new();
    let mut track_hashes = Vec::new();

    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
    {
        Some(ext) if ext == "cue" => match parse_cue_and_hash(path) {
            Ok((data, hashes, _cue)) => {
                buffer = data;
                track_hashes = hashes;
            }
            Err(e) => {
                eprintln!("Error parsing CUE file: {}\n", e);
                return Ok(());
            }
        },
        _ => {
            let mut file = File::open(path)?;
            file.read_to_end(&mut buffer)?;
        }
    }

    let buffer = &buffer[..];
    let detected = if let Some(name) = system {
        match systems::lookup_detector(name) {
            Some(detector) => detector(buffer).ok(),
            None => {
                eprintln!(
                    "Unknown system: {}. Run with --help for supported names.",
                    name
                );
                return Ok(());
            }
        }
    } else {
        systems::detect_rom_with_generic_fallback(buffer)
    };

    if let Some(info) = detected {
        println!("Detected Console: {}", info.console());
        println!("{}", info);

        if !track_hashes.is_empty() {
            println!("Track Hashes:");
            for track in &track_hashes {
                println!(
                    "  Track {:>2} ({}): {} ({} bytes)",
                    track.number,
                    track.format,
                    track.hashes.sha1_hex(),
                    track.size
                );
            }
        }
        return Ok(());
    }

    if system.is_some() {
        eprintln!("Unable to read header for the specified system.\n");
    } else {
        eprintln!("Unable to determine console type from the ROM file.\n");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Cli, OutputFormat};
    use clap::Parser;

    #[test]
    fn output_format_values_keep_existing_spelling_and_case_insensitivity() {
        for (value, expected) in [
            ("dat", OutputFormat::Dat),
            ("MAMEDAT", OutputFormat::MameDat),
            ("CueSheet", OutputFormat::Cuesheet),
        ] {
            let cli = Cli::try_parse_from(["retrospector", "--output", value, "rom.bin"])
                .expect("supported output format should parse");
            assert_eq!(cli.output, Some(expected));
        }
    }
}
