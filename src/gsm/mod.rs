//! Microsoft GSM 06.10 (wav49 / WAVE tag 0x0031). 8 kHz, 65-byte MS-GSM
//! blocks (two 160-sample frames, little-endian bitstream). 13 kbit/s only.

mod decode;
mod tables;

use crate::DecodedWav;
use crate::convert::I16_SCALE;
use crate::error::{Result, WavError};
use crate::options::DecodeOptions;

pub(crate) use decode::{GsmDecoder, MS_BLOCK, MS_SAMPLES};

/// Typical Asterisk / MS-GSM sample rate.
pub(crate) const RATE: u32 = 8_000;

/// PCM frames for `bytes` of 65-byte MS-GSM blocks.
#[inline]
pub(crate) fn pcm_frames(bytes: u64) -> u64 {
    (bytes / MS_BLOCK as u64).saturating_mul(MS_SAMPLES as u64)
}

#[inline]
pub(crate) fn pcm_frames_capped(bytes: u64, declared: Option<u64>) -> u64 {
    let frames = pcm_frames(bytes);
    match declared {
        Some(sc) => frames.min(sc),
        None => frames,
    }
}

/// Decode headerless Microsoft GSM (wav49 payload) to planar `f32`.
///
/// `data` is packed 65-byte blocks (320 PCM samples each at 8 kHz, mono).
/// Empty input is [`WavError::Empty`]. Length not a multiple of 65 is
/// [`WavError::OddPcm`]. `sample_rate` must be 8000.
pub fn decode_gsm(data: &[u8], sample_rate: u32, opts: &DecodeOptions) -> Result<DecodedWav> {
    if sample_rate != RATE {
        return Err(WavError::sample_rate(sample_rate, RATE));
    }
    if RATE > opts.max_sample_rate {
        return Err(WavError::sample_rate(RATE, opts.max_sample_rate));
    }
    if data.is_empty() {
        return Err(WavError::Empty);
    }
    if !data.len().is_multiple_of(MS_BLOCK) {
        return Err(WavError::OddPcm);
    }
    let frames = (data.len() / MS_BLOCK).saturating_mul(MS_SAMPLES);
    let max_samples = opts.max_frames(RATE);
    if frames > max_samples {
        let observed = frames as f64 / f64::from(RATE);
        return Err(WavError::too_long(observed, opts.max_duration_secs));
    }
    let out_bytes = (frames as u64).saturating_mul(4);
    if out_bytes > opts.max_output_bytes {
        return Err(WavError::output_too_large(out_bytes, opts.max_output_bytes));
    }
    Ok(DecodedWav {
        sample_rate: RATE,
        channels: vec![decode_mono_f32(data)],
    })
}

/// Headerless MS-GSM, [`DecodeOptions::speech`].
pub fn decode_gsm_mono(data: &[u8]) -> Result<DecodedWav> {
    decode_gsm(data, RATE, &DecodeOptions::speech())
}

pub(crate) fn decode_mono_f32(data: &[u8]) -> Vec<f32> {
    let n_blocks = data.len() / MS_BLOCK;
    let mut out = vec![0.0f32; n_blocks.saturating_mul(MS_SAMPLES)];
    let mut dec = GsmDecoder::new();
    let mut pcm = [0i16; MS_SAMPLES];
    for (i, chunk) in data.as_chunks::<MS_BLOCK>().0.iter().enumerate() {
        dec.decode_ms_block(chunk, &mut pcm);
        let dst = &mut out[i * MS_SAMPLES..(i + 1) * MS_SAMPLES];
        scale_i16(dst, &pcm);
    }
    out
}

#[inline]
pub(crate) fn scale_i16(dst: &mut [f32], pcm: &[i16]) {
    for (d, &s) in dst.iter_mut().zip(pcm) {
        *d = s as f32 * I16_SCALE;
    }
}

#[cfg(test)]
#[path = "../gsm_tests.rs"]
mod gsm_tests;
