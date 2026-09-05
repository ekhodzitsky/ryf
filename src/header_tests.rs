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

fn data_then_fmt_file(fmt: &[u8], payload: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(b"data");
    body.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    body.extend_from_slice(payload);
    if payload.len() % 2 == 1 {
        body.push(0);
    }
    body.extend_from_slice(b"fmt ");
    body.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
    body.extend_from_slice(fmt);
    if fmt.len() % 2 == 1 {
        body.push(0);
    }
    let mut file = Vec::new();
    file.extend_from_slice(b"RIFF");
    file.extend_from_slice(&(4 + body.len() as u32).to_le_bytes());
    file.extend_from_slice(b"WAVE");
    file.extend_from_slice(&body);
    file
}

#[test]
fn data_before_fmt_parses_and_decodes() -> Result<()> {
    let payload = [0u8, 0, 0xe8, 0x03, 0x18, 0xfc];
    let file = data_then_fmt_file(&le_pcm16_fmt(), &payload);
    let h = parse_ok(&file)?;
    assert_eq!(h.fmt.codec, SampleCodec::S16);
    assert_eq!(h.data_pos, 20);
    let d = crate::decode_bytes(&file, crate::DecodeOptions::unbounded())?;
    assert_eq!(d.frames(), 3);
    // First data still wins when a second data follows fmt.
    let first = [0u8, 1, 2, 3];
    let second = [4u8, 5, 6, 7, 8, 9, 10, 11];
    let fmt = le_pcm16_fmt();
    let mut body = Vec::new();
    body.extend_from_slice(b"data");
    body.extend_from_slice(&(first.len() as u32).to_le_bytes());
    body.extend_from_slice(&first);
    body.extend_from_slice(b"fmt ");
    body.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
    body.extend_from_slice(&fmt);
    body.extend_from_slice(b"data");
    body.extend_from_slice(&(second.len() as u32).to_le_bytes());
    body.extend_from_slice(&second);
    let mut two = Vec::new();
    two.extend_from_slice(b"RIFF");
    two.extend_from_slice(&(4 + body.len() as u32).to_le_bytes());
    two.extend_from_slice(b"WAVE");
    two.extend_from_slice(&body);
    let d = crate::decode_bytes(&two, crate::DecodeOptions::unbounded())?;
    assert_eq!(d.frames(), 2);
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

#[test]
fn rf64_ds64_table_skips_junk_sentinel() -> Result<()> {
    let fmt = le_pcm16_fmt();
    let payload = [0u8, 1, 2, 3];
    let junk = [9u8; 8];
    let mut ds64 = Vec::new();
    ds64.extend_from_slice(&200u64.to_le_bytes());
    ds64.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    ds64.extend_from_slice(&2u64.to_le_bytes());
    ds64.extend_from_slice(&1u32.to_le_bytes());
    ds64.extend_from_slice(b"JUNK");
    ds64.extend_from_slice(&(junk.len() as u64).to_le_bytes());
    let mut file = b"RF64".to_vec();
    file.extend_from_slice(&u32::MAX.to_le_bytes());
    file.extend_from_slice(b"WAVE");
    file.extend_from_slice(b"ds64");
    file.extend_from_slice(&(ds64.len() as u32).to_le_bytes());
    file.extend_from_slice(&ds64);
    file.extend_from_slice(b"fmt ");
    file.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
    file.extend_from_slice(&fmt);
    file.extend_from_slice(b"JUNK");
    file.extend_from_slice(&u32::MAX.to_le_bytes());
    file.extend_from_slice(&junk);
    file.extend_from_slice(b"data");
    file.extend_from_slice(&u32::MAX.to_le_bytes());
    file.extend_from_slice(&payload);
    let h = parse_ok(&file)?;
    assert_eq!(h.fmt.codec, SampleCodec::S16);
    let d = crate::decode_bytes(&file, crate::DecodeOptions::unbounded())?;
    assert_eq!(d.frames(), 2);
    Ok(())
}

#[test]
fn w64_data_size_without_fact_matches_size_minus_24() -> Result<()> {
    let payload = [10i16, -10, 20];
    let mut pcm = Vec::new();
    for &s in &payload {
        pcm.extend_from_slice(&s.to_le_bytes());
    }
    let mut fmt = le_pcm16_fmt();
    let pad = (8 - (pcm.len() % 8)) % 8;
    let mut file = W64_GUID_RIFF.to_vec();
    let size_pos = file.len();
    file.extend_from_slice(&0u64.to_le_bytes());
    file.extend_from_slice(&W64_GUID_WAVE);
    let fmt_pad = (8 - (fmt.len() % 8)) % 8;
    fmt.extend(std::iter::repeat_n(0u8, fmt_pad));
    file.extend_from_slice(&W64_GUID_FMT);
    file.extend_from_slice(&(24u64 + fmt.len() as u64).to_le_bytes());
    file.extend_from_slice(&fmt);
    // Size includes the 2-byte pad (Sony writers often do not; this one does).
    file.extend_from_slice(&W64_GUID_DATA);
    file.extend_from_slice(&(24u64 + pcm.len() as u64 + pad as u64).to_le_bytes());
    file.extend_from_slice(&pcm);
    file.extend(std::iter::repeat_n(0u8, pad));
    let total = file.len() as u64;
    file[size_pos..size_pos + 8].copy_from_slice(&total.to_le_bytes());
    let d = crate::decode_bytes(&file, crate::DecodeOptions::unbounded())?;
    assert_eq!(d.frames(), 4); // 6 payload bytes + 2 pad zeros
    Ok(())
}

#[test]
fn w64_data_before_fmt_decodes() -> Result<()> {
    let mut pcm = Vec::new();
    for s in [10i16, -10, 20] {
        pcm.extend_from_slice(&s.to_le_bytes());
    }
    let mut fmt = le_pcm16_fmt();
    let mut file = W64_GUID_RIFF.to_vec();
    let size_pos = file.len();
    file.extend_from_slice(&0u64.to_le_bytes());
    file.extend_from_slice(&W64_GUID_WAVE);
    let pad = (8 - (pcm.len() % 8)) % 8;
    file.extend_from_slice(&W64_GUID_DATA);
    file.extend_from_slice(&(24u64 + pcm.len() as u64).to_le_bytes());
    file.extend_from_slice(&pcm);
    file.extend(std::iter::repeat_n(0u8, pad));
    let fmt_pad = (8 - (fmt.len() % 8)) % 8;
    fmt.extend(std::iter::repeat_n(0u8, fmt_pad));
    file.extend_from_slice(&W64_GUID_FMT);
    file.extend_from_slice(&(24u64 + fmt.len() as u64).to_le_bytes());
    file.extend_from_slice(&fmt);
    let total = file.len() as u64;
    file[size_pos..size_pos + 8].copy_from_slice(&total.to_le_bytes());
    let d = crate::decode_bytes(&file, crate::DecodeOptions::unbounded())?;
    assert_eq!(d.frames(), 3);
    Ok(())
}
