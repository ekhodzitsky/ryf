use super::*;
use crate::error::WavError;
use crate::{ChannelMode, DecodeOptions};

fn wrap_g711(raw: &[u8], alaw: bool, rate: u32) -> Vec<u8> {
    let tag: u16 = if alaw { 6 } else { 7 };
    let data_len = raw.len() as u32;
    let riff_len = 36u32 + data_len;
    let mut w = Vec::with_capacity(44 + raw.len());
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&riff_len.to_le_bytes());
    w.extend_from_slice(b"WAVE");
    w.extend_from_slice(b"fmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&tag.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&rate.to_le_bytes());
    w.extend_from_slice(&rate.to_le_bytes()); // byte rate = rate * 1 * 1
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&8u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&data_len.to_le_bytes());
    w.extend_from_slice(raw);
    w
}

#[test]
fn headerless_matches_wave_and_formula() -> crate::Result<()> {
    let raw: Vec<u8> = (0u8..=255).collect();
    let from_raw = decode_g711_mulaw(&raw, 8_000)?;
    assert_eq!(from_raw.sample_rate, 8_000);
    assert_eq!(from_raw.frames(), 256);
    let wav = wrap_g711(&raw, false, 8_000);
    let from_wav = crate::decode_bytes(&wav, DecodeOptions::speech())?;
    assert_eq!(from_raw.channels[0], from_wav.channels[0]);

    let from_a = decode_g711_alaw(&raw, 16_000)?;
    let wav_a = wrap_g711(&raw, true, 16_000);
    let from_wav_a = crate::decode_bytes(&wav_a, DecodeOptions::speech())?;
    assert_eq!(from_a.channels[0], from_wav_a.channels[0]);
    Ok(())
}

#[test]
fn headerless_rejects_and_mixes() -> crate::Result<()> {
    assert!(matches!(
        decode_g711_mulaw(&[], 8_000),
        Err(WavError::Empty)
    ));
    assert!(matches!(
        decode_g711(
            &[0, 1, 2],
            G711Law::MuLaw,
            8_000,
            2,
            &DecodeOptions::speech()
        ),
        Err(WavError::OddPcm)
    ));
    assert!(matches!(
        decode_g711_mulaw(&[0xff], 0),
        Err(WavError::UnsupportedSampleRate { rate: 0, .. })
    ));
    assert!(matches!(
        decode_g711(&[0], G711Law::ALaw, 8_000, 0, &DecodeOptions::speech()),
        Err(WavError::UnsupportedCodec)
    ));

    let stereo = decode_g711(
        &[0xff, 0xff],
        G711Law::MuLaw,
        8_000,
        2,
        &DecodeOptions::speech(),
    )?;
    assert_eq!(stereo.num_channels(), 1);
    let split = decode_g711(
        &[0xff, 0x80],
        G711Law::MuLaw,
        8_000,
        2,
        &DecodeOptions::speech().with_channel_mode(ChannelMode::Split),
    )?;
    assert_eq!(split.num_channels(), 2);
    assert_eq!(split.frames(), 1);

    let tiny = DecodeOptions::speech().with_max_duration_secs(0.0);
    assert!(matches!(
        decode_g711(&[0xff; 16], G711Law::MuLaw, 8_000, 1, &tiny),
        Err(WavError::TooLong { .. })
    ));
    Ok(())
}
