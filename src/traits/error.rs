/// Lightweight typed parse errors for ROM/disc detection.
///
/// Replaces bare `Result<_, ()>` across all system parsers to improve
/// internal traceability without changing CLI behavior.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ParseError {
    /// The input buffer is smaller than the minimum required size.
    BufferTooSmall,
    /// Expected magic bytes or signature not found.
    MagicNotFound,
    /// Header or metadata is present but malformed or inconsistent.
    InvalidHeader,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BufferTooSmall => write!(f, "buffer too small"),
            Self::MagicNotFound => write!(f, "magic bytes not found"),
            Self::InvalidHeader => write!(f, "invalid header"),
        }
    }
}

impl std::error::Error for ParseError {}
