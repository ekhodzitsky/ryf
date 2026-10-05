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
    let sample_rate = crate::g722::output_rate(fmt.codec, header_rate);
    if sample_rate > opts.max_sample_rate {
        return Err(WavError::sample_rate(sample_rate, opts.max_sample_rate));
    }
    Ok(sample_rate)
}
