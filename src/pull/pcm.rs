//! Exact-size PCM collect (hot fill paths).

use std::io::Seek;

use super::{DecodePlan, check_duration, pcm_short, pull_decode, scratch_frames, uninit_f32_vec};
use crate::ChannelMode;
use crate::convert::{
    convert_f32_mono, convert_s16_mono, g711_table, mix_s16_le_to_f32, split_s16_le_to_f32,
};
use crate::error::{Result, WavError};
use crate::header::SampleCodec;
use crate::scrub::scrub_vec;
use crate::source::ByteSource;

/// Collect full planar output with exact-size allocation (no pull callback tax).
///
/// Hot paths (mono s16 / mono f32 / mix s16) write straight into one `Vec` and
/// size scratch to `min(256KiB, file)` so short clips no longer allocate ~768 KiB.
pub(crate) fn decode_collect(mss: &mut ByteSource<'_>, plan: &DecodePlan) -> Result<Vec<Vec<f32>>> {
    if plan.codec.is_adpcm() {
        return super::adpcm::collect_adpcm(mss, plan);
    }
    if plan.codec == SampleCodec::G722 {
        return super::g722::collect_g722(mss, plan);
    }

    let total = plan.total_frames;
    let max_samples = plan.max_samples;
    let sample_rate = plan.sample_rate;
    let be = plan.big_endian;
    let codec = plan.codec;
    let channels = plan.channels;
    let frame_bytes = plan.frame_bytes;
    let sample_width = plan.sample_width;

    if !be {
        match (plan.mode, codec, channels, sample_width) {
            (ChannelMode::Mono, SampleCodec::S16, 1, 2) => {
                // Skip zero-fill: convert writes every sample. `f32` is Copy
                // (no Drop), so partial failure only wastes capacity.
                #[allow(clippy::uninit_vec)]
                let mut out = {
                    let mut v = Vec::with_capacity(total);
                    // SAFETY: fill_mono_s16 writes all `total` elements before return Ok.
                    unsafe {
                        v.set_len(total);
                    }
                    v
                };
                fill_mono_s16(mss, &mut out, max_samples, sample_rate)?;
                return Ok(vec![out]);
            }
            (ChannelMode::Mono, SampleCodec::S16, n, 2) if n > 1 => {
                #[allow(clippy::uninit_vec)]
                let mut out = {
                    let mut v = Vec::with_capacity(total);
                    // SAFETY: fill_mix_s16 writes all `total` elements before return Ok.
                    unsafe {
                        v.set_len(total);
                    }
                    v
                };
                fill_mix_s16(mss, &mut out, frame_bytes, n, max_samples, sample_rate)?;
                return Ok(vec![out]);
            }
            (ChannelMode::Mono, SampleCodec::F32, 1, 4) => {
                #[allow(clippy::uninit_vec)]
                let mut out = {
                    let mut v = Vec::with_capacity(total);
                    // SAFETY: fill_mono_f32 writes all `total` elements before return Ok.
                    unsafe {
                        v.set_len(total);
                    }
                    v
                };
                fill_mono_f32(mss, &mut out, max_samples, sample_rate)?;
                return Ok(vec![out]);
            }
            (ChannelMode::Split, SampleCodec::S16, n, 2) => {
                return fill_split_s16(mss, total, frame_bytes, n, max_samples, sample_rate);
            }
            (ChannelMode::Mono, SampleCodec::ALaw | SampleCodec::MuLaw, 1, 1) => {
                let table = g711_table(matches!(codec, SampleCodec::ALaw));
                #[allow(clippy::uninit_vec)]
                let mut out = {
                    let mut v = Vec::with_capacity(total);
                    // SAFETY: fill_mono writes all `total` elements before Ok.
                    unsafe {
                        v.set_len(total);
                    }
                    v
                };
                super::g711::fill_mono(mss, &mut out, max_samples, sample_rate, table)?;
                return Ok(vec![out]);
            }
            (ChannelMode::Mono, SampleCodec::ALaw | SampleCodec::MuLaw, n, 1) if n > 1 => {
                let table = g711_table(matches!(codec, SampleCodec::ALaw));
                #[allow(clippy::uninit_vec)]
                let mut out = {
                    let mut v = Vec::with_capacity(total);
                    // SAFETY: fill_mix writes all `total` elements before Ok.
                    unsafe {
                        v.set_len(total);
                    }
                    v
                };
                super::g711::fill_mix(mss, &mut out, n, max_samples, sample_rate, table)?;
                return Ok(vec![out]);
            }
            (ChannelMode::Split, SampleCodec::ALaw | SampleCodec::MuLaw, n, 1) => {
                let table = g711_table(matches!(codec, SampleCodec::ALaw));
                return super::g711::fill_split(mss, total, n, max_samples, sample_rate, table);
            }
            _ => {}
        }
    }

    // Generic / RIFX: pull into pre-sized buffers (exact total_frames).
    let n_out = match plan.mode {
        ChannelMode::Mono => 1,
        ChannelMode::Split => channels,
    };
    let mut channels_out: Vec<Vec<f32>> = (0..n_out).map(|_| vec![0.0f32; total]).collect();
    let mut offset = 0usize;
    pull_decode(mss, plan, |block| {
        debug_assert_eq!(block.planar.len(), channels_out.len());
        let n = block.frames;
        for (dst, src) in channels_out.iter_mut().zip(block.planar.iter()) {
            dst[offset..offset + n].copy_from_slice(src);
        }
        offset += n;
        Ok(())
    })?;
    for ch in &mut channels_out {
        ch.truncate(offset);
    }
    Ok(channels_out)
}

