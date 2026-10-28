//! G.722 pull / collect (stateful; 1 encoded byte is 2 PCM samples / ch).

use super::{DecodePlan, check_duration, emit_mono_block, emit_split_block};
use crate::ChannelMode;
use crate::error::Result;
use crate::g722::{self, G722Decoder};
use crate::source::ByteSource;

pub(super) fn collect_g722(mss: &mut ByteSource<'_>, plan: &DecodePlan) -> Result<Vec<Vec<f32>>> {
    check_duration(plan.total_frames, plan.max_samples, plan.sample_rate)?;
    let channels = plan.fmt.channels;
    let count = if plan.mode == ChannelMode::Mono {
        1
    } else {
        channels
    };
    let mut out: Vec<Vec<f32>> = (0..count).map(|_| vec![0.0; plan.total_frames]).collect();
    let mut decs: Vec<G722Decoder> = (0..channels).map(|_| G722Decoder::new()).collect();
    super::input::for_each_input(
        mss,
        plan.total_frames.div_ceil(2),
        channels,
        usize::MAX,
        plan.sample_rate,
        |bytes, range| {
            let start = range.start * 2;
            let end = range.end.saturating_mul(2).min(plan.total_frames);
            let mut planes: Vec<&mut [f32]> = out.iter_mut().map(|p| &mut p[start..end]).collect();
            g722::decode_into(&mut decs, bytes, plan.mode, &mut planes);
            Ok(())
        },
    )?;
    Ok(out)
}
pub(super) fn pull_g722<F>(
    mss: &mut ByteSource<'_>,
    plan: &DecodePlan,
    on_block: &mut F,
) -> Result<usize>
where
    F: FnMut(super::StreamBlock<'_>) -> Result<()>,
{
    check_duration(plan.total_frames, plan.max_samples, plan.sample_rate)?;
    let mut decs: Vec<G722Decoder> = (0..plan.fmt.channels).map(|_| G722Decoder::new()).collect();
    let mut decoded = 0;
    super::input::for_each_input(
        mss,
        plan.total_frames.div_ceil(2),
        plan.fmt.channels,
        usize::MAX,
        plan.sample_rate,
        |raw, range| {
            let count = (range.len() * 2).min(plan.total_frames - decoded);
            decoded += emit_chunk(
                raw,
                plan.fmt.channels,
                count,
                plan.mode,
                plan.sample_rate,
                &mut decs,
                on_block,
            )?;
            Ok(())
        },
    )?;
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
    let mut slices: Vec<&mut [f32]> = planes.iter_mut().map(Vec::as_mut_slice).collect();
    g722::decode_into(decs, raw, mode, &mut slices);
    match mode {
        ChannelMode::Mono => emit_mono_block(rate, &planes[0], on_block)?,
        ChannelMode::Split => emit_split_block(rate, pcm_this, &planes, on_block)?,
    }
    Ok(pcm_this)
}
