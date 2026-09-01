//! Typed errors for sniff / probe / decode / encode (stable `Display`, no
//! `anyhow` / `thiserror` in the public API).

use std::fmt;
use std::io;

/// Result alias for this crate.
pub type Result<T> = std::result::Result<T, WavError>;

/// Errors produced while sniffing, probing, decoding, or encoding WAVE.
#[derive(Debug)]
pub enum WavError {
    /// Underlying `Read` / `Seek` / `Write` failure.
    Io(io::Error),
    /// Stream is not a supported WAVE container.
    NotWave,
    /// `fmt` codec / subtype is not implemented (or disabled by feature).
    UnsupportedCodec,
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
    /// Structural / header / chunk layout failure.
    Format(String),
}

impl WavError {
    #[inline]
    pub fn format(msg: impl Into<String>) -> Self {
        Self::Format(msg.into())
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

    /// Packet / short-read helper used by pull loops.
    #[inline]
    pub(crate) fn packet_io(err: io::Error) -> Self {
        Self::format(format!("Error reading packet: {err}"))
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
            Self::NotWave => write!(f, "Unsupported audio format"),
            Self::UnsupportedCodec => write!(f, "Unsupported audio codec"),
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
            Self::Format(msg) => write!(f, "{msg}"),
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

#[cfg(test)]
#[path = "error_tests.rs"]
mod error_tests;
