//! Microsoft ADPCM (0x0002) and IMA/DVI ADPCM (0x0011) decoders for WAV.
//!
//! Algorithms follow the public Microsoft / IMA specifications (same tables
//! used by libsndfile and dr_wav). Output is interleaved i16 PCM frames;
//! conversion to f32 is done by [`i16_frames_to_f32`]. Duration / size caps
//! are enforced by the parent WAV decoder.

use crate::ChannelMode;
use crate::error::{FormatKind, Result, WavError};
use crate::source::ByteSource;

#[cfg(test)]
type ScrubVec<T> = Vec<T>;
#[cfg(test)]
fn scrub_vec<T>(v: Vec<T>) -> ScrubVec<T> {
    v
}

/// MS-ADPCM coefficient pairs stored in the `fmt ` extension (up to 7).
#[derive(Debug, Clone)]
pub(crate) struct MsAdpcmParams {
    pub block_align: u16,
    pub samples_per_block: u16,
    pub channels: usize,
    /// `(coeff1, coeff2)` pairs; length is `num_coefs` (1..=7).
    pub coefs: Vec<(i16, i16)>,
}

/// IMA/DVI ADPCM parameters from the `fmt ` extension.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ImaAdpcmParams {
    pub block_align: u16,
    pub samples_per_block: u16,
    pub channels: usize,
}

// Microsoft ADPCM adaptation table (public domain / mmreg).
const MS_ADAPTATION: [i16; 16] = [
    230, 230, 230, 230, 307, 409, 512, 614, 768, 614, 512, 409, 307, 230, 230, 230,
];

// Default coefficients when a file ships an empty set (rare).
const MS_DEFAULT_COEFS: [(i16, i16); 7] = [
    (256, 0),
    (512, -256),
    (0, 0),
    (192, 64),
    (240, 0),
    (460, -208),
    (392, -232),
];

#[inline]
pub(super) fn clamp_i16(v: i32) -> i16 {
    v.clamp(i16::MIN as i32, i16::MAX as i32) as i16
}

