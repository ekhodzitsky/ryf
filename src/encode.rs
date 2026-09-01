//! Encode mono PCM16 or interleaved IEEE float32 as classic RIFF/WAVE.
//!
//! Product path from molv-wav: PCM16 mono write, IEEE f32 1–2 channels.
//! No RF64, no ADPCM, no RIFX.

use std::fs::File;
use std::io::Write;
use std::path::Path;

use crate::error::{Result, WavError};

fn u32_len(n: usize) -> Result<u32> {
    u32::try_from(n).map_err(|_| WavError::RiffTooLarge)
}

/// Write a mono PCM16 WAVE file.
pub fn write_s16(path: &Path, pcm: &[u8], sample_rate: u32) -> Result<()> {
    let bytes = encode_s16(pcm, sample_rate)?;
    let mut f = File::create(path)?;
    f.write_all(&bytes)?;
    Ok(())
}

/// Encode mono PCM16. `pcm` is little-endian i16; length must be even.
///
/// An empty `pcm` is a valid WAVE (header only). Zero `sample_rate` is
/// [`WavError::UnsupportedSampleRate`].
pub fn encode_s16(pcm: &[u8], sample_rate: u32) -> Result<Vec<u8>> {
    if sample_rate == 0 {
        return Err(WavError::sample_rate(0, 1));
    }
    if !pcm.len().is_multiple_of(2) {
        return Err(WavError::OddPcm);
    }
    let data_len = u32_len(pcm.len())?;
    let mut out = Vec::with_capacity(44usize.saturating_add(pcm.len()));
    push_pcm16_header(&mut out, sample_rate, 1, data_len)?;
    out.extend_from_slice(pcm);
    Ok(out)
}

/// Encode interleaved IEEE float32. `channels` is 1 or 2.
///
/// An empty `samples` is a valid WAVE. Other channel counts are
/// [`WavError::UnsupportedCodec`].
pub fn encode_f32(samples: &[f32], sample_rate: u32, channels: u16) -> Result<Vec<u8>> {
    if sample_rate == 0 {
        return Err(WavError::sample_rate(0, 1));
    }
    if channels == 0 || channels > 2 {
        return Err(WavError::UnsupportedCodec);
    }
    let ch = usize::from(channels);
    if !samples.len().is_multiple_of(ch) {
        return Err(WavError::OddPcm);
    }
    let frames = if samples.is_empty() {
        0
    } else {
        u32_len(samples.len() / ch)?
    };
    let data_len = u32_len(samples.len().saturating_mul(4))?;
    let mut out = Vec::with_capacity(58usize.saturating_add(samples.len().saturating_mul(4)));
    push_float_header(&mut out, sample_rate, channels, data_len, frames)?;
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    Ok(out)
}

fn push_pcm16_header(out: &mut Vec<u8>, sr: u32, ch: u16, data_len: u32) -> Result<()> {
    let block = ch.saturating_mul(2);
    let byte_rate = sr
        .checked_mul(u32::from(block))
        .ok_or(WavError::RiffTooLarge)?;
    push_riff(out, data_len)?;
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&ch.to_le_bytes());
    out.extend_from_slice(&sr.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    Ok(())
}

fn push_float_header(
    out: &mut Vec<u8>,
    sr: u32,
    ch: u16,
    data_len: u32,
    frames: u32,
) -> Result<()> {
    let block = ch.saturating_mul(4);
    let byte_rate = sr
        .checked_mul(u32::from(block))
        .ok_or(WavError::RiffTooLarge)?;
    // WAVE (4) + fmt(8+18) + fact(8+4) + data hdr(8) + data
    let riff_len = 50u32.checked_add(data_len).ok_or(WavError::RiffTooLarge)?;
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&riff_len.to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&18u32.to_le_bytes());
    out.extend_from_slice(&3u16.to_le_bytes());
    out.extend_from_slice(&ch.to_le_bytes());
    out.extend_from_slice(&sr.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // cbSize
    out.extend_from_slice(b"fact");
    out.extend_from_slice(&4u32.to_le_bytes());
    out.extend_from_slice(&frames.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    Ok(())
}

fn push_riff(out: &mut Vec<u8>, data_len: u32) -> Result<()> {
    let riff_len = 36u32.checked_add(data_len).ok_or(WavError::RiffTooLarge)?;
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&riff_len.to_le_bytes());
    out.extend_from_slice(b"WAVE");
    Ok(())
}

#[cfg(test)]
#[path = "encode_tests.rs"]
mod encode_tests;
