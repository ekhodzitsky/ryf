//! Pull-parser PCM decode (O(block) peak RAM for PCM paths).

use crate::ChannelMode;
use crate::error::{FormatKind, Result, WavError};
use crate::header::{FmtFields, SampleCodec, parse_header};
use crate::options::DecodeOptions;
use crate::source::ByteSource;

mod adpcm;
mod g711;
mod g722;
mod g726;
mod gate;
mod gsm;
mod input;
mod pcm;
mod special;
mod stream;

#[cfg(test)]
pub(crate) use adpcm::adpcm_est_frames;
#[cfg(test)]
pub(crate) use adpcm::adpcm_frames_capped;
#[cfg(test)]
pub(crate) use adpcm::decode_adpcm_interleaved;
#[cfg(test)]
pub(crate) use adpcm::ensure_adpcm_enabled;
pub(crate) use gate::{codec_gate, frame_layout};
pub(crate) use pcm::decode_collect;

/// Max frames per pull block (~256 KiB of source PCM).
#[inline]
pub(crate) fn block_frames_for(frame_bytes: usize) -> usize {
    ((1 << 18) / frame_bytes.max(1)).max(1)
}

/// Scratch size: never allocate a full 256 KiB block for a short file.
#[inline]
pub(crate) fn scratch_frames(frame_bytes: usize, total_frames: usize) -> usize {
    block_frames_for(frame_bytes).min(total_frames.max(1))
}

#[inline]
pub(crate) fn check_duration(
    decoded_frames: usize,
    max_samples: usize,
    sample_rate: u32,
) -> Result<()> {
    if decoded_frames > max_samples {
        let rate = sample_rate.max(1) as f64;
        let observed_s = decoded_frames as f64 / rate;
        let max_secs = max_samples as f64 / rate;
        return Err(WavError::too_long(observed_s, max_secs));
    }
    Ok(())
}

fn reject_too_many_frames(
    frames: u64,
    sample_rate: u32,
    max_samples: usize,
    max_secs: f64,
) -> Result<()> {
    if frames > max_samples as u64 {
        let observed_s = frames as f64 / sample_rate.max(1) as f64;
        return Err(WavError::too_long(observed_s, max_secs));
    }
    Ok(())
}

fn reject_output_too_large(n_out: usize, frames: usize, max_bytes: u64) -> Result<()> {
    let bytes = (n_out as u64)
        .saturating_mul(frames as u64)
        .saturating_mul(4);
    if bytes > max_bytes {
        return Err(WavError::output_too_large(bytes, max_bytes));
    }
    Ok(())
}

/// Metadata returned by [`decode_streaming`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct StreamInfo {
    pub sample_rate: u32,
    /// 1 if mixed ([`ChannelMode::Mono`]); otherwise the source channel count.
    pub channels: usize,
    /// Total frames delivered across all blocks.
    pub frames: usize,
}

/// One pull-parser block of planar f32 PCM (no ownership transfer).
///
/// - [`ChannelMode::Mono`]: `planar.len() == 1` (mixed if source is multi-ch).
/// - [`ChannelMode::Split`]: `planar.len() == n_channels`, equal lengths.
///
/// Slices are valid only for the duration of the callback.
#[derive(Debug)]
#[non_exhaustive]
pub struct StreamBlock<'a> {
    pub sample_rate: u32,
    pub frames: usize,
    pub planar: &'a [&'a [f32]],
}

/// Validated decode plan; stream is positioned at the first data byte.
pub(crate) struct DecodePlan {
    pub(crate) sample_rate: u32,
    pub(crate) frame_bytes: usize,
    pub(crate) total_frames: usize,
    pub(crate) max_samples: usize,
    pub(crate) mode: ChannelMode,
    #[cfg_attr(not(feature = "adpcm"), allow(dead_code))]
    pub(crate) data_len: u64,
    /// ADPCM block parameters, and G.726 width plus packing.
    #[cfg_attr(not(feature = "adpcm"), allow(dead_code))]
    pub(crate) fmt: FmtFields,
}

pub(crate) fn open_decode(mss: &mut ByteSource<'_>, opts: &DecodeOptions) -> Result<DecodePlan> {
    let mode = opts.channel_mode;
    let max_secs = opts.max_duration_secs;
    let mut header = parse_header(mss)?;
    let sample_rate = gate::codec_gate(&mut header.fmt, opts)?;

    let channels = header.fmt.channels;
    let max_samples = opts.max_frames(sample_rate);

    let stream_len = mss.byte_len().ok_or(WavError::StreamLengthUnknown)?;
    let data_len = header
        .data_len(Some(stream_len))
        .ok_or(WavError::StreamLengthUnknown)?;
    let (frame_bytes, count) =
        gate::frame_layout(&header.fmt, Some(data_len), header.declared_sample_count)?;
    let count = count.ok_or_else(|| WavError::format(FormatKind::InvalidSize))?;
    reject_too_many_frames(count, sample_rate, max_samples, max_secs)?;
    let total_frames =
        usize::try_from(count).map_err(|_| WavError::format(FormatKind::InvalidSize))?;

    let n_out = match mode {
        ChannelMode::Mono => 1,
        ChannelMode::Split => channels,
    };
    reject_output_too_large(n_out, total_frames, opts.max_output_bytes)?;

    Ok(DecodePlan {
        sample_rate,
        frame_bytes,
        total_frames,
        max_samples,
        mode,
        data_len,
        fmt: header.fmt,
    })
}

