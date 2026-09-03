//! Untrusted / telephony WAVE -> planar `f32`.
//!
//! [`read`] / [`decode_bytes`] use [`DecodeOptions::default`]: split channels,
//! archival caps. Speech ingest (mix-to-mono, 2 h): [`read_speech`] /
//! [`DecodeOptions::speech`]. G.711 / G.722 / GSM: WAVE tags plus headerless
//! [`decode_g711`] / [`decode_g722`] / [`decode_gsm`] (G.722 is 64 kbit/s
//! only, output 16 kHz; GSM is 8 kHz wav49). Write: PCM U8/S16/S24/S32 +
//! IEEE f32, RIFF or RF64 ([`encode`] / [`WavWriter`]). No ADPCM / G.711 /
//! G.722 / GSM / RIFX encode.
//!
//! [`DecodedWav`], [`WavError`], [`WriteSpec`]. GitHub README is the crate
//! pitch; this page is the API.
//!
//! ```
//! # fn main() -> ryf::Result<()> {
//! let pcm = ryf::f32_to_s16le(&[0.25, -0.5, 0.0]);
//! let wav = ryf::encode_s16(&pcm, 16_000)?;
//! let decoded = ryf::decode_bytes(&wav, ryf::DecodeOptions::default())?;
//! assert_eq!(decoded.frames(), 3);
//! # Ok(())
//! # }
//! ```

#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(feature = "adpcm")]
mod adpcm;
mod convert;
mod encode;
mod error;
mod g711;
mod g722;
mod gsm;
pub(crate) mod header;
mod options;
mod pull;
mod scrub;
pub mod source;
mod wav;

pub use convert::{f32_to_s16le, s16le_to_f32};
pub use encode::{
    WavWriter, WriteFormat, WriteSpec, encode, encode_f32, encode_rf64, encode_s16, write,
    write_f32, write_rf64, write_s16,
};
pub use error::{FormatKind, Result, WavError};
pub use g711::{G711Law, decode_g711, decode_g711_alaw, decode_g711_mulaw};
pub use g722::{decode_g722, decode_g722_mono};
pub use gsm::{decode_gsm, decode_gsm_mono};
pub use options::{
    DEFAULT_MAX_DECODE_SAMPLE_RATE, DEFAULT_MAX_DURATION_SECS, DEFAULT_MAX_OUTPUT_BYTES,
    DEFAULT_MAX_SAMPLE_RATE, DecodeOptions,
};
pub use source::ByteSource;
pub use wav::{
    DecodedWav, ProbeCodec, StreamBlock, StreamInfo, WavProbe, convert_s16_le_to_f32, decode,
    decode_bytes, decode_f32, decode_reader, decode_s16, decode_streaming, decode_with, probe,
    probe_with, read, read_f32, read_s16, read_speech, read_with, sniff_is_riff_wave, sniff_wav,
};

/// How decoded channels are laid out in [`DecodedWav::channels`] /
/// [`StreamBlock::planar`].
///
/// Mixing is a left-to-right sum divided by the channel count (same arithmetic
/// the ffmpeg oracle tests use).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChannelMode {
    /// One mixed track (`channels.len() == 1`).
    Mono,
    /// One track per source channel, equal lengths. Default for library read.
    #[default]
    Split,
}

/// Maximum decoded frames for `sample_rate` under [`DecodeOptions::speech`].
#[inline]
pub fn max_decode_samples(sample_rate: u32) -> usize {
    DecodeOptions::speech().max_frames(sample_rate)
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod lib_tests;
