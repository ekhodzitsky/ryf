//! Typed errors for sniff / probe / decode / encode (stable `Display`, no
//! `anyhow` / `thiserror` in the public API).

use std::fmt;
use std::io;

/// Result alias for this crate.
pub type Result<T> = std::result::Result<T, WavError>;

/// Structural failure inside a WAVE container (the bytes started as WAVE).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatKind {
    /// Short read while walking a chunk.
    Truncated,
    /// `fmt ` body is truncated or internally inconsistent.
    MalformedFmt,
    /// ds64 / fact / LIST / RIFF size is broken.
    MalformedChunk,
    /// Required chunk missing (`fmt`, `data`, `ds64`).
    MissingChunk,
    /// Channel count or speaker mask is invalid.
    ChannelLayout,
    /// Bits / block align / frame size / payload length is invalid.
    InvalidSize,
    /// Known container, unknown PCM/IEEE layout.
    UnsupportedWaveFormat,
    /// ADPCM `fmt` extra or block header is invalid.
    Adpcm,
    /// API misuse (finalized writer, missing plane, bad callback).
    InvalidOperation,
}

impl fmt::Display for FormatKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Truncated => "truncated WAVE chunk",
            Self::MalformedFmt => "malformed fmt chunk",
            Self::MalformedChunk => "malformed WAVE chunk",
            Self::MissingChunk => "missing required WAVE chunk",
            Self::ChannelLayout => "invalid channel layout",
            Self::InvalidSize => "invalid WAVE size or frame layout",
            Self::UnsupportedWaveFormat => "unsupported WAVE format layout",
            Self::Adpcm => "malformed ADPCM block or fmt extra",
            Self::InvalidOperation => "invalid WAVE operation",
        })
    }
}

/// Errors produced while sniffing, probing, decoding, or encoding WAVE.
#[derive(Debug)]
pub enum WavError {
    /// Underlying `Read` / `Seek` / `Write` failure.
    Io(io::Error),
    /// Stream is not a WAVE container (wrong magic / form).
    NotWave,
    /// `fmt` codec / subtype / channel layout is not implemented.
    ///
    /// `tag` is the WAVE `wFormatTag` (or extensible tag `0xFFFE`) when known;
    /// `0` means the API rejected a layout that is not a format tag (e.g. not
    /// PCM16 mono on [`crate::decode_s16`], G.711/G.722 channel count, write spec).
    UnsupportedCodec {
        /// WAVE format tag, or `0` if this was not a tagged codec.
        tag: u16,
    },
    /// Sample rate is zero or above the configured ceiling.
    UnsupportedSampleRate { rate: u32, max: u32 },
    /// Decoded (or declared) duration exceeds the configured budget.
    TooLong { observed_secs: f64, max_secs: f64 },
    /// Planar f32 output would exceed the configured RAM budget.
    OutputTooLarge { bytes: u64, max: u64 },
    /// `ByteSource` has no known length (required for bounded PCM decode).
    StreamLengthUnknown,
    /// Optional crate feature is off for this codec path.
    FeatureDisabled { feature: &'static str },
    /// PCM payload is not a whole number of frames (or s16 length is odd).
    OddPcm,
    /// WAVE `data` chunk decoded to zero samples.
    Empty,
    /// Encoded RIFF size does not fit in `u32`.
    RiffTooLarge,
    /// WAVE container with a broken chunk walk.
    Format(FormatKind),
}

impl WavError {
    #[inline]
    pub fn format(kind: FormatKind) -> Self {
        Self::Format(kind)
    }

    #[inline]
    pub fn unsupported_codec(tag: u16) -> Self {
        Self::UnsupportedCodec { tag }
    }

    /// Packet / short-read helper used by pull loops.
    #[inline]
    pub(crate) fn packet_io(err: io::Error) -> Self {
        Self::Io(err)
    }

    #[inline]
    pub fn too_long(observed_secs: f64, max_secs: f64) -> Self {
        Self::TooLong {
            observed_secs,
            max_secs,
        }
    }

    #[inline]
    pub fn sample_rate(rate: u32, max: u32) -> Self {
        Self::UnsupportedSampleRate { rate, max }
    }

    #[inline]
    pub fn output_too_large(bytes: u64, max: u64) -> Self {
        Self::OutputTooLarge { bytes, max }
    }

    /// Whether this error should surface as a generic unsupported-format class
    /// in higher layers (vs codec / duration / IO).
    pub fn is_format_class(&self) -> bool {
        matches!(
            self,
            Self::NotWave
                | Self::Format(_)
                | Self::StreamLengthUnknown
                | Self::OddPcm
                | Self::Empty
        )
    }
}

impl fmt::Display for WavError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::NotWave => write!(f, "not a WAVE container"),
            Self::UnsupportedCodec { tag } if *tag == 0 => {
                write!(f, "unsupported audio codec")
            }
            Self::UnsupportedCodec { tag } => {
                write!(f, "unsupported audio codec (format tag 0x{tag:04x})")
            }
            Self::UnsupportedSampleRate { rate, max } => {
                write!(f, "Unsupported sample rate: {rate}Hz (max {max}Hz)")
            }
            Self::TooLong {
                observed_secs,
                max_secs,
            } => write!(
                f,
                "Audio file too long ({observed_secs:.0}s). Maximum supported: {max_secs:.0}s."
            ),
            Self::OutputTooLarge { bytes, max } => {
                write!(
                    f,
                    "wav: decoded output too large ({bytes} bytes, max {max})"
                )
            }
            Self::StreamLengthUnknown => write!(f, "wav: stream length unknown"),
            Self::FeatureDisabled { feature } => {
                write!(f, "wav: feature `{feature}` is not enabled in this build")
            }
            Self::OddPcm => write!(f, "PCM length is not a whole number of frames"),
            Self::Empty => write!(f, "WAVE data chunk is empty"),
            Self::RiffTooLarge => write!(f, "WAVE payload does not fit in a RIFF u32"),
            Self::Format(kind) => write!(f, "{kind}"),
        }
    }
}

impl std::error::Error for WavError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for WavError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<FormatKind> for WavError {
    fn from(kind: FormatKind) -> Self {
        Self::Format(kind)
    }
}

#[cfg(test)]
#[path = "error_tests.rs"]
mod error_tests;
