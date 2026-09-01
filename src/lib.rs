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
//!
//! Also: `WAVE_FORMAT_EXTENSIBLE` (PCM / float / G.711 + Ambisonic GUIDs),
//! wild-file quirks (`valid_bits=0`, empty channel mask, short `data`,
//! streaming `u32::MAX` sizes).
//!
//! ## Non-goals
//!
//! - Encoding / writing WAVE
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
mod error;
pub(crate) mod header;
mod options;
mod pull;
mod scrub;
pub mod source;
mod wav;

pub use error::{Result, WavError};
pub use options::{
    DEFAULT_MAX_DECODE_SAMPLE_RATE, DEFAULT_MAX_DURATION_SECS, DEFAULT_MAX_OUTPUT_BYTES,
    DEFAULT_MAX_SAMPLE_RATE, DecodeOptions,
};
pub use source::ByteSource;
pub use wav::{
    DecodedWav, ProbeCodec, StreamBlock, StreamInfo, WavProbe, convert_s16_le_to_f32,
    convert_s16_mono_pub, decode, decode_bytes, decode_streaming, decode_with, probe, probe_with,
    sniff_is_riff_wave,
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
