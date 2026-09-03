//! Headerless ITU-T G.722 SB-ADPCM. **64 kbit/s only** (8 bits per pair).
//! Native output is 16 kHz. 56/48 kbit/s packed streams are not decoded.

mod decode;
mod tables;

use crate::convert::I16_SCALE;
use crate::error::{Result, WavError};
use crate::header::SampleCodec;
use crate::options::DecodeOptions;
use crate::{ChannelMode, DecodedWav};

pub(crate) use decode::G722Decoder;

/// Native G.722 PCM rate. SDP/RTP may announce 8000; output is still 16 kHz.
pub(crate) const RATE: u32 = 16_000;

/// PCM frames (per channel) for `bytes` of packed 64 kbit/s G.722.
#[inline]
pub(crate) fn pcm_frames(bytes: u64, channels: usize) -> u64 {
    if channels == 0 {
        0
    } else {
        (bytes / channels as u64).saturating_mul(2)
    }
}

/// `fact` / ds64 sample count wins when it is smaller than the byte estimate.
#[inline]
pub(crate) fn pcm_frames_capped(bytes: u64, channels: usize, declared: Option<u64>) -> u64 {
    let frames = pcm_frames(bytes, channels);
    match declared {
        Some(sc) => frames.min(sc),
        None => frames,
    }
}

/// G.722 always emits 16 kHz; other codecs keep the `fmt ` rate.
#[inline]
pub(crate) fn output_rate(codec: SampleCodec, header_rate: u32) -> u32 {
    if codec == SampleCodec::G722 {
        RATE
    } else {
        header_rate
    }
}

/// Decode headerless G.722 (64 kbit/s) to planar `f32` at 16 kHz.
///
/// `data` is packed 8 bits per 16 kHz pair (ITU / RFC 3551: high 2 bits =
/// high band, low 6 bits = low band), interleaved one byte per channel.
/// Empty input is [`WavError::Empty`]. 56/48 kbit/s packings are not
/// implemented: the decoder always consumes 8 bits per pair.
///
/// `sample_rate` is the caller-declared clock: `8000` (SDP convention) or
/// `16000` (other rates are [`WavError::UnsupportedSampleRate`]). Output
/// [`DecodedWav::sample_rate`] is always 16 kHz.
pub fn decode_g722(
    data: &[u8],
    sample_rate: u32,
    channels: u16,
    opts: &DecodeOptions,
) -> Result<DecodedWav> {
    if sample_rate != 8_000 && sample_rate != RATE {
        return Err(WavError::sample_rate(sample_rate, RATE));
    }
    if RATE > opts.max_sample_rate {
        return Err(WavError::sample_rate(RATE, opts.max_sample_rate));
    }
    if channels == 0 || channels > 26 {
        return Err(WavError::unsupported_codec(0));
    }
    let ch = usize::from(channels);
    if data.is_empty() {
        return Err(WavError::Empty);
    }
    if !data.len().is_multiple_of(ch) {
        return Err(WavError::OddPcm);
    }
    let frames = (data.len() / ch) * 2;
    let max_samples = opts.max_frames(RATE);
    if frames > max_samples {
        let observed = frames as f64 / f64::from(RATE);
        return Err(WavError::too_long(observed, opts.max_duration_secs));
    }
    let n_out = match opts.channel_mode {
        ChannelMode::Mono => 1,
        ChannelMode::Split => ch,
    };
    let out_bytes = (n_out as u64)
        .saturating_mul(frames as u64)
        .saturating_mul(4);
    if out_bytes > opts.max_output_bytes {
        return Err(WavError::output_too_large(out_bytes, opts.max_output_bytes));
    }
    Ok(DecodedWav {
        sample_rate: RATE,
        channels: decode_planar(data, ch, opts.channel_mode),
    })
}

/// Headerless G.722 (64 kbit/s), mono, [`DecodeOptions::speech`].
pub fn decode_g722_mono(data: &[u8]) -> Result<DecodedWav> {
    decode_g722(data, RATE, 1, &DecodeOptions::speech())
}

pub(crate) fn decode_planar(data: &[u8], channels: usize, mode: ChannelMode) -> Vec<Vec<f32>> {
    let frames = (data.len() / channels) * 2;
    let n_out = match mode {
        ChannelMode::Mono => 1,
        ChannelMode::Split => channels,
    };
    let mut out: Vec<Vec<f32>> = (0..n_out).map(|_| vec![0.0f32; frames]).collect();
    let mut decs: Vec<G722Decoder> = (0..channels).map(|_| G722Decoder::new()).collect();
    decode_into(&mut decs, data, mode, &mut out);
    out
}

/// Decode interleaved G.722 bytes into pre-sized planes (`decs` keep state).
/// Each plane is filled up to `plane.len()` (odd `fact` can truncate the pair).
pub(crate) fn decode_into(
    decs: &mut [G722Decoder],
    data: &[u8],
    mode: ChannelMode,
    out: &mut [Vec<f32>],
) {
    let ch = decs.len();
    if ch == 0 || data.is_empty() {
        return;
    }
    let pairs = data.len() / ch;
    match mode {
        ChannelMode::Mono => {
            let dst = &mut out[0];
            let cap = dst.len();
            let n = ch as f32;
            let mut o = 0usize;
            for t in 0..pairs {
                let mut s0 = 0.0f32;
                let mut s1 = 0.0f32;
                for (c, dec) in decs.iter_mut().enumerate() {
                    let pcm = dec.decode_byte(data[t * ch + c]);
                    s0 += pcm[0] as f32 * I16_SCALE;
                    s1 += pcm[1] as f32 * I16_SCALE;
                }
                s0 /= n;
                s1 /= n;
                if o < cap {
                    dst[o] = s0;
                    o += 1;
                }
                if o < cap {
                    dst[o] = s1;
                    o += 1;
                }
            }
        }
        ChannelMode::Split => {
            for t in 0..pairs {
                for (c, dec) in decs.iter_mut().enumerate() {
                    let pcm = dec.decode_byte(data[t * ch + c]);
                    let i0 = t * 2;
                    let dst = &mut out[c];
                    if i0 < dst.len() {
                        dst[i0] = pcm[0] as f32 * I16_SCALE;
                    }
                    if i0 + 1 < dst.len() {
                        dst[i0 + 1] = pcm[1] as f32 * I16_SCALE;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "../g722_tests.rs"]
mod g722_tests;
