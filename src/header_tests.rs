use super::*;
use crate::error::Result;
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
