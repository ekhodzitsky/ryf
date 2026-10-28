//! Exact-size PCM collect (hot fill paths).

use super::{DecodePlan, pull_decode};
use crate::ChannelMode;
use crate::convert::{g711_table, split_s16_le_to_f32};
use crate::error::Result;
use crate::header::SampleCodec;
use crate::source::ByteSource;

/// Collect full planar output with exact-size allocation (no pull callback tax).
///
/// Hot paths (mono s16 / mono f32 / mix s16) write straight into one `Vec` and
/// size scratch to `min(256KiB, file)` so short clips no longer allocate ~768 KiB.
pub(crate) fn decode_collect(mss: &mut ByteSource<'_>, plan: &DecodePlan) -> Result<Vec<Vec<f32>>> {
    if plan.fmt.codec.is_adpcm() {
        return super::adpcm::collect_adpcm(mss, plan);
    }
    if let Some(planes) = super::special::collect_framed(mss, plan)? {
        return Ok(planes);
    }

    let total = plan.total_frames;
    let max_samples = plan.max_samples;
    let sample_rate = plan.sample_rate;
    let be = plan.fmt.big_endian;
    let codec = plan.fmt.codec;
    let channels = plan.fmt.channels;
    let frame_bytes = plan.frame_bytes;
    let sample_width = plan.fmt.sample_width;

    if !be {
        match (plan.mode, codec, channels, sample_width) {
            (ChannelMode::Mono, SampleCodec::S16, _, 2)
            | (ChannelMode::Mono, SampleCodec::F32, 1, 4)
            | (ChannelMode::Mono, SampleCodec::ALaw | SampleCodec::MuLaw, _, 1) => {
                return collect_mono(mss, plan);
            }
            (ChannelMode::Split, SampleCodec::S16, n, 2) => {
                return fill_split_s16(mss, total, frame_bytes, n, max_samples, sample_rate);
            }
            (ChannelMode::Split, SampleCodec::ALaw | SampleCodec::MuLaw, n, 1) => {
                let table = g711_table(matches!(codec, SampleCodec::ALaw));
                return super::g711::fill_split(mss, total, n, max_samples, sample_rate, table);
            }
            _ => {}
        }
    }

    collect_blocks(mss, plan)
}

/// Fill final planes through bounded codec blocks; no full intermediate payload.
pub(super) fn collect_blocks(mss: &mut ByteSource<'_>, plan: &DecodePlan) -> Result<Vec<Vec<f32>>> {
    let n_out = if plan.mode == ChannelMode::Mono {
        1
    } else {
        plan.fmt.channels
    };
    let mut channels_out: Vec<Vec<f32>> =
        (0..n_out).map(|_| vec![0.0; plan.total_frames]).collect();
    let mut offset = 0usize;
    pull_decode(mss, plan, |block| {
        for (dst, src) in channels_out.iter_mut().zip(block.planar.iter()) {
            dst[offset..offset + block.frames].copy_from_slice(src);
        }
        offset += block.frames;
        Ok(())
    })?;
    for channel in &mut channels_out {
        channel.truncate(offset);
    }
    Ok(channels_out)
}

/// Extend from exact-size iterators: std initializes spare capacity before
/// exposing it as f32, without a separate zero-fill or raw destination slice.
fn collect_mono(mss: &mut ByteSource<'_>, plan: &DecodePlan) -> Result<Vec<Vec<f32>>> {
    let mut out = Vec::with_capacity(plan.total_frames);
    let channels = plan.fmt.channels;
    let scale = crate::convert::I16_SCALE;
    let table = g711_table(plan.fmt.codec == SampleCodec::ALaw);
    super::input::for_each_input(
        mss,
        plan.total_frames,
        plan.frame_bytes,
        plan.max_samples,
        plan.sample_rate,
        |bytes, _| {
            match (plan.fmt.codec, channels) {
                (SampleCodec::S16, 1) => crate::convert::append_s16(bytes, &mut out),
                (SampleCodec::S16, 2) => out.extend(bytes.as_chunks::<4>().0.iter().map(|b| {
                    let left = f32::from(i16::from_le_bytes([b[0], b[1]])) * scale;
                    let right = f32::from(i16::from_le_bytes([b[2], b[3]])) * scale;
                    (left + right) * 0.5
                })),
                (SampleCodec::S16, _) => {
                    out.extend(bytes.chunks_exact(plan.frame_bytes).map(|frame| {
                        let mut sum = 0.0;
                        for b in frame.as_chunks::<2>().0 {
                            sum += f32::from(i16::from_le_bytes(*b)) * scale;
                        }
                        sum / channels as f32
                    }))
                }
                (SampleCodec::F32, _) => out.extend(
                    bytes
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .map(|b| f32::from_le_bytes(*b)),
                ),
                _ => out.extend(bytes.chunks_exact(channels).map(|frame| {
                    let mut sum = 0.0;
                    for b in frame {
                        sum += table[usize::from(*b)];
                    }
                    sum / channels as f32
                })),
            }
            Ok(())
        },
    )?;
    Ok(vec![out])
}
fn fill_split_s16(
    mss: &mut ByteSource<'_>,
    total: usize,
    frame_bytes: usize,
    channels: usize,
    max: usize,
    rate: u32,
) -> Result<Vec<Vec<f32>>> {
    let mut out: Vec<Vec<f32>> = (0..channels).map(|_| vec![0.0; total]).collect();
    super::input::for_each_input(mss, total, frame_bytes, max, rate, |bytes, range| {
        let mut planes: Vec<&mut [f32]> = out.iter_mut().map(|p| &mut p[range.clone()]).collect();
        split_s16_le_to_f32(bytes, &mut planes);
        Ok(())
    })?;
    Ok(out)
}
