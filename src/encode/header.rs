//! Classic RIFF headers for integer PCM and IEEE f32.

use super::WriteSpec;
use crate::error::{Result, WavError};

pub(super) const MAX_CHANNELS: u16 = 26;
pub(super) const PCM_RIFF_PREFIX: u32 = 36;
pub(super) const FLOAT_RIFF_PREFIX: u32 = 50;
pub(super) const PCM_DATA_LEN_POS: u64 = 40;
pub(super) const FLOAT_DATA_LEN_POS: u64 = 54;
pub(super) const FLOAT_FACT_FRAMES_POS: u64 = 46;

pub(super) fn u32_len(n: usize) -> Result<u32> {
    u32::try_from(n).map_err(|_| WavError::RiffTooLarge)
}

pub(super) fn validate_spec(spec: WriteSpec) -> Result<()> {
    if spec.sample_rate == 0 {
        return Err(WavError::sample_rate(0, 1));
    }
    if spec.channels == 0 || spec.channels > MAX_CHANNELS {
        return Err(WavError::UnsupportedCodec);
    }
    Ok(())
}

pub(super) fn frame_bytes(spec: WriteSpec) -> Result<usize> {
    usize::from(spec.channels)
        .checked_mul(spec.format.bytes_per_sample())
        .filter(|&n| n > 0)
        .ok_or(WavError::RiffTooLarge)
}

pub(super) fn riff_prefix(spec: WriteSpec) -> u32 {
    if spec.format.is_float() {
        FLOAT_RIFF_PREFIX
    } else {
        PCM_RIFF_PREFIX
    }
}

pub(super) fn data_len_pos(spec: WriteSpec) -> u64 {
    if spec.format.is_float() {
        FLOAT_DATA_LEN_POS
    } else {
        PCM_DATA_LEN_POS
    }
}

pub(super) fn fact_frames_pos(spec: WriteSpec) -> Option<u64> {
    spec.format.is_float().then_some(FLOAT_FACT_FRAMES_POS)
}

pub(super) fn push_header(out: &mut Vec<u8>, spec: WriteSpec, data_len: u32) -> Result<()> {
    if spec.format.is_float() {
        let frame = u32::from(spec.channels).saturating_mul(4);
        let frames = data_len.checked_div(frame).unwrap_or(0);
        push_float_header(out, spec.sample_rate, spec.channels, data_len, frames)
    } else {
        push_pcm_header(out, spec, data_len)
    }
}

fn push_pcm_header(out: &mut Vec<u8>, spec: WriteSpec, data_len: u32) -> Result<()> {
    let width = spec.format.bytes_per_sample() as u16;
    let block = spec.channels.saturating_mul(width);
    let byte_rate = spec
        .sample_rate
        .checked_mul(u32::from(block))
        .ok_or(WavError::RiffTooLarge)?;
    let riff_len = PCM_RIFF_PREFIX
        .checked_add(data_len)
        .ok_or(WavError::RiffTooLarge)?;
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&riff_len.to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&spec.channels.to_le_bytes());
    out.extend_from_slice(&spec.sample_rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block.to_le_bytes());
    out.extend_from_slice(&spec.format.bits().to_le_bytes());
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
    let riff_len = FLOAT_RIFF_PREFIX
        .checked_add(data_len)
        .ok_or(WavError::RiffTooLarge)?;
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
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(b"fact");
    out.extend_from_slice(&4u32.to_le_bytes());
    out.extend_from_slice(&frames.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    Ok(())
}
