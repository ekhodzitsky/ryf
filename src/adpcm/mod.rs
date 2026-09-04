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
use crate::scrub::{ScrubVec, scrub_vec};

/// MS-ADPCM coefficient pairs stored in the `fmt ` extension (up to 7).
#[derive(Debug, Clone)]
pub(crate) struct MsAdpcmParams {
    pub block_align: u16,
    /// Header `wSamplesPerBlock`. Decode walks every nibble; duration uses
    /// the block_align formula instead of this value.
    #[allow(dead_code)]
    pub samples_per_block: u16,
    pub channels: usize,
    /// `(coeff1, coeff2)` pairs; length is `num_coefs` (1..=7).
    pub coefs: Vec<(i16, i16)>,
}

/// IMA/DVI ADPCM parameters from the `fmt ` extension.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ImaAdpcmParams {
    pub block_align: u16,
    /// Header `wSamplesPerBlock`. Same rule as [`MsAdpcmParams`].
    #[allow(dead_code)]
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

/// Microsoft adaptive step: `(adapt[nibble] * delta) >> 8`, then `16..=32767`.
/// Truncating the i32 to `i16` wraps (e.g. 20000 * 512 >> 8 = 40000 -> -25536).
#[inline]
pub(super) fn adapt_ms_delta(delta: i16, nibble: i32) -> i16 {
    let mut new_delta = (i32::from(MS_ADAPTATION[nibble as usize]) * i32::from(delta)) >> 8;
    if new_delta < 16 {
        new_delta = 16;
    }
    new_delta.min(i32::from(i16::MAX)) as i16
}

/// Duration cap is TooLong. `target` stops the walk (smaller `fact`).
/// `Some((slice, done))` means emit `slice`; `done` ends the loop.
pub(super) fn take_block(
    decoded: &[i16],
    ch: usize,
    frames: usize,
    max_frames: usize,
    target: usize,
    sample_rate: u32,
) -> Result<Option<(&[i16], bool)>> {
    let ch = ch.max(1);
    let frames_this = decoded.len() / ch;
    if frames_this > max_frames.saturating_sub(frames) {
        let observed_s = frames.saturating_add(frames_this) as f64 / sample_rate.max(1) as f64;
        let max_secs = max_frames as f64 / sample_rate.max(1) as f64;
        return Err(WavError::too_long(observed_s, max_secs));
    }
    let room = target.saturating_sub(frames);
    if room == 0 {
        return Ok(None);
    }
    let take = frames_this.min(room);
    Ok(Some((&decoded[..take * ch], take < frames_this)))
}

/// Walk MS-ADPCM blocks and invoke `on_block` with interleaved i16 (one
/// compressed block at a time - no full-stream i16 allocation).
#[allow(clippy::too_many_arguments)]
pub(crate) fn for_each_ms_adpcm_block(
    mss: &mut ByteSource<'_>,
    params: &MsAdpcmParams,
    data_len: u64,
    max_frames: usize,
    target: usize,
    sample_rate: u32,
    big_endian: bool,
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
        mss.read_buf_exact(&mut block_buf)
            .map_err(WavError::packet_io)?;
        remaining -= block as u64;

        let decoded = if ch == 1 {
            decode_ms_block_mono(&block_buf, coefs, big_endian)?
        } else {
            decode_ms_block_stereo(&block_buf, coefs, big_endian)?
        };
        match take_block(&decoded, ch, frames, max_frames, target, sample_rate)? {
            None => break,
            Some((slice, done)) => {
                on_block(slice)?;
                frames += slice.len() / ch;
                if done {
                    break;
                }
            }
        }
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
    for_each_ms_adpcm_block(
        mss,
        params,
        data_len,
        max_frames,
        usize::MAX,
        16_000,
        false,
        |block| {
            out.extend_from_slice(block);
            Ok(())
        },
    )?;
    Ok(std::mem::take(&mut out))
}

pub(super) fn i16_at(block: &[u8], off: usize, be: bool) -> Result<i16> {
    let pair = block
        .get(off..off + 2)
        .ok_or_else(|| WavError::format(FormatKind::Adpcm))?;
    Ok(if be {
        i16::from_be_bytes([pair[0], pair[1]])
    } else {
        i16::from_le_bytes([pair[0], pair[1]])
    })
}

fn decode_ms_block_mono(block: &[u8], coefs: &[(i16, i16)], be: bool) -> Result<Vec<i16>> {
    if block.len() < 7 {
        return Err(WavError::format(FormatKind::Adpcm));
    }
    let predictor = block[0] as usize;
    if predictor >= coefs.len() {
        return Err(WavError::format(FormatKind::Adpcm));
    }
    let mut delta = i16_at(block, 1, be)?;
    let sample1 = i16_at(block, 3, be)?;
    let sample2 = i16_at(block, 5, be)?;
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
            delta = adapt_ms_delta(delta, nibble);
            hist2 = hist1;
            hist1 = sample;
            out.push(sample);
        }
    }
    Ok(out)
}

fn decode_ms_block_stereo(block: &[u8], coefs: &[(i16, i16)], be: bool) -> Result<Vec<i16>> {
    if block.len() < 14 {
        return Err(WavError::format(FormatKind::Adpcm));
    }
    let predictor = [block[0], block[1]];
    let mut delta = [i16_at(block, 2, be)?, i16_at(block, 4, be)?];
    let sample1 = [i16_at(block, 6, be)?, i16_at(block, 8, be)?];
    let sample2 = [i16_at(block, 10, be)?, i16_at(block, 12, be)?];
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
            delta[c] = adapt_ms_delta(delta[c], nibble);
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
