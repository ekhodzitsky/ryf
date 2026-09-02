//! Headerless G.711. One byte per sample; rate and channel count are stated
//! by the caller (nothing here defaults to 8 kHz).

use crate::convert::{convert_g711_mono, g711_table, mix_g711, split_g711};
use crate::error::{Result, WavError};
use crate::options::DecodeOptions;
use crate::{ChannelMode, DecodedWav};

/// ITU-T G.711 companding law.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum G711Law {
    /// A-law (PCMA).
    ALaw,
    /// µ-law (PCMU).
    MuLaw,
}

/// Decode headerless G.711 to planar `f32`.
///
/// `data` is interleaved, one byte per sample. Empty input is [`WavError::Empty`].
pub fn decode_g711(
    data: &[u8],
    law: G711Law,
    sample_rate: u32,
    channels: u16,
    opts: &DecodeOptions,
) -> Result<DecodedWav> {
    if sample_rate == 0 || sample_rate > opts.max_sample_rate {
        return Err(WavError::sample_rate(sample_rate, opts.max_sample_rate));
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
    let frames = data.len() / ch;
    let max_samples = opts.max_frames(sample_rate);
    if frames > max_samples {
        let observed = frames as f64 / f64::from(sample_rate.max(1));
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
    let table = g711_table(matches!(law, G711Law::ALaw));
    let planes = match opts.channel_mode {
        ChannelMode::Mono if ch == 1 => {
            let mut out = vec![0.0f32; frames];
            convert_g711_mono(data, &mut out, table);
            vec![out]
        }
        ChannelMode::Mono => {
            let mut out = vec![0.0f32; frames];
            mix_g711(data, &mut out, ch, table);
            vec![out]
        }
        ChannelMode::Split => {
            let mut out: Vec<Vec<f32>> = (0..ch).map(|_| vec![0.0f32; frames]).collect();
            {
                let mut refs: Vec<&mut [f32]> = out.iter_mut().map(|c| c.as_mut_slice()).collect();
                split_g711(data, &mut refs, table);
            }
            out
        }
    };
    Ok(DecodedWav {
        sample_rate,
        channels: planes,
    })
}

/// Headerless A-law, mono, [`DecodeOptions::speech`].
pub fn decode_g711_alaw(data: &[u8], sample_rate: u32) -> Result<DecodedWav> {
    decode_g711(
        data,
        G711Law::ALaw,
        sample_rate,
        1,
        &DecodeOptions::speech(),
    )
}

/// Headerless µ-law, mono, [`DecodeOptions::speech`].
pub fn decode_g711_mulaw(data: &[u8], sample_rate: u32) -> Result<DecodedWav> {
    decode_g711(
        data,
        G711Law::MuLaw,
        sample_rate,
        1,
        &DecodeOptions::speech(),
    )
}

#[cfg(test)]
#[path = "g711_tests.rs"]
mod g711_tests;
