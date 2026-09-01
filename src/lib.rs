#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(feature = "adpcm")]
mod adpcm;
mod convert;
mod encode;
mod error;
pub(crate) mod header;
mod options;
mod pull;
mod scrub;
pub mod source;
mod wav;

pub use convert::{f32_to_s16le, s16le_to_f32};
pub use encode::{encode_f32, encode_s16, write_s16};
pub use error::{Result, WavError};
pub use options::{
    DEFAULT_MAX_DECODE_SAMPLE_RATE, DEFAULT_MAX_DURATION_SECS, DEFAULT_MAX_OUTPUT_BYTES,
    DEFAULT_MAX_SAMPLE_RATE, DecodeOptions,
};
/// Historical `gigastt-wav` aliases of the `DEFAULT_*` caps.
pub const MAX_DURATION_S: f64 = DEFAULT_MAX_DURATION_SECS;
pub const MAX_SAMPLE_RATE: u32 = DEFAULT_MAX_SAMPLE_RATE;
pub const MAX_DECODE_SAMPLE_RATE: u32 = DEFAULT_MAX_DECODE_SAMPLE_RATE;
pub use source::ByteSource;
pub use wav::{
    DecodedWav, ProbeCodec, StreamBlock, StreamInfo, WavProbe, convert_s16_le_to_f32,
    convert_s16_mono_pub, decode, decode_bytes, decode_f32, decode_s16, decode_streaming,
    decode_with, probe, probe_with, read_f32, read_s16, sniff_is_riff_wave, sniff_wav,
};

/// How decoded channels are laid out in [`DecodedWav::channels`] /
/// [`StreamBlock::planar`].
///
/// Mixing is a left-to-right sum divided by the channel count (same arithmetic
/// the ffmpeg oracle tests use).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChannelMode {
    /// One mixed track (`channels.len() == 1`). Default.
    #[default]
    Mono,
    /// One track per source channel, equal lengths.
    Split,
}

/// Maximum decoded frames for `sample_rate` under [`DecodeOptions::speech`].
#[inline]
pub fn max_decode_samples(sample_rate: u32) -> usize {
    DecodeOptions::default().max_frames(sample_rate)
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod lib_tests;
