//! Bounded callback conversion over shared source windows.
use super::input::for_each_input;
use super::{StreamBlock, emit_mono_block, emit_split_block, scratch_frames};
use crate::ChannelMode;
use crate::Result;
use crate::convert::{
    convert_f32_mono, convert_g711_mono, convert_s16_mono, convert_sample, g711_table, mix_g711,
    mix_s16_le_to_f32, split_g711, split_s16_le_to_f32,
};
use crate::header::SampleCodec;
use crate::source::ByteSource;
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};
pub(super) fn pull_mono_s16<F>(
    mss: &mut ByteSource<'_>,
    total: usize,
    max: usize,
    rate: u32,
    on_block: &mut F,
) -> Result<usize>
where
    F: FnMut(StreamBlock<'_>) -> Result<()>,
{
    let mut out = vec![0.0; scratch_frames(2, total)];
    for_each_input(mss, total, 2, max, rate, |bytes, range| {
        let n = range.len();
        convert_s16_mono(bytes, &mut out[..n]);
        emit_mono_block(rate, &out[..n], on_block)
    })
}
pub(super) fn pull_mono_f32<F>(
    mss: &mut ByteSource<'_>,
    total: usize,
    max: usize,
    rate: u32,
    on_block: &mut F,
) -> Result<usize>
where
    F: FnMut(StreamBlock<'_>) -> Result<()>,
{
    #[cfg(test)]
    pull_mono_f32_hits().fetch_add(1, Ordering::Relaxed);
    let mut out = vec![0.0; scratch_frames(4, total)];
    for_each_input(mss, total, 4, max, rate, |bytes, range| {
        let n = range.len();
        convert_f32_mono(bytes, &mut out[..n]);
        emit_mono_block(rate, &out[..n], on_block)
    })
}
#[cfg(test)]
pub(super) fn pull_mono_f32_hits() -> &'static AtomicUsize {
    static HITS: AtomicUsize = AtomicUsize::new(0);
    &HITS
}
pub(super) fn pull_mix_s16<F>(
    mss: &mut ByteSource<'_>,
    total: usize,
    frame_bytes: usize,
    channels: usize,
    max: usize,
    rate: u32,
    on_block: &mut F,
) -> Result<usize>
where
    F: FnMut(StreamBlock<'_>) -> Result<()>,
{
    let mut out = vec![0.0; scratch_frames(frame_bytes, total)];
    for_each_input(mss, total, frame_bytes, max, rate, |bytes, range| {
        let n = range.len();
        mix_s16_le_to_f32(bytes, &mut out[..n], channels);
        emit_mono_block(rate, &out[..n], on_block)
    })
}
pub(super) fn pull_split_s16<F>(
    mss: &mut ByteSource<'_>,
    total: usize,
    frame_bytes: usize,
    channels: usize,
    max: usize,
    rate: u32,
    on_block: &mut F,
) -> Result<usize>
where
    F: FnMut(StreamBlock<'_>) -> Result<()>,
{
    let mut out: Vec<Vec<f32>> = (0..channels)
        .map(|_| vec![0.0; scratch_frames(frame_bytes, total)])
        .collect();
    for_each_input(mss, total, frame_bytes, max, rate, |bytes, range| {
        let n = range.len();
        let mut planes: Vec<&mut [f32]> = out.iter_mut().map(|p| &mut p[..n]).collect();
        split_s16_le_to_f32(bytes, &mut planes);
        emit_split_block(rate, n, &out, on_block)
    })
}
pub(super) fn pull_generic<F>(
    mss: &mut ByteSource<'_>,
    plan: &super::DecodePlan,
    on_block: &mut F,
) -> Result<usize>
where
    F: FnMut(StreamBlock<'_>) -> Result<()>,
{
    let mode = plan.mode;
    let codec = plan.fmt.codec;
    let channels = plan.fmt.channels;
    let sample_width = plan.fmt.sample_width;
    let frame_bytes = plan.frame_bytes;
    let total_frames = plan.total_frames;
    let max_samples = plan.max_samples;
    let sample_rate = plan.sample_rate;
    let big_endian = plan.fmt.big_endian;
    let n_out = if mode == ChannelMode::Mono {
        1
    } else {
        channels
    };
    let mut planar: Vec<Vec<f32>> = (0..n_out)
        .map(|_| vec![0.0; scratch_frames(frame_bytes, total_frames)])
        .collect();
    for_each_input(
        mss,
        total_frames,
        frame_bytes,
        max_samples,
        sample_rate,
        |bytes, range| {
            let frames_this = range.len();
            if let Some(table) = g711_fast(codec, sample_width, big_endian) {
                match mode {
                    ChannelMode::Mono if channels == 1 => {
                        convert_g711_mono(bytes, &mut planar[0][..frames_this], table);
                    }
                    ChannelMode::Mono => {
                        mix_g711(bytes, &mut planar[0][..frames_this], channels, table);
                    }
                    ChannelMode::Split => {
                        let mut planes: Vec<&mut [f32]> =
                            planar.iter_mut().map(|p| &mut p[..frames_this]).collect();
                        split_g711(bytes, &mut planes, table);
                    }
                }
            } else {
                match mode {
                    ChannelMode::Mono => {
                        let mono = &mut planar[0];
                        if channels == 1 {
                            for (i, frame) in bytes.chunks_exact(frame_bytes).enumerate() {
                                mono[i] = convert_sample(codec, frame, big_endian);
                            }
                        } else {
                            let n_ch = channels as f32;
                            for (i, frame) in bytes.chunks_exact(frame_bytes).enumerate() {
                                let mut sum = 0.0f32;
                                for sample in frame.chunks_exact(sample_width) {
                                    sum += convert_sample(codec, sample, big_endian);
                                }
                                mono[i] = sum / n_ch;
                            }
                        }
                    }
                    ChannelMode::Split => {
                        for (fi, frame) in bytes.chunks_exact(frame_bytes).enumerate() {
                            for (c, sample) in frame.chunks_exact(sample_width).enumerate() {
                                planar[c][fi] = convert_sample(codec, sample, big_endian);
                            }
                        }
                    }
                }
            }

            emit_split_block(sample_rate, frames_this, &planar, on_block)
        },
    )
}
fn g711_fast(
    codec: SampleCodec,
    sample_width: usize,
    big_endian: bool,
) -> Option<&'static [f32; 256]> {
    if big_endian || sample_width != 1 {
        return None;
    }
    match codec {
        SampleCodec::ALaw => Some(g711_table(true)),
        SampleCodec::MuLaw => Some(g711_table(false)),
        _ => None,
    }
}
