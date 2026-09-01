//! Pull-parser PCM decode (O(block) peak RAM for PCM paths).

use crate::ChannelMode;
use crate::error::{Result, WavError};
use crate::header::{FmtFields, SampleCodec, parse_header};
use crate::options::DecodeOptions;
use crate::source::ByteSource;

mod adpcm;
mod pcm;
mod stream;

#[cfg(test)]
pub(crate) use adpcm::decode_adpcm_interleaved;
pub(crate) use adpcm::ensure_adpcm_enabled;
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

/// Defense-in-depth: `open_decode` already rejects over-budget plans, so
/// this is false on the success path.
#[inline]
fn need_duration_check(total_frames: usize, max_samples: usize) -> bool {
    total_frames > max_samples
}

/// Metadata returned by [`decode_streaming`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
pub struct StreamBlock<'a> {
    pub sample_rate: u32,
    pub frames: usize,
    pub planar: &'a [&'a [f32]],
}

/// Validated decode plan; stream is positioned at the first data byte.
pub(crate) struct DecodePlan {
    pub(crate) sample_rate: u32,
    pub(crate) channels: usize,
    pub(crate) sample_width: usize,
    pub(crate) frame_bytes: usize,
    pub(crate) total_frames: usize,
    pub(crate) max_samples: usize,
    pub(crate) codec: SampleCodec,
    pub(crate) big_endian: bool,
    pub(crate) mode: ChannelMode,
    #[cfg_attr(not(feature = "adpcm"), allow(dead_code))]
    pub(crate) data_len: u64,
    /// Present only for ADPCM codecs.
    #[cfg_attr(not(feature = "adpcm"), allow(dead_code))]
    pub(crate) fmt: FmtFields,
}

pub(crate) fn open_decode(mss: &mut ByteSource<'_>, opts: &DecodeOptions) -> Result<DecodePlan> {
    let mode = opts.channel_mode;
    let max_secs = opts.max_duration_secs;
    let header = parse_header(mss)?;

    let sample_rate = header.fmt.sample_rate;
    if sample_rate == 0 || sample_rate > opts.max_sample_rate {
        return Err(WavError::sample_rate(sample_rate, opts.max_sample_rate));
    }
    if matches!(header.fmt.codec, SampleCodec::Unsupported) {
        return Err(WavError::UnsupportedCodec);
    }

    let channels = header.fmt.channels;
    let sample_width = header.fmt.sample_width;
    let max_samples = opts.max_frames(sample_rate);

    let stream_len = mss.byte_len().ok_or(WavError::StreamLengthUnknown)?;
    let available = stream_len.saturating_sub(header.data_pos);
    let data_len = match header.declared_data_len {
        Some(declared) => declared.min(available),
        None => available,
    };

    let (frame_bytes, total_frames) = if header.fmt.codec.is_adpcm() {
        let (ba, spb) = match (header.fmt.adpcm_ms.as_ref(), header.fmt.adpcm_ima.as_ref()) {
            (Some(p), _) => (p.block_align as u64, p.samples_per_block as u64),
            (_, Some(p)) => (p.block_align as u64, p.samples_per_block as u64),
            _ => (0, 0),
        };
        let est = data_len.checked_div(ba).map(|n| n * spb).unwrap_or(0);
        reject_too_many_frames(est, sample_rate, max_samples, max_secs)?;
        (0, est as usize)
    } else {
        let frame_bytes = sample_width
            .checked_mul(channels)
            .filter(|&n| n > 0)
            .ok_or_else(|| WavError::format("wav: invalid frame size"))?;
        let actual_frames = data_len / frame_bytes as u64;
        // Duration follows *available* PCM. A lying `fact` / ds64 count that
        // is smaller still wins (W64 8-byte pad, RF64 leftovers). A lying
        // *larger* count is ignored — TooLong is about bytes on disk, not
        // a header that claims three hours of a 10 s file.
        let frames = match header.declared_sample_count {
            // `fact` / ds64 sampleCount is samples per channel, not interleaved.
            Some(sc) => actual_frames.min(sc),
            None => actual_frames,
        };
        reject_too_many_frames(frames, sample_rate, max_samples, max_secs)?;
        (frame_bytes, frames as usize)
    };

    let n_out = match mode {
        ChannelMode::Mono => 1,
        ChannelMode::Split => channels,
    };
    reject_output_too_large(n_out, total_frames, opts.max_output_bytes)?;

    Ok(DecodePlan {
        sample_rate,
        channels,
        sample_width,
        frame_bytes,
        total_frames,
        max_samples,
        codec: header.fmt.codec,
        big_endian: header.fmt.big_endian,
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
/// Returns total frames delivered.
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
        ChannelMode::Split => plan.channels,
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

    if plan.codec.is_adpcm() {
        return adpcm::pull_adpcm(mss, plan, &mut on_block);
    }

    let frame_bytes = plan.frame_bytes;
    let total_frames = plan.total_frames;
    let max_samples = plan.max_samples;
    let channels = plan.channels;
    let sample_width = plan.sample_width;
    let be = plan.big_endian;
    let codec = plan.codec;

    if be {
        return stream::pull_generic(
            mss,
            mode,
            codec,
            channels,
            sample_width,
            frame_bytes,
            total_frames,
            max_samples,
            sample_rate,
            true,
            &mut on_block,
        );
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
        _ => stream::pull_generic(
            mss,
            mode,
            codec,
            channels,
            sample_width,
            frame_bytes,
            total_frames,
            max_samples,
            sample_rate,
            false,
            &mut on_block,
        ),
    }
}

fn pcm_short(msg: &'static str) -> WavError {
    WavError::packet_io(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, msg))
}

/// `f32` is `Copy` — dropping a partially-filled buffer is safe.
fn uninit_f32_vec(n: usize) -> Vec<f32> {
    let mut v = Vec::with_capacity(n);
    #[allow(clippy::uninit_vec)]
    // SAFETY: caller writes every element before return Ok; Drop is a no-op.
    unsafe {
        v.set_len(n);
    }
    v
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