/// Pull-parse WAVE PCM into O(block) scratch and invoke `on_block` per block.
///
/// Peak RAM is ~256 KiB of source bytes + one planar f32 block (not the
/// full decoded file). ADPCM walks one compressed block at a time.
///
/// Returns total frames delivered. On error, earlier callbacks may have
/// consumed audio and the source cursor may have advanced. Rewind explicitly
/// before retrying a complete decode.
pub fn decode_streaming<F>(
    mss: &mut ByteSource<'_>,
    opts: &DecodeOptions,
    mut on_block: F,
) -> Result<StreamInfo>
where
    F: FnMut(StreamBlock<'_>) -> Result<()>,
{
    let plan = open_decode(mss, opts)?;
    let sample_rate = plan.sample_rate;
    let channels = match plan.mode {
        ChannelMode::Mono => 1,
        ChannelMode::Split => plan.fmt.channels,
    };
    let frames = pull_decode(mss, &plan, &mut on_block)?;
    Ok(StreamInfo {
        sample_rate,
        channels,
        frames,
    })
}

/// Shared pull loop used by [`decode_streaming`] and generic collect.
pub(crate) fn pull_decode<F>(
    mss: &mut ByteSource<'_>,
    plan: &DecodePlan,
    mut on_block: F,
) -> Result<usize>
where
    F: FnMut(StreamBlock<'_>) -> Result<()>,
{
    let sample_rate = plan.sample_rate;
    let mode = plan.mode;

    if plan.fmt.codec.is_adpcm() {
        return adpcm::pull_adpcm(mss, plan, &mut on_block);
    }
    if let Some(n) = special::pull_framed(mss, plan, &mut on_block)? {
        return Ok(n);
    }

    let frame_bytes = plan.frame_bytes;
    let total_frames = plan.total_frames;
    let max_samples = plan.max_samples;
    let channels = plan.fmt.channels;
    let sample_width = plan.fmt.sample_width;
    let be = plan.fmt.big_endian;
    let codec = plan.fmt.codec;

    if be {
        return stream::pull_generic(mss, plan, &mut on_block);
    }

    match (mode, codec, channels, sample_width) {
        (ChannelMode::Mono, SampleCodec::S16, 1, 2) => {
            stream::pull_mono_s16(mss, total_frames, max_samples, sample_rate, &mut on_block)
        }
        (ChannelMode::Mono, SampleCodec::S16, n, 2) if n > 1 => stream::pull_mix_s16(
            mss,
            total_frames,
            frame_bytes,
            n,
            max_samples,
            sample_rate,
            &mut on_block,
        ),
        (ChannelMode::Mono, SampleCodec::F32, 1, 4) => {
            stream::pull_mono_f32(mss, total_frames, max_samples, sample_rate, &mut on_block)
        }
        (ChannelMode::Split, SampleCodec::S16, n, 2) => stream::pull_split_s16(
            mss,
            total_frames,
            frame_bytes,
            n,
            max_samples,
            sample_rate,
            &mut on_block,
        ),
        _ => stream::pull_generic(mss, plan, &mut on_block),
    }
}

fn emit_mono_block<F>(sample_rate: u32, mono: &[f32], on_block: &mut F) -> Result<()>
where
    F: FnMut(StreamBlock<'_>) -> Result<()>,
{
    let planar: [&[f32]; 1] = [mono];
    on_block(StreamBlock {
        sample_rate,
        frames: mono.len(),
        planar: &planar,
    })
}

fn emit_split_block<F>(
    sample_rate: u32,
    frames_this: usize,
    planar: &[Vec<f32>],
    on_block: &mut F,
) -> Result<()>
where
    F: FnMut(StreamBlock<'_>) -> Result<()>,
{
    let channels = planar.len();
    if channels <= 8 {
        let mut slots: [&[f32]; 8] = [&[]; 8];
        for c in 0..channels {
            slots[c] = &planar[c][..frames_this];
        }
        on_block(StreamBlock {
            sample_rate,
            frames: frames_this,
            planar: &slots[..channels],
        })
    } else {
        let refs: Vec<&[f32]> = planar.iter().map(|ch| &ch[..frames_this]).collect();
        on_block(StreamBlock {
            sample_rate,
            frames: frames_this,
            planar: &refs,
        })
    }
}

#[cfg(test)]
#[path = "../pull_tests.rs"]
mod pull_tests;
