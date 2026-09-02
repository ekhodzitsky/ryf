use super::*;
use crate::error::WavError;
use crate::header::ProbeCodec;
use crate::source::ByteSource;
use crate::{ChannelMode, DecodeOptions, decode_bytes, decode_streaming, decode_with, probe};

fn wrap_g722(raw: &[u8], tag: u16, rate: u32, channels: u16) -> Vec<u8> {
    let data_len = raw.len() as u32;
    let block = channels;
    let byte_rate = 8_000u32.saturating_mul(u32::from(block));
    let mut w = Vec::with_capacity(46 + raw.len());
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(38 + data_len).to_le_bytes());
    w.extend_from_slice(b"WAVE");
    w.extend_from_slice(b"fmt ");
    w.extend_from_slice(&18u32.to_le_bytes());
    w.extend_from_slice(&tag.to_le_bytes());
    w.extend_from_slice(&channels.to_le_bytes());
    w.extend_from_slice(&rate.to_le_bytes());
    w.extend_from_slice(&byte_rate.to_le_bytes());
    w.extend_from_slice(&block.to_le_bytes());
    w.extend_from_slice(&4u16.to_le_bytes());
    w.extend_from_slice(&0u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&data_len.to_le_bytes());
    w.extend_from_slice(raw);
    w
}

#[test]
fn headerless_two_samples_per_byte() -> crate::Result<()> {
    let raw = [0u8, 0x55, 0xaa, 0xff];
    let d = decode_g722_mono(&raw)?;
    assert_eq!(d.sample_rate, 16_000);
    assert_eq!(d.frames(), 8);
    assert_eq!(d.num_channels(), 1);
    assert!(d.channels[0].iter().all(|s| s.is_finite()));
    Ok(())
}

#[test]
fn headerless_sdp_8000_still_16k() -> crate::Result<()> {
    let d = decode_g722(&[0u8; 8], 8_000, 1, &DecodeOptions::speech())?;
    assert_eq!(d.sample_rate, 16_000);
    assert_eq!(d.frames(), 16);
    Ok(())
}

#[test]
fn headerless_rejects() {
    assert!(matches!(decode_g722_mono(&[]), Err(WavError::Empty)));
    assert!(matches!(
        decode_g722(&[0, 1, 2], 16_000, 2, &DecodeOptions::speech()),
        Err(WavError::OddPcm)
    ));
    assert!(matches!(
        decode_g722(&[0], 0, 1, &DecodeOptions::speech()),
        Err(WavError::UnsupportedSampleRate { rate: 0, .. })
    ));
    assert!(matches!(
        decode_g722(&[0], 12_000, 1, &DecodeOptions::speech()),
        Err(WavError::UnsupportedSampleRate { .. })
    ));
    assert!(matches!(
        decode_g722(&[0], 16_000, 0, &DecodeOptions::speech()),
        Err(WavError::UnsupportedCodec { .. })
    ));
    let tiny = DecodeOptions::speech().with_max_duration_secs(0.0);
    assert!(matches!(
        decode_g722(&[0xff; 16], 16_000, 1, &tiny),
        Err(WavError::TooLong { .. })
    ));
    let ram = DecodeOptions::speech().with_max_output_bytes(4);
    assert!(matches!(
        decode_g722(&[0xff; 8], 16_000, 1, &ram),
        Err(WavError::OutputTooLarge { .. })
    ));
}

fn wrap_g722_fmt16(raw: &[u8], tag: u16) -> Vec<u8> {
    let data_len = raw.len() as u32;
    let mut w = Vec::with_capacity(44 + raw.len());
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + data_len).to_le_bytes());
    w.extend_from_slice(b"WAVE");
    w.extend_from_slice(b"fmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&tag.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&16_000u32.to_le_bytes());
    w.extend_from_slice(&8_000u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&4u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&data_len.to_le_bytes());
    w.extend_from_slice(raw);
    w
}

#[test]
fn wave_tags_match_headerless() -> crate::Result<()> {
    let raw: Vec<u8> = (0u8..=63).collect();
    let from_raw = decode_g722_mono(&raw)?;
    for tag in [0x0064u16, 0x0065, 0x028F] {
        let wav = wrap_g722(&raw, tag, 16_000, 1);
        let p = probe(&mut ByteSource::from_slice(&wav))?;
        assert_eq!(p.codec, ProbeCodec::G722);
        assert_eq!(p.sample_rate, 16_000);
        let from_wav = decode_bytes(&wav, DecodeOptions::speech())?;
        assert_eq!(from_wav.sample_rate, 16_000);
        assert_eq!(from_raw.channels[0], from_wav.channels[0], "tag {tag:#06x}");
        assert_eq!(p.declared_frames, Some(128));
    }
    let fmt16 = wrap_g722_fmt16(&raw, 0x028F);
    let from_16 = decode_bytes(&fmt16, DecodeOptions::speech())?;
    assert_eq!(from_raw.channels[0], from_16.channels[0]);
    Ok(())
}

