use super::*;
use crate::error::{Result, WavError};
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
    assert_eq!(SampleCodec::Unsupported.probe(), ProbeCodec::Unsupported);
    assert!(SampleCodec::MsAdpcm.is_adpcm());
    assert!(SampleCodec::ImaAdpcm.is_adpcm());
    assert!(!SampleCodec::S16.is_adpcm());
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
        .ok_or_else(|| crate::WavError::format("expected a fixed channel mask"))?;
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

fn riff(fmt: &[u8], data: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(b"fmt ");
    body.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
    body.extend_from_slice(fmt);
    if fmt.len() % 2 == 1 {
        body.push(0);
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

fn le_pcm16_fmt() -> Vec<u8> {
    let mut f = Vec::new();
    f.extend_from_slice(&1u16.to_le_bytes());
    f.extend_from_slice(&1u16.to_le_bytes());
    f.extend_from_slice(&16_000u32.to_le_bytes());
    f.extend_from_slice(&32_000u32.to_le_bytes());
    f.extend_from_slice(&2u16.to_le_bytes());
    f.extend_from_slice(&16u16.to_le_bytes());
    f
}

fn parse_ok(bytes: &[u8]) -> Result<WavHeader> {
    parse_header(&mut ByteSource::from_slice(bytes))
}
fn parse_err(bytes: &[u8]) -> bool {
    parse_header(&mut ByteSource::from_slice(bytes)).is_err()
}

fn ieee_fmt(chunk_pad: &[u8], bits: u16, block: u16, ch: u16) -> Vec<u8> {
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

fn g711_fmt(tag: u16, extra: &[u8]) -> Vec<u8> {
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

fn ext_fmt(guid: [u8; 16], bits: u16, valid: u16, mask: u32, ch: u16, extra_cb: u16) -> Vec<u8> {
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
    assert_eq!(container_width(3, 2, 16).unwrap_or(0), 2); // not multiple → packed
    assert_eq!(container_width(16, 1, 16).unwrap_or(0), 2); // w>8 → packed
    assert!(pcm_codec_for(12, 2).is_err());
    assert!(pcm_codec_for(16, 1).is_err());
}

#[test]
fn channel_mask_expand_overflow_and_unfixable() {
    assert_eq!(fix_wave_channel_mask(0, 32), Some(u32::MAX));
    assert!(fix_wave_channel_mask(1u32 << 31, 3).is_none());
}

#[test]
fn fmt_pcm_ieee_g711_error_matrix() -> Result<()> {
    assert!(parse_err(&riff(&[0u8; 8], &[0, 1]))); // fmt < 16
    let mut bad_pcm = le_pcm16_fmt();
    bad_pcm.extend_from_slice(&[0u8; 4]); // len 20, not 16/18/40
    assert!(parse_err(&riff(&bad_pcm, &[0, 1, 2, 3])));

    let ieee40 = ieee_fmt(&[0u8; 24], 32, 4, 1);
    assert_eq!(
        parse_ok(&riff(&ieee40, &[0u8; 8]))?.fmt.codec,
        SampleCodec::F32
    );
    let ieee_wild = ieee_fmt(&[0u8; 8], 32, 4, 1); // 16+8 > 18
    assert_eq!(
        parse_ok(&riff(&ieee_wild, &[0u8; 8]))?.fmt.codec,
        SampleCodec::F32
    );
    assert!(parse_err(&riff(&ieee_fmt(&[0], 32, 4, 1), &[0u8; 8]))); // len 17
    let mut ieee18_bad = ieee_fmt(&0u16.to_le_bytes(), 32, 4, 1);
    ieee18_bad[16] = 1; // cbSize != 0
    assert!(parse_err(&riff(&ieee18_bad, &[0u8; 8])));
    assert!(parse_err(&riff(&ieee_fmt(&[], 16, 2, 1), &[0u8; 4]))); // bits not 32/64
    let padded_f32 = ieee_fmt(&[], 32, 8, 1); // width 8 >= 4
    assert_eq!(parse_ok(&riff(&padded_f32, &[0u8; 8]))?.fmt.sample_width, 8);
    let padded_f64 = ieee_fmt(&[], 64, 16, 1);
    assert_eq!(
        parse_ok(&riff(&padded_f64, &[0u8; 16]))?.fmt.codec,
        SampleCodec::F64
    );

    let mut alaw_extra = 4u16.to_le_bytes().to_vec();
    alaw_extra.extend_from_slice(&[9, 8, 7, 6, 1, 2]); // extra 4 + 2 surplus
    let alaw_skip = g711_fmt(WAVE_FORMAT_ALAW, &alaw_extra);
    assert_eq!(
        parse_ok(&riff(&alaw_skip, &[0xd5]))?.fmt.codec,
        SampleCodec::ALaw
    );
    let g711_17 = g711_fmt(WAVE_FORMAT_MULAW, &[0]);
    assert_eq!(
        parse_ok(&riff(&g711_17, &[0xff]))?.fmt.codec,
        SampleCodec::MuLaw
    );
    Ok(())
}

#[test]
fn fmt_adpcm_error_matrix() -> Result<()> {
    let mut bits = Vec::new();
    bits.extend_from_slice(&WAVE_FORMAT_ADPCM_MS.to_le_bytes());
    bits.extend_from_slice(&1u16.to_le_bytes());
    bits.extend_from_slice(&16_000u32.to_le_bytes());
    bits.extend_from_slice(&8_000u32.to_le_bytes());
    bits.extend_from_slice(&32u16.to_le_bytes());
    bits.extend_from_slice(&8u16.to_le_bytes()); // not 4
    bits.extend_from_slice(&32u16.to_le_bytes());
    assert!(parse_err(&riff(&bits, &[0u8; 32])));

    let short = {
        let mut f = Vec::new();
        f.extend_from_slice(&WAVE_FORMAT_ADPCM_IMA.to_le_bytes());
        f.extend_from_slice(&1u16.to_le_bytes());
        f.extend_from_slice(&16_000u32.to_le_bytes());
        f.extend_from_slice(&8_000u32.to_le_bytes());
        f.extend_from_slice(&32u16.to_le_bytes());
        f.extend_from_slice(&4u16.to_le_bytes());
        f // no extra, len 16 < 20
    };
    assert!(parse_err(&riff(&short, &[0u8; 32])));

    fn adpcm(tag: u16, ch: u16, block: u16, extra: u16, rest: &[u8]) -> Vec<u8> {
        let mut f = Vec::new();
        f.extend_from_slice(&tag.to_le_bytes());
        f.extend_from_slice(&ch.to_le_bytes());
        f.extend_from_slice(&16_000u32.to_le_bytes());
        f.extend_from_slice(&8_000u32.to_le_bytes());
        f.extend_from_slice(&block.to_le_bytes());
        f.extend_from_slice(&4u16.to_le_bytes());
        f.extend_from_slice(&extra.to_le_bytes());
        f.extend_from_slice(rest);
        f
    }
    assert!(parse_err(&riff(
        &adpcm(WAVE_FORMAT_ADPCM_MS, 3, 32, 32, &[0u8; 32]),
        &[0u8; 32]
    )));
    assert!(parse_err(&riff(
        &adpcm(WAVE_FORMAT_ADPCM_MS, 1, 0, 32, &[0u8; 32]),
        &[0u8; 32]
    )));
    assert!(parse_err(&riff(
        &adpcm(WAVE_FORMAT_ADPCM_MS, 1, 32, 8, &[0u8; 8]),
        &[0u8; 32]
    )));
    let mut coef0 = 50u16.to_le_bytes().to_vec();
    coef0.extend_from_slice(&0u16.to_le_bytes());
    coef0.extend_from_slice(&[0u8; 28]);
    assert!(parse_err(&riff(
        &adpcm(WAVE_FORMAT_ADPCM_MS, 1, 32, 32, &coef0),
        &[0u8; 32]
    )));
    let mut truncated = 50u16.to_le_bytes().to_vec();
    truncated.extend_from_slice(&7u16.to_le_bytes());
    truncated.extend_from_slice(&[0u8; 8]); // not 28 coef bytes
    assert!(parse_err(&riff(
        &adpcm(WAVE_FORMAT_ADPCM_MS, 1, 32, 32, &truncated),
        &[0u8; 32]
    )));

    let mut padded = 50u16.to_le_bytes().to_vec();
    padded.extend_from_slice(&7u16.to_le_bytes());
    for _ in 0..7 {
        padded.extend_from_slice(&256i16.to_le_bytes());
        padded.extend_from_slice(&0i16.to_le_bytes());
    }
    padded.extend_from_slice(&[1, 2, 3, 4]);
    assert_eq!(
        parse_ok(&riff(
            &adpcm(WAVE_FORMAT_ADPCM_MS, 1, 32, 36, &padded),
            &[0u8; 32]
        ))?
        .fmt
        .codec,
        SampleCodec::MsAdpcm
    );
    assert!(parse_err(&riff(
        &adpcm(WAVE_FORMAT_ADPCM_IMA, 1, 32, 4, &50u16.to_le_bytes()),
        &[0u8; 32]
    )));
    Ok(())
}

#[test]
fn fmt_extensible_error_matrix() -> Result<()> {
    let pcm = KSDATAFORMAT_SUBTYPE_PCM;
    let mut short_ext = ext_fmt(pcm, 16, 16, 0, 1, 22);
    short_ext.truncate(30);
    assert!(parse_err(&riff(&short_ext, &[0, 1]))); // < 40
    assert!(parse_err(&riff(
        &ext_fmt(pcm, 16, 16, 0, 1, 10),
        &[0, 1, 2, 3]
    ))); // cb != 22
    assert!(parse_err(&riff(&ext_fmt(pcm, 12, 12, 0, 1, 22), &[0u8; 4]))); // bits % 8
    assert!(parse_err(&riff(&ext_fmt(pcm, 16, 24, 0, 1, 22), &[0u8; 4]))); // valid > container
    assert!(parse_err(&riff(&ext_fmt(pcm, 48, 48, 0, 1, 22), &[0u8; 8])));
    let ieee = KSDATAFORMAT_SUBTYPE_IEEE_FLOAT;
    assert!(parse_err(&riff(
        &ext_fmt(ieee, 32, 16, 0, 1, 22),
        &[0u8; 8]
    )));
    assert!(parse_err(&riff(
        &ext_fmt(ieee, 16, 16, 0, 1, 22),
        &[0u8; 4]
    )));
    let alaw = KSDATAFORMAT_SUBTYPE_ALAW;
    assert!(parse_err(&riff(&ext_fmt(alaw, 16, 16, 0, 1, 22), &[0])));
    let h = parse_ok(&riff(&ext_fmt(alaw, 8, 4, 0, 1, 22), &[0xd5]))?;
    assert_eq!(h.fmt.codec, SampleCodec::Unsupported);
    let mulaw = KSDATAFORMAT_SUBTYPE_MULAW;
    assert!(parse_err(&riff(&ext_fmt(mulaw, 16, 16, 0, 1, 22), &[0])));
    let h = parse_ok(&riff(&ext_fmt(mulaw, 8, 3, 0, 1, 22), &[0xff]))?;
    assert_eq!(h.fmt.codec, SampleCodec::Unsupported);
    assert!(parse_err(&riff(
        &ext_fmt([0u8; 16], 16, 16, 0, 1, 22),
        &[0, 1]
    )));
    assert!(parse_err(&riff(
        &ext_fmt(pcm, 16, 16, 1u32 << 19, 1, 22),
        &[0, 1]
    )));
    let h = parse_ok(&riff(&ext_fmt(pcm, 16, 16, 1u32 << 31, 3, 22), &[0u8; 12]))?;
    assert_eq!(h.fmt.channels, 3);
    let wav = riff(&ext_fmt(alaw, 8, 4, 0, 1, 22), &[0xd5]);
    assert!(matches!(
        crate::probe(&mut ByteSource::from_slice(&wav)),
        Err(WavError::UnsupportedCodec)
    ));
    Ok(())
}

#[test]
fn riff_walk_error_matrix() {
    let mut tiny = b"RIFF".to_vec();
    tiny.extend_from_slice(&2u32.to_le_bytes());
    tiny.extend_from_slice(b"WAVE");
    assert!(parse_err(&tiny));
    let mut avi = b"RIFF".to_vec();
    avi.extend_from_slice(&4u32.to_le_bytes());
    avi.extend_from_slice(b"AVI ");
    assert!(parse_err(&avi));

    let mut fact_short = Vec::new();
    fact_short.extend_from_slice(b"RIFF");
    let fmt = le_pcm16_fmt();
    let mut body = Vec::new();
    body.extend_from_slice(b"fmt ");
    body.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
    body.extend_from_slice(&fmt);
    body.extend_from_slice(b"fact");
    body.extend_from_slice(&2u32.to_le_bytes());
    body.extend_from_slice(&[1, 2]);
    body.extend_from_slice(b"data");
    body.extend_from_slice(&4u32.to_le_bytes());
    body.extend_from_slice(&[0, 1, 2, 3]);
    fact_short.extend_from_slice(&(4 + body.len() as u32).to_le_bytes());
    fact_short.extend_from_slice(b"WAVE");
    fact_short.extend_from_slice(&body);
    assert!(parse_err(&fact_short));

    let mut list_short = riff(&le_pcm16_fmt(), &[0, 1, 2, 3]);
    if let Some(at) = list_short.windows(4).position(|w| w == b"data") {
        let mut list = b"LIST".to_vec();
        list.extend_from_slice(&2u32.to_le_bytes());
        list.extend_from_slice(&[0, 1]);
        list_short.splice(at..at, list);
    }
    let _ = parse_header(&mut ByteSource::from_slice(&list_short));
}

#[test]
fn rf64_ds64_and_sentinel_errors() {
    fn rf64_prefix() -> Vec<u8> {
        let mut v = b"RF64".to_vec();
        v.extend_from_slice(&u32::MAX.to_le_bytes());
        v.extend_from_slice(b"WAVE");
        v
    }
    let mut bad = rf64_prefix();
    bad.extend_from_slice(b"ds64");
    bad.extend_from_slice(&8u32.to_le_bytes());
    bad.extend_from_slice(&[0u8; 8]);
    assert!(parse_err(&bad));

    let mut table = rf64_prefix();
    table.extend_from_slice(b"ds64");
    // 28 + 12 table entry + 4 surplus
    table.extend_from_slice(&44u32.to_le_bytes());
    table.extend_from_slice(&100u64.to_le_bytes()); // riffSize
    table.extend_from_slice(&8u64.to_le_bytes()); // dataSize
    table.extend_from_slice(&4u64.to_le_bytes()); // sampleCount
    table.extend_from_slice(&1u32.to_le_bytes()); // tableLength
    table.extend_from_slice(&[0u8; 12]);
    table.extend_from_slice(&[9, 8, 7, 6]);
    let fmt = le_pcm16_fmt();
    table.extend_from_slice(b"fmt ");
    table.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
    table.extend_from_slice(&fmt);
    table.extend_from_slice(b"data");
    table.extend_from_slice(&u32::MAX.to_le_bytes());
    table.extend_from_slice(&[0u8; 8]);
    assert!(parse_ok(&table).is_ok());

    let mut unk = rf64_prefix();
    // valid ds64 28
    unk.extend_from_slice(b"ds64");
    unk.extend_from_slice(&28u32.to_le_bytes());
    unk.extend_from_slice(&80u64.to_le_bytes());
    unk.extend_from_slice(&4u64.to_le_bytes());
    unk.extend_from_slice(&0u64.to_le_bytes());
    unk.extend_from_slice(&0u32.to_le_bytes());
    unk.extend_from_slice(b"JUNK");
    unk.extend_from_slice(&u32::MAX.to_le_bytes());
    assert!(parse_err(&unk));
}

#[test]
fn w64_error_and_unknown_chunk() {
    let mut not_wave = W64_GUID_RIFF.to_vec();
    not_wave.extend_from_slice(&40u64.to_le_bytes());
    not_wave.extend_from_slice(&[0u8; 16]);
    assert!(parse_err(&not_wave));

    let mut tiny_chunk = W64_GUID_RIFF.to_vec();
    tiny_chunk.extend_from_slice(&64u64.to_le_bytes());
    tiny_chunk.extend_from_slice(&W64_GUID_WAVE);
    tiny_chunk.extend_from_slice(&[1u8; 16]);
    tiny_chunk.extend_from_slice(&8u64.to_le_bytes());
    assert!(parse_err(&tiny_chunk));

    let fmt = le_pcm16_fmt();
    let mut missing_data = W64_GUID_RIFF.to_vec();
    missing_data.extend_from_slice(&0u64.to_le_bytes());
    missing_data.extend_from_slice(&W64_GUID_WAVE);
    let pad = (8 - (fmt.len() % 8)) % 8;
    let chunk_size = 24u64 + fmt.len() as u64 + pad as u64;
    missing_data.extend_from_slice(&W64_GUID_FMT);
    missing_data.extend_from_slice(&chunk_size.to_le_bytes());
    missing_data.extend_from_slice(&fmt);
    missing_data.extend(std::iter::repeat_n(0u8, pad));
    let total = missing_data.len() as u64;
    missing_data[16..24].copy_from_slice(&total.to_le_bytes());
    assert!(parse_err(&missing_data));

    let mut junk = W64_GUID_RIFF.to_vec();
    junk.extend_from_slice(&0u64.to_le_bytes());
    junk.extend_from_slice(&W64_GUID_WAVE);
    let junk_guid = [0x11u8; 16];
    let jbody = [7u8; 8];
    junk.extend_from_slice(&junk_guid);
    junk.extend_from_slice(&32u64.to_le_bytes());
    junk.extend_from_slice(&jbody);
    junk.extend_from_slice(&W64_GUID_FMT);
    junk.extend_from_slice(&chunk_size.to_le_bytes());
    junk.extend_from_slice(&fmt);
    junk.extend(std::iter::repeat_n(0u8, pad));
    let payload = [0u8, 1, 2, 3];
    let dpad = (8 - (payload.len() % 8)) % 8;
    junk.extend_from_slice(&W64_GUID_DATA);
    junk.extend_from_slice(&(24u64 + payload.len() as u64 + dpad as u64).to_le_bytes());
    junk.extend_from_slice(&payload);
    junk.extend(std::iter::repeat_n(0u8, dpad));
    let total = junk.len() as u64;
    junk[16..24].copy_from_slice(&total.to_le_bytes());
    assert!(parse_ok(&junk).is_ok());

    let mut fact_bad = W64_GUID_RIFF.to_vec();
    fact_bad.extend_from_slice(&80u64.to_le_bytes());
    fact_bad.extend_from_slice(&W64_GUID_WAVE);
    fact_bad.extend_from_slice(&W64_GUID_FACT);
    fact_bad.extend_from_slice(&24u64.to_le_bytes());
    assert!(parse_err(&fact_bad));
}
