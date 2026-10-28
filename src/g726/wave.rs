//! WAVE `fmt ` classification for G.726 and the `0x0064` conflict.
//!
//! Tag `0x0045` is the ffmpeg G.726 mux tag: AAL2 packing (MSB first),
//! mono, 8000 Hz, `wBitsPerSample` 2/3/4/5, `nBlockAlign` a multiple of
//! the byte quantum (1, 3, 1, 5). The data chunk is one codeword stream.
//!
//! Tag `0x0064` is mmreg G.726 and the Asterisk G.722 alias. Auto mode
//! keeps a 64 kbit/s byte rate on the G.722 path. Everything else needs
//! [`Tag0064`] or returns [`FormatKind::AmbiguousCodec`].

use super::{G726Packing, G726Rate, Tag0064};
use crate::error::{FormatKind, Result, WavError};
use crate::header::{FmtFields, G726Params, SampleCodec, Tag0064Facts};

pub(crate) fn fmt_fixed(
    channels: usize,
    sample_rate: u32,
    byte_rate: u32,
    block_align: u16,
    bits: u16,
    big_endian: bool,
    format_tag: u16,
) -> Result<FmtFields> {
    if channels != 1 {
        return Err(WavError::format(FormatKind::ChannelLayout));
    }
    if sample_rate != 8_000 && sample_rate != 0 {
        return Err(WavError::sample_rate(sample_rate, 8_000));
    }
    let width = agreed_bits(sample_rate, byte_rate, bits, channels)?;
    let rate =
        G726Rate::from_bits(width).ok_or_else(|| WavError::format(FormatKind::InvalidSize))?;
    if block_align == 0 || !block_align.is_multiple_of(rate.byte_quantum()) {
        return Err(WavError::format(FormatKind::InvalidSize));
    }
    Ok(fields(
        SampleCodec::G726,
        channels,
        sample_rate,
        big_endian,
        format_tag,
        crate::header::CodecParams::G726(G726Params {
            bits: width,
            packing: G726Packing::Aal2,
        }),
    ))
}

pub(crate) fn fmt_tag_0064(
    channels: usize,
    sample_rate: u32,
    byte_rate: u32,
    block_align: u16,
    bits: u16,
    big_endian: bool,
) -> Result<FmtFields> {
    Ok(fields(
        SampleCodec::Unresolved0064,
        channels,
        sample_rate,
        big_endian,
        0x0064,
        crate::header::CodecParams::Tag0064(Tag0064Facts {
            byte_rate,
            bits,
            block_align,
        }),
    ))
}

pub(crate) fn resolve_tag(fmt: &mut FmtFields, policy: Tag0064) -> Result<()> {
    if fmt.codec != SampleCodec::Unresolved0064 {
        return Ok(());
    }
    let facts = fmt
        .tag_0064_facts()
        .copied()
        .ok_or_else(|| WavError::format(FormatKind::MalformedFmt))?;
    match policy {
        Tag0064::Auto => {
            if is_g722(fmt, &facts) {
                force_g722(fmt);
                Ok(())
            } else {
                Err(WavError::format(FormatKind::AmbiguousCodec))
            }
        }
        Tag0064::G722 => {
            force_g722(fmt);
            Ok(())
        }
        Tag0064::G726 { packing } => {
            if fmt.channels != 1 {
                return Err(WavError::format(FormatKind::ChannelLayout));
            }
            if fmt.sample_rate != 8_000 {
                return Err(WavError::sample_rate(fmt.sample_rate, 8_000));
            }
            let width = agreed_bits(fmt.sample_rate, facts.byte_rate, facts.bits, fmt.channels)?;
            let rate = G726Rate::from_bits(width)
                .ok_or_else(|| WavError::format(FormatKind::InvalidSize))?;
            if facts.block_align == 0 || !facts.block_align.is_multiple_of(rate.byte_quantum()) {
                return Err(WavError::format(FormatKind::InvalidSize));
            }
            fmt.codec = SampleCodec::G726;
            fmt.sample_width = 1;
            fmt.params = crate::header::CodecParams::G726(G726Params {
                bits: width,
                packing,
            });
            Ok(())
        }
    }
}

fn is_g722(fmt: &FmtFields, facts: &Tag0064Facts) -> bool {
    let Ok(ch) = u32::try_from(fmt.channels) else {
        return false;
    };
    ch > 0 && facts.byte_rate == 8_000u32.saturating_mul(ch)
}

fn force_g722(fmt: &mut FmtFields) {
    fmt.codec = SampleCodec::G722;
    fmt.sample_width = 1;
    fmt.params = crate::header::CodecParams::None;
}

fn agreed_bits(sample_rate: u32, byte_rate: u32, bits: u16, channels: usize) -> Result<u8> {
    let from_bits = match bits {
        2..=5 => Some(bits as u8),
        _ => None,
    };
    let from_rate = bits_from_byte_rate(sample_rate, byte_rate, channels);
    match (from_bits, from_rate) {
        (Some(a), Some(b)) if a == b => Ok(a),
        (Some(a), None) => Ok(a),
        (None, Some(b)) => Ok(b),
        (Some(_), Some(_)) => Err(WavError::format(FormatKind::MalformedFmt)),
        (None, None) => Err(WavError::format(FormatKind::InvalidSize)),
    }
}

fn bits_from_byte_rate(sample_rate: u32, byte_rate: u32, channels: usize) -> Option<u8> {
    let ch = u32::try_from(channels).ok()?;
    if sample_rate == 0 || ch == 0 || byte_rate == 0 || !byte_rate.is_multiple_of(ch) {
        return None;
    }
    let per = byte_rate / ch;
    let num = per.checked_mul(8)?;
    if !num.is_multiple_of(sample_rate) {
        return None;
    }
    let bits = num / sample_rate;
    if matches!(bits, 2..=5) {
        Some(bits as u8)
    } else {
        None
    }
}

fn fields(
    codec: SampleCodec,
    channels: usize,
    sample_rate: u32,
    big_endian: bool,
    format_tag: u16,
    params: crate::header::CodecParams,
) -> FmtFields {
    FmtFields {
        codec,
        channels,
        sample_rate,
        sample_width: 1,
        big_endian,
        format_tag,
        params,
    }
}

#[cfg(test)]
#[path = "../g726_wave_tests.rs"]
mod g726_wave_tests;

#[cfg(test)]
#[path = "../g726_wave_edge_tests.rs"]
mod g726_wave_edge_tests;
