//! # ryf
//!
//! just wav.
//!
//! Pure-Rust **WAVE family** reader (RIFF / RIFX / RF64 / BW64 / Wave64).
//!
//! ## Support matrix
//!
//! | Containers | Codecs |
//! |------------|--------|
//! | RIFF / WAVE | PCM int 8 / 16 / 24 / 32 (incl. S24 in 4-byte containers) |
//! | RIFX (big-endian) | IEEE float 32 / 64 |
//! | RF64 / BW64 (`ds64`) | G.711 A-law / µ-law |
//! | Sony Wave64 | MS-ADPCM, IMA/DVI ADPCM (`adpcm` feature) |
//! | Write: classic RIFF | PCM16 mono, IEEE f32 1–2 ch |
//!
//! Also: `WAVE_FORMAT_EXTENSIBLE` (PCM / float / G.711 + Ambisonic GUIDs),
//! wild-file quirks (`valid_bits=0`, empty channel mask, short `data`,
//! streaming `u32::MAX` sizes).
//!
//! ## Non-goals
//!
//! - RF64 / ADPCM / RIFX **encode** (write path is classic RIFF PCM16 + IEEE f32)
//! - GSM, MPEG-in-WAV, and other exotic codecs
//! - Resampling
//! - Async I/O (sync demux; call from `spawn_blocking` in async apps)
//! - `mmap`, `zeroize`, `tracing`
//!
//! Default features (`adpcm`, `simd`): **no external crates** (pure `std`).
//!
//! ```no_run
//! use ryf::{ChannelMode, DecodeOptions, decode_bytes};
//!
//! let opts = DecodeOptions::speech().with_channel_mode(ChannelMode::Mono);
//! let data = std::fs::read("speech.wav")?;
//! let decoded = decode_bytes(&data, opts)?;
//! let _ = (decoded.sample_rate, decoded.channels[0].len());
//! # Ok::<(), ryf::WavError>(())
//! ```

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

/// Whether decoded channels are mixed to mono or kept separate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChannelMode {
    #[default]
    Mono,
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
