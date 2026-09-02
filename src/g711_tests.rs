use super::*;
use crate::error::WavError;
use crate::source::ByteSource;
use crate::{ChannelMode, DecodeOptions, decode_bytes, decode_streaming, decode_with};

fn wrap_g711(raw: &[u8], alaw: bool, rate: u32, channels: u16) -> Vec<u8> {
    let tag: u16 = if alaw { 6 } else { 7 };
    let data_len = raw.len() as u32;
    let riff_len = 36u32 + data_len;
    let block = channels;
    let byte_rate = rate.saturating_mul(u32::from(block));
    let mut w = Vec::with_capacity(44 + raw.len());
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&riff_len.to_le_bytes());
    w.extend_from_slice(b"WAVE");
    w.extend_from_slice(b"fmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&tag.to_le_bytes());
    w.extend_from_slice(&channels.to_le_bytes());
    w.extend_from_slice(&rate.to_le_bytes());
    w.extend_from_slice(&byte_rate.to_le_bytes());
    w.extend_from_slice(&block.to_le_bytes());
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
    let wav = wrap_g711(&raw, false, 8_000, 1);
    let from_wav = decode_bytes(&wav, DecodeOptions::speech())?;
    assert_eq!(from_raw.channels[0], from_wav.channels[0]);

    let from_a = decode_g711_alaw(&raw, 16_000)?;
    let wav_a = wrap_g711(&raw, true, 16_000, 1);
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
    let ram = DecodeOptions::speech().with_max_output_bytes(4);
    assert!(matches!(
        decode_g711(&[0xff; 8], G711Law::ALaw, 8_000, 1, &ram),
        Err(WavError::OutputTooLarge { .. })
    ));
    Ok(())
}

#[test]
fn wave_collect_mix_split_and_cursor() -> crate::Result<()> {
    use std::io::Cursor;

    let raw: Vec<u8> = (0u8..=255).collect();
    let mono = wrap_g711(&raw, false, 8_000, 1);
    let mut cur = ByteSource::from_read_seek(Cursor::new(mono.clone()), Some(mono.len() as u64));
    let from_cur = decode_with(&mut cur, &DecodeOptions::speech())?;
    assert_eq!(from_cur.frames(), 256);

    let stereo: Vec<u8> = raw.iter().flat_map(|&b| [b, b ^ 0x80]).collect();
    let wav = wrap_g711(&stereo, false, 8_000, 2);
    let mix = decode_bytes(&wav, DecodeOptions::speech())?;
    assert_eq!(mix.num_channels(), 1);
    assert_eq!(mix.frames(), 256);

    let opts = DecodeOptions::speech().with_channel_mode(ChannelMode::Split);
    let split = decode_bytes(&wav, opts.clone())?;
    assert_eq!(split.num_channels(), 2);
    assert_eq!(split.frames(), 256);

    let mut cur = ByteSource::from_read_seek(Cursor::new(wav.clone()), Some(wav.len() as u64));
    let cursor_mix = decode_with(&mut cur, &DecodeOptions::speech())?;
    assert_eq!(cursor_mix.channels[0], mix.channels[0]);

    let n = wav.len() as u64;
    let mut cur = ByteSource::from_read_seek(Cursor::new(wav), Some(n));
    let cursor_split = decode_with(&mut cur, &opts)?;
    assert_eq!(cursor_split.channels, split.channels);

    let alaw = wrap_g711(&stereo, true, 16_000, 2);
    let a = decode_bytes(&alaw, DecodeOptions::speech())?;
    assert_eq!(a.sample_rate, 16_000);
    assert_eq!(a.frames(), 256);
    Ok(())
}

#[test]
fn wave_streaming_mono_and_split() -> crate::Result<()> {
    let raw: Vec<u8> = (0u8..=255).collect();
    let wav = wrap_g711(&raw, false, 8_000, 1);
    let mut src = ByteSource::from_slice(&wav);
    let mut frames = 0usize;
    let info = decode_streaming(&mut src, &DecodeOptions::speech(), |b| {
        frames += b.frames;
        assert_eq!(b.planar.len(), 1);
        Ok(())
    })?;
    assert_eq!(info.frames, 256);
    assert_eq!(frames, 256);

    let stereo: Vec<u8> = raw.iter().flat_map(|&b| [b, !b]).collect();
    let wav = wrap_g711(&stereo, true, 8_000, 2);
    let opts = DecodeOptions::speech().with_channel_mode(ChannelMode::Split);
    let mut src = ByteSource::from_slice(&wav);
    let mut n_ch = 0usize;
    let info = decode_streaming(&mut src, &opts, |b| {
        n_ch = b.planar.len();
        Ok(())
    })?;
    assert_eq!(info.frames, 256);
    assert_eq!(n_ch, 2);

    let mut src = ByteSource::from_slice(&wav);
    let mix = decode_streaming(&mut src, &DecodeOptions::speech(), |b| {
        assert_eq!(b.planar.len(), 1);
        Ok(())
    })?;
    assert_eq!(mix.frames, 256);

    use std::io::Cursor;
    let mut cur = ByteSource::from_read_seek(Cursor::new(wav.clone()), Some(wav.len() as u64));
    let info = decode_streaming(&mut cur, &opts, |_| Ok(()))?;
    assert_eq!(info.frames, 256);
    Ok(())
}
