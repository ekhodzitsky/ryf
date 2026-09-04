//! Exact-size G.711 collect (LUT kernels).

use super::{check_duration, pcm_short, scratch_frames, uninit_f32_vec};
use crate::convert::{convert_g711_mono, mix_g711, split_g711};
use crate::error::{Result, WavError};
use crate::scrub::scrub_vec;
use crate::source::ByteSource;

pub(super) fn fill_mono(
    mss: &mut ByteSource<'_>,
    out: &mut [f32],
    max_samples: usize,
    sample_rate: u32,
    table: &'static [f32; 256],
) -> Result<()> {
    let total = out.len();
    if total > max_samples {
        return check_duration(total, max_samples, sample_rate);
    }
    if let Some(rest) = mss.remaining_slice() {
        if rest.len() < total {
            return Err(pcm_short("wav: short g711 data"));
        }
        convert_g711_mono(&rest[..total], out, table);
        mss.advance(total as u64).map_err(WavError::packet_io)?;
        return Ok(());
    }
    let block = scratch_frames(1, total);
    let mut raw = scrub_vec(vec![0u8; block]);
    let mut decoded = 0usize;
    let mut bytes_left = total as u64;
    while bytes_left >= 1 {
        let frames_this = (bytes_left as usize).min(block);
        mss.read_buf_exact(&mut raw[..frames_this])
            .map_err(WavError::packet_io)?;
        bytes_left -= frames_this as u64;
        convert_g711_mono(
            &raw[..frames_this],
            &mut out[decoded..decoded + frames_this],
            table,
        );
        decoded += frames_this;
    }
    Ok(())
}

pub(super) fn fill_mix(
    mss: &mut ByteSource<'_>,
    out: &mut [f32],
    channels: usize,
    max_samples: usize,
    sample_rate: u32,
    table: &'static [f32; 256],
) -> Result<()> {
    let total = out.len();
    if total > max_samples {
        return check_duration(total, max_samples, sample_rate);
    }
    let need = total * channels;
    if let Some(rest) = mss.remaining_slice() {
        if rest.len() < need {
            return Err(pcm_short("wav: short g711 mix data"));
        }
        mix_g711(&rest[..need], out, channels, table);
        mss.advance(need as u64).map_err(WavError::packet_io)?;
        return Ok(());
    }
    let block = scratch_frames(channels, total);
    let mut raw = scrub_vec(vec![0u8; block * channels]);
    let mut decoded = 0usize;
    let mut bytes_left = need as u64;
    while bytes_left >= channels as u64 {
        let frames_this = ((bytes_left / channels as u64) as usize).min(block);
        let want = frames_this * channels;
        mss.read_buf_exact(&mut raw[..want])
            .map_err(WavError::packet_io)?;
        bytes_left -= want as u64;
        mix_g711(
            &raw[..want],
            &mut out[decoded..decoded + frames_this],
            channels,
            table,
        );
        decoded += frames_this;
    }
    Ok(())
}

pub(super) fn fill_split(
    mss: &mut ByteSource<'_>,
    total_frames: usize,
    channels: usize,
    max_samples: usize,
    sample_rate: u32,
    table: &'static [f32; 256],
) -> Result<Vec<Vec<f32>>> {
    if total_frames > max_samples {
        check_duration(total_frames, max_samples, sample_rate)?;
    }
    let mut out: Vec<Vec<f32>> = (0..channels)
        .map(|_| uninit_f32_vec(total_frames))
        .collect();
    let need = total_frames * channels;
    if let Some(rest) = mss.remaining_slice() {
        if rest.len() < need {
            return Err(pcm_short("wav: short g711 split data"));
        }
        {
            let mut planes: Vec<&mut [f32]> = out.iter_mut().map(|c| c.as_mut_slice()).collect();
            split_g711(&rest[..need], &mut planes, table);
        }
        mss.advance(need as u64).map_err(WavError::packet_io)?;
        return Ok(out);
    }
    let block = scratch_frames(channels, total_frames);
    let mut raw = scrub_vec(vec![0u8; block * channels]);
    let mut decoded = 0usize;
    let mut bytes_left = need as u64;
    while bytes_left >= channels as u64 {
        let frames_this = ((bytes_left / channels as u64) as usize).min(block);
        let want = frames_this * channels;
        mss.read_buf_exact(&mut raw[..want])
            .map_err(WavError::packet_io)?;
        bytes_left -= want as u64;
        {
            let mut planes: Vec<&mut [f32]> = out
                .iter_mut()
                .map(|c| &mut c[decoded..decoded + frames_this])
                .collect();
            split_g711(&raw[..want], &mut planes, table);
        }
        decoded += frames_this;
    }
    Ok(out)
}
