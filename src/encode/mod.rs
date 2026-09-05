//! WAVE encode: integer PCM 8/16/24/32, IEEE f32, G.711 A-law/mu-law, 1-26
//! channels. Classic RIFF when it fits; RF64 when sizes overflow. RIFX:
//! [`encode_rifx`]. `WAVEFORMATEXTENSIBLE`: [`encode_extensible`].
//! No ADPCM / G.722 / GSM encode.

use std::fs::File;
use std::io::Write;
use std::path::Path;

use crate::error::{Result, WavError};

mod header;
mod writer;

use header::{
    frame_bytes, needs_rf64, needs_rf64_extensible, push_extensible_header, push_header,
    push_rf64_extensible_header, push_rf64_header, push_rifx_header, swap_sample_bytes, u32_len,
    validate_spec,
};
pub use writer::WavWriter;

/// Sample format written into a WAVE `data` chunk (RIFF, RF64, or RIFX).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
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
    /// G.711 A-law (payload is already A-law bytes).
    ALaw,
    /// G.711 mu-law (payload is already mu-law bytes).
    MuLaw,
}

impl WriteFormat {
    /// `fmt ` `wBitsPerSample`.
    #[must_use]
    pub fn bits(self) -> u16 {
        match self {
            Self::U8 | Self::ALaw | Self::MuLaw => 8,
            Self::S16 => 16,
            Self::S24 => 24,
            Self::S32 | Self::F32 => 32,
        }
    }

    /// Container bytes per sample of one channel.
    #[must_use]
    pub fn bytes_per_sample(self) -> usize {
        match self {
            Self::U8 | Self::ALaw | Self::MuLaw => 1,
            Self::S16 => 2,
            Self::S24 => 3,
            Self::S32 | Self::F32 => 4,
        }
    }

    #[must_use]
    pub fn is_float(self) -> bool {
        matches!(self, Self::F32)
    }

    /// IEEE f32 and G.711 write `fmt ` size 18 plus a `fact` chunk.
    #[must_use]
    pub(crate) fn has_fact(self) -> bool {
        matches!(self, Self::F32 | Self::ALaw | Self::MuLaw)
    }

    /// WAVE `wFormatTag` (extensible uses `0xFFFE` instead).
    #[must_use]
    pub fn tag(self) -> u16 {
        match self {
            Self::U8 | Self::S16 | Self::S24 | Self::S32 => 1,
            Self::F32 => 3,
            Self::ALaw => 6,
            Self::MuLaw => 7,
        }
    }
}

/// WAVE write parameters (RIFF, or RF64 when sizes overflow).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
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

    #[must_use]
    pub fn alaw(sample_rate: u32, channels: u16) -> Self {
        Self {
            sample_rate,
            channels,
            format: WriteFormat::ALaw,
        }
    }

    #[must_use]
    pub fn mulaw(sample_rate: u32, channels: u16) -> Self {
        Self {
            sample_rate,
            channels,
            format: WriteFormat::MuLaw,
        }
    }
}

fn prepared(spec: WriteSpec, pcm: &[u8]) -> Result<usize> {
    validate_spec(spec)?;
    let fb = frame_bytes(spec)?;
    if !pcm.len().is_multiple_of(fb) {
        return Err(WavError::OddPcm);
    }
    Ok(fb)
}

fn write_all(path: &Path, bytes: &[u8]) -> Result<()> {
    File::create(path)?.write_all(bytes)?;
    Ok(())
}

fn append_payload(out: &mut Vec<u8>, pcm: &[u8]) {
    out.extend_from_slice(pcm);
    if pcm.len() % 2 == 1 {
        out.push(0);
    }
}

/// Encode interleaved PCM bytes as WAVE.
///
/// Classic RIFF when the sizes fit in `u32`; RF64 otherwise. `pcm` length
/// must be a whole number of frames. Empty `pcm` is a valid header-only WAVE.
pub fn encode(spec: WriteSpec, pcm: &[u8]) -> Result<Vec<u8>> {
    let _fb = prepared(spec, pcm)?;
    let data_len = pcm.len() as u64;
    if needs_rf64(spec, data_len) {
        return encode_rf64(spec, pcm);
    }
    let data_len = u32_len(pcm.len())?;
    let mut out = Vec::with_capacity(58usize.saturating_add(pcm.len()));
    push_header(&mut out, spec, data_len)?;
    append_payload(&mut out, pcm);
    Ok(out)
}

/// Encode interleaved PCM as RF64 even when the file would fit in RIFF.
pub fn encode_rf64(spec: WriteSpec, pcm: &[u8]) -> Result<Vec<u8>> {
    let fb = prepared(spec, pcm)?;
    let data_len = pcm.len() as u64;
    let frames = data_len / fb as u64;
    let mut out = Vec::with_capacity(94usize.saturating_add(pcm.len()));
    push_rf64_header(&mut out, spec, data_len, frames)?;
    append_payload(&mut out, pcm);
    Ok(out)
}

