use super::*;
use crate::error::{FormatKind, Result};
use crate::source::ByteSource;

#[test]
fn container_and_codec_helpers() {
    assert!(!Container::Riff.big_endian());
    assert!(Container::Rifx.big_endian());
    assert!(!Container::Rf64.big_endian());

    assert_eq!(SampleCodec::U8.probe(), ProbeCodec::PcmU8);
    assert_eq!(SampleCodec::S16.probe(), ProbeCodec::PcmS16);
    assert_eq!(SampleCodec::S24.probe(), ProbeCodec::PcmS24);
    assert_eq!(SampleCodec::S24_4.probe(), ProbeCodec::PcmS24);
    assert_eq!(SampleCodec::S32.probe(), ProbeCodec::PcmS32);
    assert_eq!(SampleCodec::F32.probe(), ProbeCodec::Float32);
    assert_eq!(SampleCodec::F64.probe(), ProbeCodec::Float64);
    assert_eq!(SampleCodec::ALaw.probe(), ProbeCodec::ALaw);
    assert_eq!(SampleCodec::MuLaw.probe(), ProbeCodec::MuLaw);
    assert_eq!(SampleCodec::MsAdpcm.probe(), ProbeCodec::MsAdpcm);
    assert_eq!(SampleCodec::ImaAdpcm.probe(), ProbeCodec::ImaAdpcm);
    assert_eq!(SampleCodec::G722.probe(), ProbeCodec::G722);
    assert_eq!(SampleCodec::Unsupported.probe(), ProbeCodec::Unsupported);
    assert!(SampleCodec::MsAdpcm.is_adpcm());
    assert!(SampleCodec::ImaAdpcm.is_adpcm());
    assert!(!SampleCodec::G722.is_adpcm());
    assert!(!SampleCodec::S16.is_adpcm());
    assert_eq!(WAVE_FORMAT_G722_ASTERISK, 0x0064);
    assert_eq!(WAVE_FORMAT_G722_ADPCM, 0x0065);
    assert_eq!(WAVE_FORMAT_ADPCM_G722, 0x028F);
}

#[test]
fn ambisonic_and_channel_mask_helpers() -> Result<()> {
    assert_eq!(map_ambisonic_channel_count(4)?, 4);
    assert_eq!(map_ambisonic_channel_count(16)?, 16);
    assert!(map_ambisonic_channel_count(10).is_err());
    assert!(map_ambisonic_channel_count(32).is_err());

    // Expand mask when channels > popcount.
    assert_eq!(fix_wave_channel_mask(0b01, 2), Some(0b11));
    // Shrink mask when popcount > channels.
    let shrunk = fix_wave_channel_mask(0b1111, 2)
        .ok_or_else(|| crate::WavError::format(FormatKind::ChannelLayout))?;
    assert_eq!(shrunk.count_ones(), 2);
    // Too many channels.
    assert!(fix_wave_channel_mask(0, 33).is_none());
    // Equal already.
    assert_eq!(fix_wave_channel_mask(0b11, 2), Some(0b11));
    Ok(())
}

#[test]
fn parse_minimal_pcm_and_reject_garbage() -> Result<()> {
    let mut fmt = Vec::new();
    fmt.extend_from_slice(&1u16.to_le_bytes());
    fmt.extend_from_slice(&1u16.to_le_bytes());
    fmt.extend_from_slice(&16_000u32.to_le_bytes());
    fmt.extend_from_slice(&32_000u32.to_le_bytes());
    fmt.extend_from_slice(&2u16.to_le_bytes());
    fmt.extend_from_slice(&16u16.to_le_bytes());
    let payload = [0u8; 4];
    let mut body = Vec::new();
    body.extend_from_slice(b"fmt ");
    body.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
    body.extend_from_slice(&fmt);
    body.extend_from_slice(b"data");
    body.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    body.extend_from_slice(&payload);
    let mut file = Vec::new();
    file.extend_from_slice(b"RIFF");
    file.extend_from_slice(&(4 + body.len() as u32).to_le_bytes());
    file.extend_from_slice(b"WAVE");
    file.extend_from_slice(&body);

    let mut src = ByteSource::from_slice(&file);
    let h = parse_header(&mut src)?;
    assert_eq!(h.fmt.sample_rate, 16_000);
    assert_eq!(h.fmt.channels, 1);
    assert_eq!(h.fmt.codec, SampleCodec::S16);

    let mut src = ByteSource::from_slice(b"not a wave file!!!!");
    assert!(parse_header(&mut src).is_err());
    Ok(())
}

pub(super) fn riff(fmt: &[u8], data: &[u8]) -> Vec<u8> {
    riff_parts(fmt, None, data)
}

pub(super) fn riff_fact(fmt: &[u8], fact: u32, data: &[u8]) -> Vec<u8> {
    riff_parts(fmt, Some(fact), data)
}

