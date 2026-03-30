# RetroSpector

**RetroSpector** is a command-line ROM inspector written in Rust. It auto-detects the console type from a ROM file's contents, verifies internal checksums, and displays header metadata such as title, region, mapper, cartridge type, and licensee.

## Installation

Install from [crates.io](https://crates.io/crates/retrospector):

```bash
cargo install retrospector
```

Or clone the repository and build from source:

```bash
git clone https://github.com/pathawks/retrospector.git
cd retrospector
cargo build --release
```

The binary will be at `target/release/retrospector`.

## Usage

```bash
# Inspect a ROM (console is auto-detected)
retrospector game.sfc

# Inspect multiple files at once
retrospector *.gb *.gba

# Force a specific system when auto-detection fails
retrospector --system snes game.bin

# See all supported --system values
retrospector --help
```

### Output Formats

By default, RetroSpector prints human-readable information to the terminal. Use `--output` to produce machine-readable formats:

```bash
# No-Intro/Logiqx DAT XML (CRC32, MD5, SHA1, SHA256)
retrospector --output dat *.sfc

# MAME-style DAT XML
retrospector --output mamedat *.sfc

# Enriched CUE sheet (for disc images)
retrospector --output cuesheet game.cue
```

## Supported Systems

### NES / Famicom

- Header format classification (NES 2.0, iNES, iNES 0.7, Archaic iNES, TNES, raw)
- Mapper and submapper identification (400+ mappers)
- PRG/CHR ROM and RAM sizes, mirroring, battery, trainer
- Console type (NES, Vs. System, PlayChoice-10), timing (NTSC, PAL, Dendy)
- Expansion device identification (40+ device types)
- PRG and CHR CRC32 checksums
- Overdump detection (mirrored/repeated PRG and CHR data)
- Blank ROM detection (all 0x00 or 0xFF)
- Mapper-specific ROM/CHR size validation

### Famicom Disk System

- Disk info block parsing (game name, type, version, side, manufacturing date)
- File table listing (PRG, CHR, nametable files with sizes)
- BCD date decoding (Shōwa, Heisei, and Gregorian eras)
- PRG and CHR CRC32 checksums
- FWNES header detection

### Super NES / Super Famicom

- Title, region, revision, cartridge type
- ROM speed (SlowROM / FastROM) and map mode (LoROM, HiROM, SA-1, ExLoROM, ExHiROM)
- Internal 16-bit checksum verification with mirror-aware calculation
- Copier/trainer header detection and stripping (512-byte preamble)
- Multi-pass header offset detection (7 candidate locations)
- Overdump detection (duplicated data and trailing 0x00/0xFF padding)
- Non-power-of-2 ROM mirroring simulation for checksum verification

### Nintendo 64

- Title (Shift-JIS decoded, NFKC normalized), media format, game code, region, revision
- CRC1/CRC2 verification with automatic CIC chip detection (6103, 6105, 6106)
- Byte-order detection and normalization (z64, n64, v64 formats)

### Game Boy / Game Boy Color

- Title, region, version, cartridge type (MBC1/2/3/5 variants)
- ROM and RAM sizes, SGB and GBC compatibility flags
- Licensee lookup (old 1-byte and new 2-byte codes, 250+ known makers)
- Header checksum and global checksum verification
- Overdump detection

### Game Boy Advance

- Title, game code, region, version
- Maker code lookup (100+ known makers)
- Header checksum verification

### Sega Genesis / Mega Drive / 32X

- Domestic and overseas titles (Shift-JIS decoded)
- Copyright, device support, region codes
- 16-bit checksum verification
- Sonic & Knuckles lock-on cartridge detection and recursive parsing
- Genesis vs. 32X differentiation

### Master System / Game Gear

- Product code (BCD-decoded), version, region
- ROM size validation with header location auto-detection (3 possible offsets)
- 16-bit checksum verification (variable ranges based on ROM size)
- Console type differentiation (SMS vs. Game Gear from region code)

### Nintendo DS

- Title, game code, version
- Nintendo logo CRC and header size validation

### Saturn

- Title, media format, region codes, version, release date, maker ID
- IP.BIN header parsing with magic verification
- ISO 9660 PVD parsing

### Dreamcast

- Title, region codes, version, release date, producer
- IP.BIN header parsing with magic verification
- ISO 9660 PVD parsing

### PlayStation

- ISO 9660 PVD parsing
- System ID verification
- Raw and cooked sector format handling

### Sega CD / Mega CD

- Domestic and overseas titles, copyright, product code, region codes
- SEGADISCSYSTEM magic detection
- Raw and cooked sector format handling
- ISO 9660 PVD parsing

### GameCube

- Title, game code, region, version
- Nintendo disc magic verification

### Wii

- Title, game code, region, version
- Nintendo disc magic verification
- Wii/GameCube differentiation

### CD-i

- ISO 9660 PVD parsing
- CD-I system ID verification

### Atari 7800

- Title, cartridge type
- Header CRC32 verification
- SHA1 hash

### Atari Lynx

- Title
- Header checksum verification
- SHA1 hash

### Atari 2600

- TIA register pattern detection for ROM identification
- Reset vector validation
- Bank alignment checking (2 KB – 128 KB)

### Atari 5200

- CAR header format support
- ANTIC/POKEY hardware register detection
- ANTIC mode 6/7 title decoding

### Atari Jaguar

- Start address verification
- ROM size validation (512 KB – 4 MB, power-of-2)
- SHA1 hash

### ColecoVision

- Boot mode detection (title screen vs. direct boot)
- Title and copyright text parsing (with special glyph decoding)
- Entry point address extraction

### Intellivision

- EXEC header parsing (MOB base, process table, background graphics, title/year)
- Title and copyright year extraction from GRAM string tables
- Standard boot vs. direct boot (0x4800/0x7000 bypass) detection
- 10-bit ROM word validation (big-endian 16-bit storage)

### Odyssey2 / Videopac

- Intel 8048 instruction analysis for ROM identification
- Multi-game vs. single-game classification
- Game selection key dispatch routine analysis

### game.com

- Program name and ID
- Slot configuration and data-only/icon flags
- TigerDMGC cartridge signature detection

### Virtual Boy

- Title (Shift-JIS decoded), game code, region, version
- Maker code lookup
- Power-of-2 ROM size validation (128 KB – 2 MB)

### ISO 9660 (generic fallback)

- Primary Volume Descriptor parsing (volume ID, publisher, dates)
- Fallback for unrecognized disc formats

## Legal

This project is not affiliated with, endorsed by, or sponsored by Nintendo,
Sega, Sony, Atari, Philips, or any other console manufacturer. All trademarks
and registered trademarks are the property of their respective owners. Console
and system names are used solely for identification and interoperability
purposes.
