//! IMA/DVI ADPCM (0x0011) block decoder.

use super::ImaAdpcmParams;
use crate::error::{FormatKind, Result, WavError};
use crate::source::ByteSource;

#[cfg(test)]
use super::{ScrubVec, scrub_vec};

// IMA step / index tables (public domain).
const IMA_STEP: [i16; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66,
    73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449,
    494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272,
    2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630, 9493,
    10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794, 32767,
];

const IMA_INDEX: [i8; 16] = [-1, -1, -1, -1, 2, 4, 6, 8, -1, -1, -1, -1, 2, 4, 6, 8];

/// Walk IMA/DVI ADPCM blocks; one compressed block of interleaved i16 at a time.
pub(crate) fn for_each_ima_adpcm_block(
    mss: &mut ByteSource<'_>,
    params: &ImaAdpcmParams,
    data_len: u64,
    max_frames: usize,
    sample_rate: u32,
    big_endian: bool,
    mut on_block: impl FnMut(&[i16]) -> Result<()>,
) -> Result<usize> {
    let ch = params.channels;
    if ch == 0 || ch > 2 {
        return Err(WavError::format(FormatKind::ChannelLayout));
    }
    let block = params.block_align as usize;
    if block < 4 * ch {
        return Err(WavError::format(FormatKind::Adpcm));
    }

    let mut remaining = data_len;
    let mut block_buf = vec![0u8; block];
    let mut frames = 0usize;

    while remaining >= block as u64 {
        mss.read_buf_exact(&mut block_buf).map_err(WavError::from)?;
        remaining -= block as u64;

        let decoded = if ch == 1 {
            decode_ima_block_mono(&block_buf, big_endian)?
        } else {
            decode_ima_block_stereo(&block_buf, big_endian)?
        };
        let frames_this = decoded.len() / ch;
        if frames + frames_this > max_frames {
            let observed_s = (frames + frames_this) as f64 / sample_rate.max(1) as f64;
            let max_secs = max_frames as f64 / sample_rate.max(1) as f64;
            return Err(WavError::too_long(observed_s, max_secs));
        }
        on_block(&decoded)?;
        frames += frames_this;
    }

    Ok(frames)
}

/// Decode all IMA/DVI ADPCM blocks into interleaved i16 frames.
#[cfg(test)]
pub(crate) fn decode_ima_adpcm(
    mss: &mut ByteSource<'_>,
    params: &ImaAdpcmParams,
    data_len: u64,
    max_frames: usize,
) -> Result<Vec<i16>> {
    let ch = params.channels.max(1);
    let n_blocks = (data_len as usize) / params.block_align.max(1) as usize;
    let hint = n_blocks
        .saturating_mul(params.samples_per_block as usize)
        .min(max_frames)
        .saturating_mul(ch);
    let mut out: ScrubVec<i16> = scrub_vec(Vec::new());
    out.try_reserve(hint)
        .map_err(|_| WavError::format(FormatKind::Adpcm))?;
    for_each_ima_adpcm_block(mss, params, data_len, max_frames, 16_000, false, |block| {
        out.extend_from_slice(block);
        Ok(())
    })?;
    Ok(std::mem::take(&mut out))
}

fn decode_ima_nibble(nibble: u8, predictor: &mut i32, step_index: &mut i32) -> i16 {
    let step = i32::from(IMA_STEP[(*step_index as usize).min(88)]);
    let mut diff = step >> 3;
    if nibble & 1 != 0 {
        diff += step >> 2;
    }
    if nibble & 2 != 0 {
        diff += step >> 1;
    }
    if nibble & 4 != 0 {
        diff += step;
    }
    if nibble & 8 != 0 {
        *predictor -= diff;
    } else {
        *predictor += diff;
    }
    *predictor = (*predictor).clamp(i16::MIN as i32, i16::MAX as i32);
    *step_index += i32::from(IMA_INDEX[(nibble & 0x0f) as usize]);
    *step_index = (*step_index).clamp(0, 88);
    *predictor as i16
}

pub(crate) fn decode_ima_block_mono(block: &[u8], be: bool) -> Result<Vec<i16>> {
    if block.len() < 4 {
        return Err(WavError::format(FormatKind::Adpcm));
    }
    let mut predictor = i32::from(super::i16_at(block, 0, be));
    let mut step_index = i32::from(block[2]);
    if !(0..=88).contains(&step_index) {
        return Err(WavError::format(FormatKind::Adpcm));
    }

    let mut out = Vec::with_capacity((block.len() - 4) * 2 + 1);
    out.push(predictor as i16);

    for &byte in &block[4..] {
        // Low nibble first for IMA WAV.
        out.push(decode_ima_nibble(
            byte & 0x0f,
            &mut predictor,
            &mut step_index,
        ));
        out.push(decode_ima_nibble(
            (byte >> 4) & 0x0f,
            &mut predictor,
            &mut step_index,
        ));
    }
    Ok(out)
}

pub(crate) fn decode_ima_block_stereo(block: &[u8], be: bool) -> Result<Vec<i16>> {
    if block.len() < 8 {
        return Err(WavError::format(FormatKind::Adpcm));
    }
    let mut pred = [
        i32::from(super::i16_at(block, 0, be)),
        i32::from(super::i16_at(block, 4, be)),
    ];
    let mut step_idx = [i32::from(block[2]), i32::from(block[6])];
    for &s in &step_idx {
        if !(0..=88).contains(&s) {
            return Err(WavError::format(FormatKind::Adpcm));
        }
    }

    let mut out = Vec::with_capacity((block.len() - 8) * 2 + 2);
    out.push(pred[0] as i16);
    out.push(pred[1] as i16);

    // Remaining: groups of 4 bytes left + 4 bytes right.
    let data = &block[8..];
    let mut i = 0;
    while i + 8 <= data.len() {
        let left_bytes = &data[i..i + 4];
        let right_bytes = &data[i + 4..i + 8];
        let mut left_s = [0i16; 8];
        let mut right_s = [0i16; 8];
        for (bi, &byte) in left_bytes.iter().enumerate() {
            left_s[bi * 2] = decode_ima_nibble(byte & 0x0f, &mut pred[0], &mut step_idx[0]);
            left_s[bi * 2 + 1] =
                decode_ima_nibble((byte >> 4) & 0x0f, &mut pred[0], &mut step_idx[0]);
        }
        for (bi, &byte) in right_bytes.iter().enumerate() {
            right_s[bi * 2] = decode_ima_nibble(byte & 0x0f, &mut pred[1], &mut step_idx[1]);
            right_s[bi * 2 + 1] =
                decode_ima_nibble((byte >> 4) & 0x0f, &mut pred[1], &mut step_idx[1]);
        }
        for s in 0..8 {
            out.push(left_s[s]);
            out.push(right_s[s]);
        }
        i += 8;
    }
    Ok(out)
}