/// Walk MS-ADPCM blocks and invoke `on_block` with interleaved i16 (one
/// compressed block at a time - no full-stream i16 allocation).
pub(crate) fn for_each_ms_adpcm_block(
    mss: &mut ByteSource<'_>,
    params: &MsAdpcmParams,
    data_len: u64,
    max_frames: usize,
    sample_rate: u32,
    mut on_block: impl FnMut(&[i16]) -> Result<()>,
) -> Result<usize> {
    let ch = params.channels;
    if ch == 0 || ch > 2 {
        return Err(WavError::format(FormatKind::ChannelLayout));
    }
    let block = params.block_align as usize;
    if block < 7 * ch {
        return Err(WavError::format(FormatKind::Adpcm));
    }
    let coefs = if params.coefs.is_empty() {
        MS_DEFAULT_COEFS.as_slice()
    } else {
        params.coefs.as_slice()
    };

    let mut remaining = data_len;
    let mut block_buf = vec![0u8; block];
    let mut frames = 0usize;

    while remaining >= block as u64 {
        mss.read_buf_exact(&mut block_buf).map_err(WavError::from)?;
        remaining -= block as u64;

        let decoded = if ch == 1 {
            decode_ms_block_mono(&block_buf, coefs)?
        } else {
            decode_ms_block_stereo(&block_buf, coefs)?
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

/// Decode all MS-ADPCM blocks from `data_len` bytes into interleaved i16 frames.
#[cfg(test)]
pub(crate) fn decode_ms_adpcm(
    mss: &mut ByteSource<'_>,
    params: &MsAdpcmParams,
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
    for_each_ms_adpcm_block(mss, params, data_len, max_frames, 16_000, |block| {
        out.extend_from_slice(block);
        Ok(())
    })?;
    Ok(std::mem::take(&mut out))
}

fn decode_ms_block_mono(block: &[u8], coefs: &[(i16, i16)]) -> Result<Vec<i16>> {
    if block.len() < 7 {
        return Err(WavError::format(FormatKind::Adpcm));
    }
    let predictor = block[0] as usize;
    if predictor >= coefs.len() {
        return Err(WavError::format(FormatKind::Adpcm));
    }
    let mut delta = i16::from_le_bytes([block[1], block[2]]);
    let sample1 = i16::from_le_bytes([block[3], block[4]]);
    let sample2 = i16::from_le_bytes([block[5], block[6]]);
    let (c1, c2) = coefs[predictor];

    let mut out = Vec::with_capacity((block.len() - 7) * 2 + 2);
    // History samples: sample2 then sample1 (Microsoft order).
    out.push(sample2);
    out.push(sample1);
    let mut hist1 = sample1;
    let mut hist2 = sample2;

    for &byte in &block[7..] {
        for nibble_shift in [4, 0] {
            let nibble = ((byte >> nibble_shift) & 0x0f) as i32;
            let signed_nibble = if nibble & 0x8 != 0 {
                nibble - 0x10
            } else {
                nibble
            };
            let mut pred =
                (i32::from(hist1) * i32::from(c1) + i32::from(hist2) * i32::from(c2)) >> 8;
            pred += signed_nibble * i32::from(delta);
            let sample = clamp_i16(pred);
            let mut new_delta = (i32::from(MS_ADAPTATION[nibble as usize]) * i32::from(delta)) >> 8;
            if new_delta < 16 {
                new_delta = 16;
            }
            delta = new_delta as i16;
            hist2 = hist1;
            hist1 = sample;
            out.push(sample);
        }
    }
    Ok(out)
}

fn decode_ms_block_stereo(block: &[u8], coefs: &[(i16, i16)]) -> Result<Vec<i16>> {
    if block.len() < 14 {
        return Err(WavError::format(FormatKind::Adpcm));
    }
    let predictor = [block[0], block[1]];
    let mut delta = [
        i16::from_le_bytes([block[2], block[3]]),
        i16::from_le_bytes([block[4], block[5]]),
    ];
    let sample1 = [
        i16::from_le_bytes([block[6], block[7]]),
        i16::from_le_bytes([block[8], block[9]]),
    ];
    let sample2 = [
        i16::from_le_bytes([block[10], block[11]]),
        i16::from_le_bytes([block[12], block[13]]),
    ];
    for &p in &predictor {
        if p as usize >= coefs.len() {
            return Err(WavError::format(FormatKind::Adpcm));
        }
    }

    let mut out = Vec::with_capacity((block.len() - 14) * 2 + 4);
    out.push(sample2[0]);
    out.push(sample2[1]);
    out.push(sample1[0]);
    out.push(sample1[1]);

    let mut hist1 = sample1;
    let mut hist2 = sample2;

    for &byte in &block[14..] {
        // High nibble = left, low nibble = right.
        let nibbles = [((byte >> 4) & 0x0f) as i32, (byte & 0x0f) as i32];
        for (c, &nibble) in nibbles.iter().enumerate() {
            let (c1, c2) = coefs[predictor[c] as usize];
            let mut pred =
                (i32::from(hist1[c]) * i32::from(c1) + i32::from(hist2[c]) * i32::from(c2)) >> 8;
            let signed_nibble = if nibble & 0x8 != 0 {
                nibble - 0x10
            } else {
                nibble
            };
            pred += signed_nibble * i32::from(delta[c]);
            let sample = clamp_i16(pred);
            let mut new_delta =
                (i32::from(MS_ADAPTATION[nibble as usize]) * i32::from(delta[c])) >> 8;
            if new_delta < 16 {
                new_delta = 16;
            }
            delta[c] = new_delta as i16;
            hist2[c] = hist1[c];
            hist1[c] = sample;
            out.push(sample);
        }
    }
    Ok(out)
}

/// Convert interleaved i16 frames to mono f32 (mix) or per-channel split.
pub(crate) fn i16_frames_to_f32(
    interleaved: &[i16],
    channels: usize,
    mode: ChannelMode,
) -> Vec<Vec<f32>> {
    if channels == 0 {
        return vec![Vec::new()];
    }
    let frames = interleaved.len() / channels;
    match mode {
        ChannelMode::Mono => {
            let mut mono = Vec::with_capacity(frames);
            if channels == 1 {
                for &s in interleaved {
                    mono.push(s as f32 * crate::convert::I16_SCALE);
                }
            } else {
                let n = channels as f32;
                for frame in interleaved.chunks_exact(channels) {
                    let mut sum = 0.0f32;
                    for &s in frame {
                        sum += s as f32 * crate::convert::I16_SCALE;
                    }
                    mono.push(sum / n);
                }
            }
            vec![mono]
        }
        ChannelMode::Split => {
            let mut out = vec![Vec::with_capacity(frames); channels];
            for frame in interleaved.chunks_exact(channels) {
                for (c, &s) in frame.iter().enumerate() {
                    out[c].push(s as f32 * crate::convert::I16_SCALE);
                }
            }
            out
        }
    }
}

mod ima;

pub(crate) use ima::for_each_ima_adpcm_block;
#[cfg(test)]
pub(crate) use ima::{decode_ima_adpcm, decode_ima_block_mono, decode_ima_block_stereo};

#[cfg(test)]
#[path = "../adpcm_tests.rs"]
mod adpcm_tests;
