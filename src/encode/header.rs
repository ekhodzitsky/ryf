//! RIFF and RF64 headers for integer PCM and IEEE f32.

use super::{WriteFormat, WriteSpec};
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
        return Err(WavError::unsupported_codec(0));
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

pub(super) fn needs_rf64(spec: WriteSpec, data_len: u64) -> bool {
    u64::from(riff_prefix(spec)).saturating_add(data_len) > u64::from(u32::MAX)
}

/// RF64 header bytes including the `data` size field (payload follows).
pub(super) fn rf64_header_len(spec: WriteSpec) -> u64 {
    // RF64+WAVE (12) + ds64 (36) + fmt + optional fact + data hdr (8).
    if spec.format.is_float() { 94 } else { 80 }
}

pub(super) const RF64_RIFF_SIZE_POS: u64 = 20;
pub(super) const RF64_DATA_SIZE_POS: u64 = 28;
pub(super) const RF64_SAMPLE_COUNT_POS: u64 = 36;
/// `fact` sampleCount in an RF64 IEEE header (after ds64 + WAVEFORMATEX).
pub(super) const RF64_FACT_FRAMES_POS: u64 = 82;

pub(super) fn push_rf64_header(
    out: &mut Vec<u8>,
    spec: WriteSpec,
    data_len: u64,
    frames: u64,
) -> Result<()> {
    let header_len = rf64_header_len(spec);
    let file_len = header_len.saturating_add(data_len);
    let riff_size = file_len.saturating_sub(8);
    out.extend_from_slice(b"RF64");
    out.extend_from_slice(&u32::MAX.to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"ds64");
    out.extend_from_slice(&28u32.to_le_bytes());
    out.extend_from_slice(&riff_size.to_le_bytes());
    out.extend_from_slice(&data_len.to_le_bytes());
    out.extend_from_slice(&frames.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    push_fmt_and_data(out, spec, u32::MAX, frames.min(u64::from(u32::MAX)) as u32)
}

fn push_fmt_and_data(
    out: &mut Vec<u8>,
    spec: WriteSpec,
    data_len: u32,
    frames_u32: u32,
) -> Result<()> {
    if spec.format.is_float() {
        let block = spec.channels.saturating_mul(4);
        let byte_rate = spec
            .sample_rate
            .checked_mul(u32::from(block))
            .ok_or(WavError::RiffTooLarge)?;
        out.extend_from_slice(b"fmt ");
        out.extend_from_slice(&18u32.to_le_bytes());
        out.extend_from_slice(&3u16.to_le_bytes());
        out.extend_from_slice(&spec.channels.to_le_bytes());
        out.extend_from_slice(&spec.sample_rate.to_le_bytes());
        out.extend_from_slice(&byte_rate.to_le_bytes());
        out.extend_from_slice(&block.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(b"fact");
        out.extend_from_slice(&4u32.to_le_bytes());
        out.extend_from_slice(&frames_u32.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_len.to_le_bytes());
        Ok(())
    } else {
        push_pcm_fmt_data(out, spec, data_len)
    }
}

fn push_pcm_fmt_data(out: &mut Vec<u8>, spec: WriteSpec, data_len: u32) -> Result<()> {
    let width = spec.format.bytes_per_sample() as u16;
    let block = spec.channels.saturating_mul(width);
    let byte_rate = spec
        .sample_rate
        .checked_mul(u32::from(block))
        .ok_or(WavError::RiffTooLarge)?;
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

fn push_pcm_header(out: &mut Vec<u8>, spec: WriteSpec, data_len: u32) -> Result<()> {
    let riff_len = PCM_RIFF_PREFIX
        .checked_add(data_len)
        .ok_or(WavError::RiffTooLarge)?;
    let width = spec.format.bytes_per_sample() as u16;
    let block = spec.channels.saturating_mul(width);
    let byte_rate = spec
        .sample_rate
        .checked_mul(u32::from(block))
        .ok_or(WavError::RiffTooLarge)?;
    let mut h = [0u8; 44];
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&riff_len.to_le_bytes());
    h[8..12].copy_from_slice(b"WAVE");
    h[12..16].copy_from_slice(b"fmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes());
    h[20..22].copy_from_slice(&1u16.to_le_bytes());
    h[22..24].copy_from_slice(&spec.channels.to_le_bytes());
    h[24..28].copy_from_slice(&spec.sample_rate.to_le_bytes());
    h[28..32].copy_from_slice(&byte_rate.to_le_bytes());
    h[32..34].copy_from_slice(&block.to_le_bytes());
    h[34..36].copy_from_slice(&spec.format.bits().to_le_bytes());
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&data_len.to_le_bytes());
    out.extend_from_slice(&h);
    Ok(())
}

fn push_float_header(
    out: &mut Vec<u8>,
    sr: u32,
    ch: u16,
    data_len: u32,
    frames: u32,
) -> Result<()> {
    let spec = WriteSpec {
        sample_rate: sr,
        channels: ch,
        format: WriteFormat::F32,
    };
    let riff_len = FLOAT_RIFF_PREFIX
        .checked_add(data_len)
        .ok_or(WavError::RiffTooLarge)?;
    let block = spec.channels.saturating_mul(4);
    let byte_rate = spec
        .sample_rate
        .checked_mul(u32::from(block))
        .ok_or(WavError::RiffTooLarge)?;
    let mut h = [0u8; 58];
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&riff_len.to_le_bytes());
    h[8..12].copy_from_slice(b"WAVE");
    h[12..16].copy_from_slice(b"fmt ");
    h[16..20].copy_from_slice(&18u32.to_le_bytes());
    h[20..22].copy_from_slice(&3u16.to_le_bytes());
    h[22..24].copy_from_slice(&spec.channels.to_le_bytes());
    h[24..28].copy_from_slice(&spec.sample_rate.to_le_bytes());
    h[28..32].copy_from_slice(&byte_rate.to_le_bytes());
    h[32..34].copy_from_slice(&block.to_le_bytes());
    h[34..36].copy_from_slice(&32u16.to_le_bytes());
    h[36..38].copy_from_slice(&0u16.to_le_bytes());
    h[38..42].copy_from_slice(b"fact");
    h[42..46].copy_from_slice(&4u32.to_le_bytes());
    h[46..50].copy_from_slice(&frames.to_le_bytes());
    h[50..54].copy_from_slice(b"data");
    h[54..58].copy_from_slice(&data_len.to_le_bytes());
    out.extend_from_slice(&h);
    Ok(())
}