#[test]
fn headerless_stereo_matches_wave() -> crate::Result<()> {
    let raw: Vec<u8> = (0u8..=31).flat_map(|b| [b, b ^ 0x5a]).collect();
    let from_raw = decode_g722(&raw, 16_000, 2, &DecodeOptions::speech())?;
    let wav = wrap_g722(&raw, 0x0064, 16_000, 2);
    let from_wav = decode_bytes(&wav, DecodeOptions::speech())?;
    assert_eq!(from_raw.channels[0], from_wav.channels[0]);
    let split_opts = DecodeOptions::speech().with_channel_mode(ChannelMode::Split);
    let split_raw = decode_g722(&raw, 16_000, 2, &split_opts)?;
    let split_wav = decode_bytes(&wav, split_opts)?;
    assert_eq!(split_raw.channels, split_wav.channels);
    Ok(())
}

#[test]
fn wave_mix_split_and_cursor() -> crate::Result<()> {
    use std::io::Cursor;

    let raw: Vec<u8> = (0u8..=31).collect();
    let stereo: Vec<u8> = raw.iter().flat_map(|&b| [b, b ^ 0x80]).collect();
    let wav = wrap_g722(&stereo, 0x028F, 16_000, 2);
    let mix = decode_bytes(&wav, DecodeOptions::speech())?;
    assert_eq!(mix.num_channels(), 1);
    assert_eq!(mix.frames(), 64);

    let opts = DecodeOptions::speech().with_channel_mode(ChannelMode::Split);
    let split = decode_bytes(&wav, opts.clone())?;
    assert_eq!(split.num_channels(), 2);
    assert_eq!(split.frames(), 64);

    let n = wav.len() as u64;
    let mut cur = ByteSource::from_read_seek(Cursor::new(wav.clone()), Some(n));
    let cursor_mix = decode_with(&mut cur, &DecodeOptions::speech())?;
    assert_eq!(cursor_mix.channels[0], mix.channels[0]);

    let mut cur = ByteSource::from_read_seek(Cursor::new(wav), Some(n));
    let cursor_split = decode_with(&mut cur, &opts)?;
    assert_eq!(cursor_split.channels, split.channels);
    Ok(())
}

#[test]
fn wave_streaming_and_sdp_header_rate() -> crate::Result<()> {
    let raw: Vec<u8> = (0u8..=31).collect();
    let wav = wrap_g722(&raw, 0x0064, 8_000, 1);
    let mut src = ByteSource::from_slice(&wav);
    let mut frames = 0usize;
    let info = decode_streaming(&mut src, &DecodeOptions::speech(), |b| {
        frames += b.frames;
        assert_eq!(b.planar.len(), 1);
        Ok(())
    })?;
    assert_eq!(info.sample_rate, 16_000);
    assert_eq!(info.frames, 64);
    assert_eq!(frames, 64);

    let stereo: Vec<u8> = raw.iter().flat_map(|&b| [b, !b]).collect();
    let wav = wrap_g722(&stereo, 0x028F, 16_000, 2);
    let opts = DecodeOptions::default().with_channel_mode(ChannelMode::Split);
    let mut src = ByteSource::from_slice(&wav);
    let mut n_ch = 0usize;
    let info = decode_streaming(&mut src, &opts, |b| {
        n_ch = b.planar.len();
        Ok(())
    })?;
    assert_eq!(info.frames, 64);
    assert_eq!(n_ch, 2);
    Ok(())
}

#[test]
fn ffmpeg_g722_wav_matches() -> crate::Result<()> {
    if !ffmpeg_available() {
        eprintln!("ffmpeg not available, skipping g722 oracle");
        return Ok(());
    }
    let dir = tempfile::TempDir::new()?;
    let wav_path = dir.path().join("g722.wav");
    let pcm_path = dir.path().join("g722.f32");
    let enc = std::process::Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "sine=f=440:r=16000:d=0.05",
            "-c:a",
            "g722",
            "-y",
        ])
        .arg(&wav_path)
        .status()?;
    assert!(enc.success(), "ffmpeg g722 encode failed");
    let dec = std::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-i"])
        .arg(&wav_path)
        .args(["-f", "f32le", "-y"])
        .arg(&pcm_path)
        .status()?;
    assert!(dec.success(), "ffmpeg g722 decode failed");
    let wav = std::fs::read(&wav_path)?;
    let raw = std::fs::read(&pcm_path)?;
    let ff: Vec<f32> = raw
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| f32::from_le_bytes(*c))
        .collect();
    let own = decode_bytes(&wav, DecodeOptions::unbounded())?;
    assert_eq!(own.sample_rate, 16_000);
    assert_eq!(own.num_channels(), 1);
    assert_eq!(own.frames(), ff.len());
    for (i, (a, b)) in own.channels[0].iter().zip(ff.iter()).enumerate() {
        assert_eq!(
            a.to_bits(),
            b.to_bits(),
            "g722 ffmpeg mismatch at {i}: {a} vs {b}"
        );
    }
    Ok(())
}

fn ffmpeg_available() -> bool {
    std::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