fn fill_mono_s16(
    mss: &mut ByteSource<'_>,
    out: &mut [f32],
    max_samples: usize,
    sample_rate: u32,
) -> Result<()> {
    use std::io::SeekFrom;

    let total = out.len();
    if total > max_samples {
        return check_duration(total, max_samples, sample_rate);
    }
    let need = total * 2;
    // Zero-copy: in-memory slice sources convert PCM in place (no raw scratch).
    if let Some(rest) = mss.remaining_slice() {
        if rest.len() < need {
            return Err(pcm_short("wav: short s16 data"));
        }
        convert_s16_mono(&rest[..need], out);
        mss.seek(SeekFrom::Current(need as i64))
            .map_err(WavError::packet_io)?;
        return Ok(());
    }
    let block = scratch_frames(2, total);
    let mut raw = scrub_vec(vec![0u8; block * 2]);
    let mut decoded = 0usize;
    let mut bytes_left = total as u64 * 2;
    while bytes_left >= 2 {
        let frames_this = ((bytes_left / 2) as usize).min(block);
        let want = frames_this * 2;
        mss.read_buf_exact(&mut raw[..want])
            .map_err(WavError::packet_io)?;
        bytes_left -= want as u64;
        convert_s16_mono(&raw[..want], &mut out[decoded..decoded + frames_this]);
        decoded += frames_this;
    }
    debug_assert_eq!(decoded, total);
    Ok(())
}

fn fill_mix_s16(
    mss: &mut ByteSource<'_>,
    out: &mut [f32],
    frame_bytes: usize,
    channels: usize,
    max_samples: usize,
    sample_rate: u32,
) -> Result<()> {
    use std::io::SeekFrom;

    let total = out.len();
    if total > max_samples {
        return check_duration(total, max_samples, sample_rate);
    }
    let need = total * frame_bytes;
    if let Some(rest) = mss.remaining_slice() {
        if rest.len() < need {
            return Err(pcm_short("wav: short s16 mix data"));
        }
        mix_s16_le_to_f32(&rest[..need], out, channels);
        mss.seek(SeekFrom::Current(need as i64))
            .map_err(WavError::packet_io)?;
        return Ok(());
    }
    let block = scratch_frames(frame_bytes, total);
    let mut raw = scrub_vec(vec![0u8; block * frame_bytes]);
    let mut decoded = 0usize;
    let mut bytes_left = total as u64 * frame_bytes as u64;
    while bytes_left >= frame_bytes as u64 {
        let frames_this = ((bytes_left / frame_bytes as u64) as usize).min(block);
        let want = frames_this * frame_bytes;
        mss.read_buf_exact(&mut raw[..want])
            .map_err(WavError::packet_io)?;
        bytes_left -= want as u64;
        mix_s16_le_to_f32(
            &raw[..want],
            &mut out[decoded..decoded + frames_this],
            channels,
        );
        decoded += frames_this;
    }
    Ok(())
}