fn riff_parts(fmt: &[u8], fact: Option<u32>, data: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(b"fmt ");
    body.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
    body.extend_from_slice(fmt);
    if fmt.len() % 2 == 1 {
        body.push(0);
    }
    if let Some(n) = fact {
        body.extend_from_slice(b"fact");
        body.extend_from_slice(&4u32.to_le_bytes());
        body.extend_from_slice(&n.to_le_bytes());
    }
    body.extend_from_slice(b"data");
    body.extend_from_slice(&(data.len() as u32).to_le_bytes());
    body.extend_from_slice(data);
    if data.len() % 2 == 1 {
        body.push(0);
    }
    let mut file = Vec::new();
    file.extend_from_slice(b"RIFF");
    file.extend_from_slice(&(4 + body.len() as u32).to_le_bytes());
    file.extend_from_slice(b"WAVE");
    file.extend_from_slice(&body);
    file
}

pub(super) fn le_pcm16_fmt() -> Vec<u8> {
    let mut f = Vec::new();
    f.extend_from_slice(&1u16.to_le_bytes());
    f.extend_from_slice(&1u16.to_le_bytes());
    f.extend_from_slice(&16_000u32.to_le_bytes());
    f.extend_from_slice(&32_000u32.to_le_bytes());
    f.extend_from_slice(&2u16.to_le_bytes());
    f.extend_from_slice(&16u16.to_le_bytes());
    f
}

pub(super) fn parse_ok(bytes: &[u8]) -> Result<WavHeader> {
    parse_header(&mut ByteSource::from_slice(bytes))
}
pub(super) fn parse_err(bytes: &[u8]) -> bool {
    parse_header(&mut ByteSource::from_slice(bytes)).is_err()
}

pub(super) fn ieee_fmt(chunk_pad: &[u8], bits: u16, block: u16, ch: u16) -> Vec<u8> {
    let mut f = Vec::new();
    f.extend_from_slice(&WAVE_FORMAT_IEEE_FLOAT.to_le_bytes());
    f.extend_from_slice(&ch.to_le_bytes());
    f.extend_from_slice(&16_000u32.to_le_bytes());
    f.extend_from_slice(&(16_000u32 * u32::from(block)).to_le_bytes());
    f.extend_from_slice(&block.to_le_bytes());
    f.extend_from_slice(&bits.to_le_bytes());
    f.extend_from_slice(chunk_pad);
    f
}

pub(super) fn g711_fmt(tag: u16, extra: &[u8]) -> Vec<u8> {
    let mut f = Vec::new();
    f.extend_from_slice(&tag.to_le_bytes());
    f.extend_from_slice(&1u16.to_le_bytes());
    f.extend_from_slice(&8_000u32.to_le_bytes());
    f.extend_from_slice(&8_000u32.to_le_bytes());
    f.extend_from_slice(&1u16.to_le_bytes());
    f.extend_from_slice(&8u16.to_le_bytes());
    f.extend_from_slice(extra);
    f
}

pub(super) fn ext_fmt(
    guid: [u8; 16],
    bits: u16,
    valid: u16,
    mask: u32,
    ch: u16,
    extra_cb: u16,
) -> Vec<u8> {
    let width = (bits / 8).max(1);
    let mut f = Vec::new();
    f.extend_from_slice(&WAVE_FORMAT_EXTENSIBLE.to_le_bytes());
    f.extend_from_slice(&ch.to_le_bytes());
    f.extend_from_slice(&16_000u32.to_le_bytes());
    f.extend_from_slice(&(16_000u32 * u32::from(ch) * u32::from(width)).to_le_bytes());
    f.extend_from_slice(&(ch * width).to_le_bytes());
    f.extend_from_slice(&bits.to_le_bytes());
    f.extend_from_slice(&extra_cb.to_le_bytes());
    f.extend_from_slice(&valid.to_le_bytes());
    f.extend_from_slice(&mask.to_le_bytes());
    f.extend_from_slice(&guid);
    f
}

#[test]
fn container_width_and_pcm_codec_edges() {
    assert!(container_width(2, 0, 16).is_err());
    assert!(container_width(2, 1, 0).is_err());
    assert!(container_width(2, 1, 72).is_err());
    assert_eq!(container_width(0, 1, 16).unwrap_or(0), 2);
    assert_eq!(container_width(3, 2, 16).unwrap_or(0), 2); // not multiple: packed
    assert_eq!(container_width(16, 1, 16).unwrap_or(0), 2); // w>8: packed
    assert!(pcm_codec_for(12, 2).is_err());
    assert!(pcm_codec_for(16, 1).is_err());
}

#[test]
fn channel_mask_expand_overflow_and_unfixable() {
    assert_eq!(fix_wave_channel_mask(0, 32), Some(u32::MAX));
    assert!(fix_wave_channel_mask(1u32 << 31, 3).is_none());
}
