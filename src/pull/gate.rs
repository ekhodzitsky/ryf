//! Codec checks shared by probe and decode.

use crate::error::{FormatKind, Result, WavError};
use crate::header::{FmtFields, SampleCodec};
use crate::options::DecodeOptions;

/// Resolve tag `0x0064`, reject an unsupported codec, and apply the
/// output-rate ceiling.
///
/// The returned rate is the PCM rate decode emits. G.722 is always
/// 16 kHz, including when `fmt ` says 8000, so the ceiling is applied
/// after that conversion.
pub(crate) fn codec_gate(fmt: &mut FmtFields, opts: &DecodeOptions) -> Result<u32> {
    crate::g726::resolve_tag(fmt, opts.tag_0064)?;
    let header_rate = fmt.sample_rate;
    if header_rate == 0 || header_rate > opts.max_sample_rate {
        return Err(WavError::sample_rate(header_rate, opts.max_sample_rate));
    }
    if matches!(fmt.codec, SampleCodec::Unresolved0064) {
        return Err(WavError::format(FormatKind::AmbiguousCodec));
    }
    if fmt.codec == SampleCodec::G726 && header_rate != crate::g726::RATE {
        return Err(WavError::sample_rate(header_rate, crate::g726::RATE));
    }
    if matches!(fmt.codec, SampleCodec::Unsupported) {
        return Err(WavError::unsupported_codec(fmt.format_tag));
    }
    if fmt.codec.is_adpcm() {
        super::adpcm::ensure_adpcm_enabled()?;
    }
    let sample_rate = crate::g722::output_rate(fmt.codec, header_rate);
    if sample_rate > opts.max_sample_rate {
        return Err(WavError::sample_rate(sample_rate, opts.max_sample_rate));
    }
    Ok(sample_rate)
}

/// Common frame calculation for probe and decode. Smaller positive fact/ds64
/// counts trim the available audio; larger declarations never inflate it.
pub(crate) fn frame_layout(
    fmt: &FmtFields,
    data_len: Option<u64>,
    declared: Option<u64>,
) -> Result<(usize, Option<u64>)> {
    if let Some(layout) = super::special::framed_count(fmt, data_len, declared)? {
        return Ok(layout);
    }
    if fmt.codec.is_adpcm() {
        let (block, channels, ima) = match &fmt.params {
            crate::header::CodecParams::Ms(p) => (p.block_align, p.channels, false),
            crate::header::CodecParams::Ima(p) => (p.block_align, p.channels, true),
            _ => return Err(WavError::format(FormatKind::Adpcm)),
        };
        return Ok((
            0,
            data_len.map(|n| {
                super::adpcm::adpcm_frames_capped(
                    n,
                    u64::from(block),
                    channels as u64,
                    ima,
                    declared,
                )
            }),
        ));
    }
    let bytes = fmt
        .sample_width
        .checked_mul(fmt.channels)
        .filter(|n| *n > 0)
        .ok_or_else(|| WavError::format(FormatKind::InvalidSize))?;
    let frames = data_len.map(|n| {
        let available = n / bytes as u64;
        match declared {
            Some(n) if n > 0 => available.min(n),
            _ => available,
        }
    });
    Ok((bytes, frames))
}