fn fill_mono_f32(
    mss: &mut ByteSource<'_>,
    out: &mut [f32],
    max_samples: usize,
    sample_rate: u32,
) -> Result<()> {
    use std::io::SeekFrom;

    let total = out.len();
    if total > max_samples {
        return check_duration(total, max_samples, sample_rate);
    }
    let need = total * 4;
    if let Some(rest) = mss.remaining_slice() {
        if rest.len() < need {
            return Err(pcm_short("wav: short f32 data"));
        }
        convert_f32_mono(&rest[..need], out);
        mss.seek(SeekFrom::Current(need as i64))
            .map_err(WavError::packet_io)?;
        return Ok(());
    }
    let block = scratch_frames(4, total);
    let mut raw = scrub_vec(vec![0u8; block * 4]);
    let mut decoded = 0usize;
    let mut bytes_left = total as u64 * 4;
    while bytes_left >= 4 {
        let frames_this = ((bytes_left / 4) as usize).min(block);
        let want = frames_this * 4;
        mss.read_buf_exact(&mut raw[..want])
            .map_err(WavError::packet_io)?;
        bytes_left -= want as u64;
        convert_f32_mono(&raw[..want], &mut out[decoded..decoded + frames_this]);
        decoded += frames_this;
    }
    Ok(())
}

fn fill_split_s16(
    mss: &mut ByteSource<'_>,
    total_frames: usize,
    frame_bytes: usize,
    channels: usize,
    max_samples: usize,
    sample_rate: u32,
) -> Result<Vec<Vec<f32>>> {
    use std::io::SeekFrom;

    if total_frames > max_samples {
        check_duration(total_frames, max_samples, sample_rate)?;
    }
    let mut out: Vec<Vec<f32>> = (0..channels)
        .map(|_| uninit_f32_vec(total_frames))
        .collect();
    let need = total_frames * frame_bytes;
    if let Some(rest) = mss.remaining_slice() {
        if rest.len() < need {
            return Err(pcm_short("wav: short s16 split data"));
        }
        {
            let mut planes: Vec<&mut [f32]> = out.iter_mut().map(|c| c.as_mut_slice()).collect();
            split_s16_le_to_f32(&rest[..need], &mut planes);
        }
        mss.seek(SeekFrom::Current(need as i64))
            .map_err(WavError::packet_io)?;
        return Ok(out);
    }
    let block = scratch_frames(frame_bytes, total_frames);
    let mut raw = scrub_vec(vec![0u8; block * frame_bytes]);
    let mut decoded = 0usize;
    let mut bytes_left = total_frames as u64 * frame_bytes as u64;
    while bytes_left >= frame_bytes as u64 {
        let frames_this = ((bytes_left / frame_bytes as u64) as usize).min(block);
        let want = frames_this * frame_bytes;
        mss.read_buf_exact(&mut raw[..want])
            .map_err(WavError::packet_io)?;
        bytes_left -= want as u64;
        {
            let mut planes: Vec<&mut [f32]> = out
                .iter_mut()
                .map(|c| &mut c[decoded..decoded + frames_this])
                .collect();
            split_s16_le_to_f32(&raw[..want], &mut planes);
        }
        decoded += frames_this;
    }
    Ok(out)
}
