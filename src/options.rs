//! Decode / probe configuration.

use crate::ChannelMode;

/// Speech-ingest caps (2 h, 192 kHz, 4 GiB planar f32). Used by
/// [`DecodeOptions::speech`] / [`crate::read_speech`]. Library default is
/// [`DecodeOptions::unbounded`] (split channels).
pub const DEFAULT_MAX_DURATION_SECS: f64 = 7200.0;
pub const DEFAULT_MAX_SAMPLE_RATE: u32 = 192_000;
/// Ceiling applied when converting duration to frame budget (limits RAM
/// estimate if a file lies about a huge sample rate).
pub const DEFAULT_MAX_DECODE_SAMPLE_RATE: u32 = 48_000;
/// Default planar-f32 RAM budget (covers 2 h stereo @ 48 kHz, rejects 8+ ch
/// at the duration ceiling).
pub const DEFAULT_MAX_OUTPUT_BYTES: u64 = 4 * 1024 * 1024 * 1024;

/// Options for [`crate::decode_with`] / [`crate::probe_with`].
#[derive(Debug, Clone, PartialEq)]
pub struct DecodeOptions {
    pub channel_mode: ChannelMode,
    /// Hard upper bound on decoded audio length (seconds).
    pub max_duration_secs: f64,
    /// Reject headers whose sample rate is above this value (or zero).
    pub max_sample_rate: u32,
    /// Cap used when deriving the frame budget from `max_duration_secs`.
    pub max_decode_sample_rate: u32,
    /// Hard cap on decoded planar f32 bytes (`n_out * frames * 4`).
    pub max_output_bytes: u64,
    /// Optional caller label (path / upload id). Stored on the options;
    /// not interpolated into [`crate::WavError`] messages.
    pub source_label: Option<String>,
}

impl Default for DecodeOptions {
    fn default() -> Self {
        Self::unbounded()
    }
}

impl DecodeOptions {
    /// Mix-to-mono + speech-ingest caps (2 h, 192 kHz, 4 GiB planar).
    pub fn speech() -> Self {
        Self {
            channel_mode: ChannelMode::Mono,
            max_duration_secs: DEFAULT_MAX_DURATION_SECS,
            max_sample_rate: DEFAULT_MAX_SAMPLE_RATE,
            max_decode_sample_rate: DEFAULT_MAX_DECODE_SAMPLE_RATE,
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
            source_label: None,
        }
    }

    /// Split channels, no practical duration / rate ceiling (library default).
    ///
    /// Still uses a finite decode-rate ceiling of 192 kHz when sizing the
    /// frame budget so a corrupt header cannot request petabytes of RAM
    /// from a single multiplication.
    pub fn unbounded() -> Self {
        Self {
            channel_mode: ChannelMode::Split,
            max_duration_secs: (usize::MAX / 4) as f64 / 192_000.0,
            max_sample_rate: 384_000,
            max_decode_sample_rate: 192_000,
            max_output_bytes: u64::MAX / 2,
            source_label: None,
        }
    }

    #[inline]
    pub fn with_channel_mode(mut self, mode: ChannelMode) -> Self {
        self.channel_mode = mode;
        self
    }

    #[inline]
    pub fn with_max_duration_secs(mut self, secs: f64) -> Self {
        self.max_duration_secs = secs;
        self
    }

    #[inline]
    pub fn with_max_sample_rate(mut self, rate: u32) -> Self {
        self.max_sample_rate = rate;
        self
    }

    #[inline]
    pub fn with_max_decode_sample_rate(mut self, rate: u32) -> Self {
        self.max_decode_sample_rate = rate;
        self
    }

    #[inline]
    pub fn with_source_label(mut self, label: impl Into<String>) -> Self {
        self.source_label = Some(label.into());
        self
    }

    #[inline]
    pub fn with_max_output_bytes(mut self, bytes: u64) -> Self {
        self.max_output_bytes = bytes;
        self
    }

    /// Maximum decoded frames allowed at `sample_rate` under these options.
    #[inline]
    pub fn max_frames(&self, sample_rate: u32) -> usize {
        let rate = sample_rate.min(self.max_decode_sample_rate).max(1) as f64;
        let frames = self.max_duration_secs * rate;
        if !frames.is_finite() || frames <= 0.0 {
            return 0;
        }
        frames.min(usize::MAX as f64) as usize
    }

    #[inline]
    pub fn source_label_str(&self) -> &str {
        self.source_label.as_deref().unwrap_or("wav")
    }
}

#[cfg(test)]
#[path = "options_tests.rs"]
mod options_tests;
