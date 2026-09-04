//! G.722 pull / collect (stateful; 1 encoded byte is 2 PCM samples / ch).

use super::{
    DecodePlan, check_duration, emit_mono_block, emit_split_block, pcm_short, scratch_frames,
};
use crate::ChannelMode;
use crate::error::{Result, WavError};
use crate::g722::{self, G722Decoder};
use crate::scrub::scrub_vec;
use crate::source::ByteSource;

pub(super) fn collect_g722(mss: &mut ByteSource<'_>, plan: &DecodePlan) -> Result<Vec<Vec<f32>>> {
    let total = plan.total_frames;
    if total > plan.max_samples {
        check_duration(total, plan.max_samples, plan.sample_rate)?;
    }
    let channels = plan.channels;
    if total == 0 || channels == 0 {
        return Ok(super::empty_planes(plan.mode, channels));
    }
    let need = total.div_ceil(2).saturating_mul(channels);
    let mut planes = if let Some(rest) = mss.remaining_slice() {
        if rest.len() < need {
            return Err(pcm_short("wav: short g722 data"));
        }
        g722::decode_planar(&rest[..need], channels, plan.mode)
    } else {
        let mut owned = scrub_vec(vec![0u8; need]);
        mss.read_buf_exact(&mut owned)
            .map_err(WavError::packet_io)?;
        g722::decode_planar(&owned, channels, plan.mode)
    };
    for p in &mut planes {
        p.truncate(total);
    }
    if mss.remaining_slice().is_some() {
        mss.advance(need as u64).map_err(WavError::packet_io)?;
    }
    Ok(planes)
}

pub(super) fn pull_g722<F>(
    mss: &mut ByteSource<'_>,
    plan: &DecodePlan,
    on_block: &mut F,
) -> Result<usize>
where
    F: FnMut(super::StreamBlock<'_>) -> Result<()>,
{
    let channels = plan.channels;
    let total_pcm = plan.total_frames;
    if total_pcm > plan.max_samples {
        check_duration(total_pcm, plan.max_samples, plan.sample_rate)?;
    }
    if total_pcm == 0 || channels == 0 {
        return Ok(0);
    }
    let pairs_total = total_pcm.div_ceil(2);
    let need = pairs_total.saturating_mul(channels);
    let rate = plan.sample_rate;
    let mut decs: Vec<G722Decoder> = (0..channels).map(|_| G722Decoder::new()).collect();

    if let Some(rest) = mss.remaining_slice() {
        if rest.len() < need {
            return Err(pcm_short("wav: short g722 data"));
        }
        let n = emit_from_bytes(
            &rest[..need],
            channels,
            total_pcm,
            plan.mode,
            rate,
            &mut decs,
            on_block,
        )?;
        mss.advance(need as u64).map_err(WavError::packet_io)?;
        return Ok(n);
    }

    let block_pairs = scratch_frames(channels, pairs_total);
    let mut raw = scrub_vec(vec![0u8; block_pairs * channels]);
    let mut decoded = 0usize;
    let mut pairs_left = pairs_total;
    while pairs_left > 0 {
        let pairs_this = pairs_left.min(block_pairs);
        let want = pairs_this * channels;
        mss.read_buf_exact(&mut raw[..want])
            .map_err(WavError::packet_io)?;
        let pcm_this = (pairs_this * 2).min(total_pcm - decoded);
        decoded += emit_chunk(
            &raw[..want],
            channels,
            pcm_this,
            plan.mode,
            rate,
            &mut decs,
            on_block,
        )?;
        pairs_left -= pairs_this;
    }
    Ok(decoded)
}

fn emit_from_bytes<F>(
    data: &[u8],
    channels: usize,
    total_pcm: usize,
    mode: ChannelMode,
    rate: u32,
    decs: &mut [G722Decoder],
    on_block: &mut F,
) -> Result<usize>
where
    F: FnMut(super::StreamBlock<'_>) -> Result<()>,
{
    let pairs = data.len() / channels;
    let block_pairs = scratch_frames(channels, pairs);
    let mut decoded = 0usize;
    let mut off = 0usize;
    while off < pairs {
        let pairs_this = (pairs - off).min(block_pairs);
        let start = off * channels;
        let end = start + pairs_this * channels;
        let pcm_this = (pairs_this * 2).min(total_pcm - decoded);
        decoded += emit_chunk(
            &data[start..end],
            channels,
            pcm_this,
            mode,
            rate,
            decs,
            on_block,
        )?;
        off += pairs_this;
    }
    Ok(decoded)
}

fn emit_chunk<F>(
    raw: &[u8],
    channels: usize,
    pcm_this: usize,
    mode: ChannelMode,
    rate: u32,
    decs: &mut [G722Decoder],
    on_block: &mut F,
) -> Result<usize>
where
    F: FnMut(super::StreamBlock<'_>) -> Result<()>,
{
    let mut planes: Vec<Vec<f32>> = match mode {
        ChannelMode::Mono => vec![vec![0.0f32; pcm_this]],
        ChannelMode::Split => (0..channels).map(|_| vec![0.0f32; pcm_this]).collect(),
    };
    g722::decode_into(decs, raw, mode, &mut planes);
    match mode {
        ChannelMode::Mono => emit_mono_block(rate, &planes[0], on_block)?,
        ChannelMode::Split => emit_split_block(rate, pcm_this, &planes, on_block)?,
    }
    Ok(pcm_this)
}