/// Write [`encode_rf64`] output to `path`.
pub fn write_rf64(path: &Path, spec: WriteSpec, pcm: &[u8]) -> Result<()> {
    write_all(path, &encode_rf64(spec, pcm)?)
}

/// Write [`encode`] output to `path`.
pub fn write(path: &Path, spec: WriteSpec, pcm: &[u8]) -> Result<()> {
    write_all(path, &encode(spec, pcm)?)
}

/// Write a mono PCM16 WAVE file.
pub fn write_s16(path: &Path, pcm: &[u8], sample_rate: u32) -> Result<()> {
    write(path, WriteSpec::s16(sample_rate, 1), pcm)
}

/// Write interleaved IEEE f32 WAVE (1-26 channels).
pub fn write_f32(path: &Path, samples: &[f32], sample_rate: u32, channels: u16) -> Result<()> {
    write_all(path, &encode_f32(samples, sample_rate, channels)?)
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
    let nbytes = samples.len().checked_mul(4).ok_or(WavError::RiffTooLarge)?;
    if needs_rf64(spec, nbytes as u64) {
        let mut pcm = Vec::with_capacity(nbytes);
        append_f32_le(&mut pcm, samples);
        return encode_rf64(spec, &pcm);
    }
    let data_len = u32_len(nbytes)?;
    let mut out = Vec::with_capacity(58usize.saturating_add(nbytes));
    push_header(&mut out, spec, data_len)?;
    append_f32_le(&mut out, samples);
    Ok(out)
}

/// Compress interleaved f32 to G.711 A-law WAVE.
pub fn encode_alaw(samples: &[f32], sample_rate: u32, channels: u16) -> Result<Vec<u8>> {
    encode_g711(samples, sample_rate, channels, true)
}

/// Compress interleaved f32 to G.711 mu-law WAVE.
pub fn encode_mulaw(samples: &[f32], sample_rate: u32, channels: u16) -> Result<Vec<u8>> {
    encode_g711(samples, sample_rate, channels, false)
}

fn encode_g711(samples: &[f32], sample_rate: u32, channels: u16, alaw: bool) -> Result<Vec<u8>> {
    let spec = if alaw {
        WriteSpec::alaw(sample_rate, channels)
    } else {
        WriteSpec::mulaw(sample_rate, channels)
    };
    validate_spec(spec)?;
    let ch = usize::from(channels);
    if !samples.len().is_multiple_of(ch) {
        return Err(WavError::OddPcm);
    }
    let s16 = crate::f32_to_s16le(samples);
    let packed = crate::convert::g711::s16le_to_g711(&s16, alaw)?;
    encode(spec, &packed)
}

/// Encode as RIFX (big-endian). `pcm` is the same little-endian payload as [`encode`].
pub fn encode_rifx(spec: WriteSpec, pcm: &[u8]) -> Result<Vec<u8>> {
    let _fb = prepared(spec, pcm)?;
    if needs_rf64(spec, pcm.len() as u64) {
        return Err(WavError::RiffTooLarge);
    }
    let data_len = u32_len(pcm.len())?;
    let payload = swap_sample_bytes(pcm, spec.format.bytes_per_sample());
    let mut out = Vec::with_capacity(58usize.saturating_add(payload.len()));
    push_rifx_header(&mut out, spec, data_len)?;
    append_payload(&mut out, &payload);
    Ok(out)
}

/// Encode with `WAVEFORMATEXTENSIBLE` (`fmt ` size 40).
/// PCM / IEEE / G.711. Classic RIFF when sizes fit; RF64 otherwise.
pub fn encode_extensible(spec: WriteSpec, pcm: &[u8]) -> Result<Vec<u8>> {
    let fb = prepared(spec, pcm)?;
    let data_len = pcm.len() as u64;
    if needs_rf64_extensible(spec, data_len) {
        let frames = data_len / fb as u64;
        let mut out = Vec::with_capacity(116usize.saturating_add(pcm.len()));
        push_rf64_extensible_header(&mut out, spec, data_len, frames)?;
        append_payload(&mut out, pcm);
        return Ok(out);
    }
    let data_len = u32_len(pcm.len())?;
    let mut out = Vec::with_capacity(80usize.saturating_add(pcm.len()));
    push_extensible_header(&mut out, spec, data_len)?;
    append_payload(&mut out, pcm);
    Ok(out)
}

fn append_f32_le(out: &mut Vec<u8>, samples: &[f32]) {
    #[cfg(target_endian = "little")]
    {
        // SAFETY: `f32` is plain IEEE bits; length is `samples.len() * 4`.
        let n = samples.len() * 4;
        let bytes = unsafe { std::slice::from_raw_parts(samples.as_ptr().cast::<u8>(), n) };
        out.extend_from_slice(bytes);
    }
    #[cfg(target_endian = "big")]
    {
        for s in samples {
            out.extend_from_slice(&s.to_le_bytes());
        }
    }
}

#[cfg(test)]
#[path = "../encode_more_tests.rs"]
mod encode_more_tests;
#[cfg(test)]
#[path = "../encode_tests.rs"]
mod encode_tests;
