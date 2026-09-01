//! Streaming PCM pull loops (s16 / f32 / generic).

use std::io::Seek;

use super::{
    StreamBlock, check_duration, emit_mono_block, emit_split_block, need_duration_check, pcm_short,
    scratch_frames,
};
use crate::ChannelMode;
use crate::convert::{
    convert_f32_mono, convert_s16_mono, convert_sample, mix_s16_le_to_f32, split_s16_le_to_f32,
};
use crate::error::{Result, WavError};
use crate::header::SampleCodec;
use crate::source::ByteSource;

pub(super) fn pull_mono_s16<F>(
    mss: &mut ByteSource<'_>,
    total_frames: usize,
    max_samples: usize,
    sample_rate: u32,
    on_block: &mut F,
) -> Result<usize>
where
    F: FnMut(StreamBlock<'_>) -> Result<()>,
{
    use std::io::SeekFrom;

    let block_frames = scratch_frames(2, total_frames);
    let check = need_duration_check(total_frames, max_samples);
    let mut f32b = vec![0.0f32; block_frames];
    let need = total_frames * 2;

    // Zero-copy: walk PCM from the borrowed slice (no intermediate raw buffer).
    if let Some(rest) = mss.remaining_slice() {
        if rest.len() < need {
            return Err(pcm_short("wav: short s16 data"));
        }
        let pcm = &rest[..need];
        let mut decoded = 0usize;
        while decoded < total_frames {
            let frames_this = (total_frames - decoded).min(block_frames);
            let b0 = decoded * 2;
            let b1 = b0 + frames_this * 2;
            convert_s16_mono(&pcm[b0..b1], &mut f32b[..frames_this]);
            decoded += frames_this;
            if check {
                check_duration(decoded, max_samples, sample_rate)?;
            }
            emit_mono_block(sample_rate, &f32b[..frames_this], on_block)?;
        }
        mss.seek(SeekFrom::Current(need as i64))
            .map_err(WavError::packet_io)?;
        return Ok(decoded);
    }

    let mut raw = vec![0u8; block_frames * 2];
    let mut decoded = 0usize;
    let mut bytes_left = total_frames as u64 * 2;

    while bytes_left >= 2 {
        let frames_this = ((bytes_left / 2) as usize).min(block_frames);
        let want = frames_this * 2;
        mss.read_buf_exact(&mut raw[..want])
            .map_err(WavError::packet_io)?;
        bytes_left -= want as u64;

        convert_s16_mono(&raw[..want], &mut f32b[..frames_this]);
        decoded += frames_this;
        if check {
            check_duration(decoded, max_samples, sample_rate)?;
        }
        emit_mono_block(sample_rate, &f32b[..frames_this], on_block)?;
    }
    Ok(decoded)
}

pub(super) fn pull_mix_s16<F>(
    mss: &mut ByteSource<'_>,
    total_frames: usize,
    frame_bytes: usize,
    channels: usize,
    max_samples: usize,
    sample_rate: u32,
    on_block: &mut F,
) -> Result<usize>
where
    F: FnMut(StreamBlock<'_>) -> Result<()>,
{
    use std::io::SeekFrom;

    let block_frames = scratch_frames(frame_bytes, total_frames);
    let check = need_duration_check(total_frames, max_samples);
    let mut f32b = vec![0.0f32; block_frames];
    let need = total_frames * frame_bytes;

    if let Some(rest) = mss.remaining_slice() {
        if rest.len() < need {
            return Err(pcm_short("wav: short s16 mix data"));
        }
        let pcm = &rest[..need];
        let mut decoded = 0usize;
        while decoded < total_frames {
            let frames_this = (total_frames - decoded).min(block_frames);
            let b0 = decoded * frame_bytes;
            let b1 = b0 + frames_this * frame_bytes;
            mix_s16_le_to_f32(&pcm[b0..b1], &mut f32b[..frames_this], channels);
            decoded += frames_this;
            if check {
                check_duration(decoded, max_samples, sample_rate)?;
            }
            emit_mono_block(sample_rate, &f32b[..frames_this], on_block)?;
        }
        mss.seek(SeekFrom::Current(need as i64))
            .map_err(WavError::packet_io)?;
        return Ok(decoded);
    }

    let mut raw = vec![0u8; block_frames * frame_bytes];
    let mut decoded = 0usize;
    let mut bytes_left = total_frames as u64 * frame_bytes as u64;

    while bytes_left >= frame_bytes as u64 {
        let frames_this = ((bytes_left / frame_bytes as u64) as usize).min(block_frames);
        let want = frames_this * frame_bytes;
        mss.read_buf_exact(&mut raw[..want])
            .map_err(WavError::packet_io)?;
        bytes_left -= want as u64;
        mix_s16_le_to_f32(&raw[..want], &mut f32b[..frames_this], channels);
        decoded += frames_this;
        if check {
            check_duration(decoded, max_samples, sample_rate)?;
        }
        emit_mono_block(sample_rate, &f32b[..frames_this], on_block)?;
    }
    Ok(decoded)
}

pub(super) fn pull_mono_f32<F>(
    mss: &mut ByteSource<'_>,
    total_frames: usize,
    max_samples: usize,
    sample_rate: u32,
    on_block: &mut F,
) -> Result<usize>
where
    F: FnMut(StreamBlock<'_>) -> Result<()>,
{
    use std::io::SeekFrom;

    let block_frames = scratch_frames(4, total_frames);
    let check = need_duration_check(total_frames, max_samples);
    let mut f32b = vec![0.0f32; block_frames];
    let need = total_frames * 4;

    if let Some(rest) = mss.remaining_slice() {
        if rest.len() < need {
            return Err(pcm_short("wav: short f32 data"));
        }
        let pcm = &rest[..need];
        let mut decoded = 0usize;
        while decoded < total_frames {
            let frames_this = (total_frames - decoded).min(block_frames);
            let b0 = decoded * 4;
            let b1 = b0 + frames_this * 4;
            convert_f32_mono(&pcm[b0..b1], &mut f32b[..frames_this]);
            decoded += frames_this;
            if check {
                check_duration(decoded, max_samples, sample_rate)?;
            }
            emit_mono_block(sample_rate, &f32b[..frames_this], on_block)?;
        }
        mss.seek(SeekFrom::Current(need as i64))
            .map_err(WavError::packet_io)?;
        return Ok(decoded);
    }

    let mut raw = vec![0u8; block_frames * 4];
    let mut decoded = 0usize;
    let mut bytes_left = total_frames as u64 * 4;

    while bytes_left >= 4 {
        let frames_this = ((bytes_left / 4) as usize).min(block_frames);
        let want = frames_this * 4;
        mss.read_buf_exact(&mut raw[..want])
            .map_err(WavError::packet_io)?;
        bytes_left -= want as u64;

        convert_f32_mono(&raw[..want], &mut f32b[..frames_this]);
        decoded += frames_this;
        if check {
            check_duration(decoded, max_samples, sample_rate)?;
        }
        emit_mono_block(sample_rate, &f32b[..frames_this], on_block)?;
    }
    Ok(decoded)
}

pub(super) fn pull_split_s16<F>(
    mss: &mut ByteSource<'_>,
    total_frames: usize,
    frame_bytes: usize,
    channels: usize,
    max_samples: usize,
    sample_rate: u32,
    on_block: &mut F,
) -> Result<usize>
where
    F: FnMut(StreamBlock<'_>) -> Result<()>,
{
    use std::io::SeekFrom;

    let block_frames = scratch_frames(frame_bytes, total_frames);
    let check = need_duration_check(total_frames, max_samples);
    let mut planar: Vec<Vec<f32>> = (0..channels).map(|_| vec![0.0f32; block_frames]).collect();
    let need = total_frames * frame_bytes;

    if let Some(rest) = mss.remaining_slice() {
        if rest.len() < need {
            return Err(pcm_short("wav: short s16 split data"));
        }
        let pcm = &rest[..need];
        let mut decoded = 0usize;
        while decoded < total_frames {
            let frames_this = (total_frames - decoded).min(block_frames);
            let b0 = decoded * frame_bytes;
            let b1 = b0 + frames_this * frame_bytes;
            {
                let mut planes: Vec<&mut [f32]> =
                    planar.iter_mut().map(|p| &mut p[..frames_this]).collect();
                split_s16_le_to_f32(&pcm[b0..b1], &mut planes);
            }
            decoded += frames_this;
            if check {
                check_duration(decoded, max_samples, sample_rate)?;
            }
            emit_split_block(sample_rate, frames_this, &planar, on_block)?;
        }
        mss.seek(SeekFrom::Current(need as i64))
            .map_err(WavError::packet_io)?;
        return Ok(decoded);
    }

    let mut raw = vec![0u8; block_frames * frame_bytes];
    let mut decoded = 0usize;
    let mut bytes_left = total_frames as u64 * frame_bytes as u64;

    while bytes_left >= frame_bytes as u64 {
        let frames_this = ((bytes_left / frame_bytes as u64) as usize).min(block_frames);
        let want = frames_this * frame_bytes;
        mss.read_buf_exact(&mut raw[..want])
            .map_err(WavError::packet_io)?;
        bytes_left -= want as u64;
        {
            let mut planes: Vec<&mut [f32]> =
                planar.iter_mut().map(|p| &mut p[..frames_this]).collect();
            split_s16_le_to_f32(&raw[..want], &mut planes);
        }
        decoded += frames_this;
        if check {
            check_duration(decoded, max_samples, sample_rate)?;
        }
        emit_split_block(sample_rate, frames_this, &planar, on_block)?;
    }
    Ok(decoded)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn pull_generic<F>(
    mss: &mut ByteSource<'_>,
    mode: ChannelMode,
    codec: SampleCodec,
    channels: usize,
    sample_width: usize,
    frame_bytes: usize,
    total_frames: usize,
    max_samples: usize,
    sample_rate: u32,
    big_endian: bool,
    on_block: &mut F,
) -> Result<usize>
where
    F: FnMut(StreamBlock<'_>) -> Result<()>,
{
    let block_frames = scratch_frames(frame_bytes, total_frames);
    let check = need_duration_check(total_frames, max_samples);
    let mut raw = vec![0u8; block_frames * frame_bytes];
    let n_out = match mode {
        ChannelMode::Mono => 1,
        ChannelMode::Split => channels,
    };
    let mut planar: Vec<Vec<f32>> = (0..n_out).map(|_| vec![0.0f32; block_frames]).collect();
    let mut decoded = 0usize;
    let mut bytes_left = total_frames as u64 * frame_bytes as u64;

    while bytes_left >= frame_bytes as u64 {
        let frames_this = ((bytes_left / frame_bytes as u64) as usize).min(block_frames);
        let want = frames_this * frame_bytes;
        mss.read_buf_exact(&mut raw[..want])
            .map_err(WavError::packet_io)?;
        bytes_left -= want as u64;

        match mode {
            ChannelMode::Mono => {
                let mono = &mut planar[0];
                if channels == 1 {
                    for (i, frame) in raw[..want].chunks_exact(frame_bytes).enumerate() {
                        mono[i] = convert_sample(codec, frame, big_endian);
                    }
                } else {
                    let n_ch = channels as f32;
                    for (i, frame) in raw[..want].chunks_exact(frame_bytes).enumerate() {
                        let mut sum = 0.0f32;
                        for sample in frame.chunks_exact(sample_width) {
                            sum += convert_sample(codec, sample, big_endian);
                        }
                        mono[i] = sum / n_ch;
                    }
                }
            }
            ChannelMode::Split => {
                for (fi, frame) in raw[..want].chunks_exact(frame_bytes).enumerate() {
                    for (c, sample) in frame.chunks_exact(sample_width).enumerate() {
                        planar[c][fi] = convert_sample(codec, sample, big_endian);
                    }
                }
            }
        }

        decoded += frames_this;
        if check {
            check_duration(decoded, max_samples, sample_rate)?;
        }
        if n_out <= 8 {
            let mut slots: [&[f32]; 8] = [&[]; 8];
            for c in 0..n_out {
                slots[c] = &planar[c][..frames_this];
            }
            on_block(StreamBlock {
                sample_rate,
                frames: frames_this,
                planar: &slots[..n_out],
            })?;
        } else {
            let refs: Vec<&[f32]> = planar.iter().map(|ch| &ch[..frames_this]).collect();
            on_block(StreamBlock {
                sample_rate,
                frames: frames_this,
                planar: &refs,
            })?;
        }
    }
    Ok(decoded)
}
