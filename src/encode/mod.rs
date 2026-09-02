//! WAVE encode: integer PCM 8/16/24/32, IEEE f32, 1–26 channels.
//!
//! Classic RIFF when it fits; RF64 when the payload would overflow a `u32`
//! size, or via [`encode_rf64`]. Drop-in molv path: [`encode_s16`] (mono) /
//! [`encode_f32`] / [`write_s16`]. No ADPCM, no RIFX.

use std::fs::File;
use std::io::Write;
use std::path::Path;

use crate::error::{Result, WavError};

mod header;
mod writer;

use header::{frame_bytes, needs_rf64, push_header, push_rf64_header, u32_len, validate_spec};
pub use writer::WavWriter;

/// Sample format written as classic little-endian RIFF.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteFormat {
    /// Unsigned 8-bit PCM.
    U8,
    /// Signed 16-bit PCM.
    S16,
    /// Packed signed 24-bit PCM (3-byte containers).
    S24,
    /// Signed 32-bit PCM.
    S32,
    /// IEEE float32.
    F32,
}

impl WriteFormat {
    /// `fmt ` `wBitsPerSample`.
    #[must_use]
    pub fn bits(self) -> u16 {
        match self {
            Self::U8 => 8,
            Self::S16 => 16,
            Self::S24 => 24,
            Self::S32 | Self::F32 => 32,
        }
    }

    /// Container bytes per sample of one channel.
    #[must_use]
    pub fn bytes_per_sample(self) -> usize {
        match self {
            Self::U8 => 1,
            Self::S16 => 2,
            Self::S24 => 3,
            Self::S32 | Self::F32 => 4,
        }
    }

    #[must_use]
    pub fn is_float(self) -> bool {
        matches!(self, Self::F32)
    }
}

/// Classic-RIFF write parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteSpec {
    pub sample_rate: u32,
    /// 1..=26 (same ceiling as decode).
    pub channels: u16,
    pub format: WriteFormat,
}

impl WriteSpec {
    #[must_use]
    pub fn u8(sample_rate: u32, channels: u16) -> Self {
        Self {
            sample_rate,
            channels,
            format: WriteFormat::U8,
        }
    }

    #[must_use]
    pub fn s16(sample_rate: u32, channels: u16) -> Self {
        Self {
            sample_rate,
            channels,
            format: WriteFormat::S16,
        }
    }

    #[must_use]
    pub fn s24(sample_rate: u32, channels: u16) -> Self {
        Self {
            sample_rate,
            channels,
            format: WriteFormat::S24,
        }
    }

    #[must_use]
    pub fn s32(sample_rate: u32, channels: u16) -> Self {
        Self {
            sample_rate,
            channels,
            format: WriteFormat::S32,
        }
    }

    #[must_use]
    pub fn f32(sample_rate: u32, channels: u16) -> Self {
        Self {
            sample_rate,
            channels,
            format: WriteFormat::F32,
        }
    }
}

/// Encode interleaved PCM bytes as WAVE.
///
/// Classic RIFF when the sizes fit in `u32`; RF64 otherwise. `pcm` length
/// must be a whole number of frames. Empty `pcm` is a valid header-only WAVE.
pub fn encode(spec: WriteSpec, pcm: &[u8]) -> Result<Vec<u8>> {
    validate_spec(spec)?;
    let fb = frame_bytes(spec)?;
    if !pcm.len().is_multiple_of(fb) {
        return Err(WavError::OddPcm);
    }
    let data_len = pcm.len() as u64;
    if needs_rf64(spec, data_len) {
        return encode_rf64(spec, pcm);
    }
    let data_len = u32_len(pcm.len())?;
    let mut out = Vec::with_capacity(58usize.saturating_add(pcm.len()));
    push_header(&mut out, spec, data_len)?;
    out.extend_from_slice(pcm);
    Ok(out)
}

/// Encode interleaved PCM as RF64 even when the file would fit in RIFF.
pub fn encode_rf64(spec: WriteSpec, pcm: &[u8]) -> Result<Vec<u8>> {
    validate_spec(spec)?;
    let fb = frame_bytes(spec)?;
    if !pcm.len().is_multiple_of(fb) {
        return Err(WavError::OddPcm);
    }
    let data_len = pcm.len() as u64;
    let frames = data_len / fb as u64;
    let mut out = Vec::with_capacity(94usize.saturating_add(pcm.len()));
    push_rf64_header(&mut out, spec, data_len, frames)?;
    out.extend_from_slice(pcm);
    Ok(out)
}

/// Write [`encode_rf64`] output to `path`.
pub fn write_rf64(path: &Path, spec: WriteSpec, pcm: &[u8]) -> Result<()> {
    let bytes = encode_rf64(spec, pcm)?;
    let mut f = File::create(path)?;
    f.write_all(&bytes)?;
    Ok(())
}

/// Write [`encode`] output to `path`.
pub fn write(path: &Path, spec: WriteSpec, pcm: &[u8]) -> Result<()> {
    let bytes = encode(spec, pcm)?;
    let mut f = File::create(path)?;
    f.write_all(&bytes)?;
    Ok(())
}

/// Write a mono PCM16 WAVE file (molv drop-in).
pub fn write_s16(path: &Path, pcm: &[u8], sample_rate: u32) -> Result<()> {
    write(path, WriteSpec::s16(sample_rate, 1), pcm)
}

/// Write interleaved IEEE f32 WAVE (1–26 channels).
pub fn write_f32(path: &Path, samples: &[f32], sample_rate: u32, channels: u16) -> Result<()> {
    let bytes = encode_f32(samples, sample_rate, channels)?;
    let mut f = File::create(path)?;
    f.write_all(&bytes)?;
    Ok(())
}

/// Encode mono PCM16. `pcm` is little-endian i16; length must be even.
///
/// An empty `pcm` is a valid WAVE (header only). Zero `sample_rate` is
/// [`WavError::UnsupportedSampleRate`].
pub fn encode_s16(pcm: &[u8], sample_rate: u32) -> Result<Vec<u8>> {
    encode(WriteSpec::s16(sample_rate, 1), pcm)
}

/// Encode interleaved IEEE float32. `channels` is 1..=26.
///
/// An empty `samples` is a valid WAVE.
pub fn encode_f32(samples: &[f32], sample_rate: u32, channels: u16) -> Result<Vec<u8>> {
    let spec = WriteSpec::f32(sample_rate, channels);
    validate_spec(spec)?;
    let ch = usize::from(channels);
    if !samples.len().is_multiple_of(ch) {
        return Err(WavError::OddPcm);
    }
    let mut pcm = Vec::with_capacity(samples.len().saturating_mul(4));
    for s in samples {
        pcm.extend_from_slice(&s.to_le_bytes());
    }
    encode(spec, &pcm)
}

#[cfg(test)]
#[path = "../encode_tests.rs"]
mod encode_tests;
