use super::*;
use crate::error::{FormatKind, WavError};
use crate::header::{ProbeCodec, SampleCodec};
use crate::source::ByteSource;
use crate::{DecodeOptions, decode_bytes, decode_streaming, decode_with, probe};
use std::io::Cursor;

fn wrap_gsm(raw: &[u8], extra: bool, block: u16, channels: u16) -> Vec<u8> {
    wrap_gsm_endian(raw, extra, block, channels, false)
}

fn wrap_gsm_rifx(raw: &[u8]) -> Vec<u8> {
    wrap_gsm_endian(raw, true, MS_BLOCK as u16, 1, true)
}

fn wrap_gsm_endian(raw: &[u8], extra: bool, block: u16, channels: u16, be: bool) -> Vec<u8> {
    let data_len = raw.len() as u32;
    let fmt_len = if extra { 20u32 } else { 16u32 };
    let pad = data_len % 2;
    let riff_len = 4 + 8 + fmt_len + 8 + data_len + pad;
    let rate = RATE;
    let byte_rate = 1_625u32;
    let mut w = Vec::with_capacity((riff_len as usize) + 8);
    let u16b = |v: u16| if be { v.to_be_bytes() } else { v.to_le_bytes() };
    let u32b = |v: u32| if be { v.to_be_bytes() } else { v.to_le_bytes() };
    w.extend_from_slice(if be { b"RIFX" } else { b"RIFF" });
    w.extend_from_slice(&u32b(riff_len));
    w.extend_from_slice(b"WAVE");
    w.extend_from_slice(b"fmt ");
    w.extend_from_slice(&u32b(fmt_len));
    w.extend_from_slice(&u16b(0x0031));
    w.extend_from_slice(&u16b(channels));
    w.extend_from_slice(&u32b(rate));
    w.extend_from_slice(&u32b(byte_rate));
    w.extend_from_slice(&u16b(block));
    w.extend_from_slice(&u16b(0));
    if extra {
        w.extend_from_slice(&u16b(2));
        w.extend_from_slice(&u16b(MS_SAMPLES as u16));
    }
    w.extend_from_slice(b"data");
    w.extend_from_slice(&u32b(data_len));
    w.extend_from_slice(raw);
    if pad == 1 {
        w.push(0);
    }
    w
}

fn pattern_blocks(n: usize) -> Vec<u8> {
    (0..n * MS_BLOCK)
        .map(|i| (i as u8).wrapping_mul(13).wrapping_add(7))
        .collect()
}

#[test]
fn probe_and_pcm_frames() {
    assert_eq!(SampleCodec::Gsm.probe(), ProbeCodec::Gsm);
    assert!(!SampleCodec::Gsm.is_adpcm());
    assert_eq!(pcm_frames(0), 0);
    assert_eq!(pcm_frames(64), 0);
    assert_eq!(pcm_frames(MS_BLOCK as u64), MS_SAMPLES as u64);
    assert_eq!(pcm_frames((MS_BLOCK * 2) as u64), (MS_SAMPLES * 2) as u64);
    assert_eq!(pcm_frames_capped(130, Some(10)), 10);
    assert_eq!(pcm_frames_capped(130, Some(10_000)), 640);
}

#[test]
fn headerless_one_block() -> crate::Result<()> {
    let raw = [0u8; MS_BLOCK];
    let d = decode_gsm_mono(&raw)?;
    assert_eq!(d.sample_rate, RATE);
    assert_eq!(d.frames(), MS_SAMPLES);
    assert_eq!(d.num_channels(), 1);
    assert!(d.channels[0].iter().all(|s| s.is_finite()));
    Ok(())
}

#[test]
fn headerless_rejects() {
    assert!(matches!(decode_gsm_mono(&[]), Err(WavError::Empty)));
    assert!(matches!(decode_gsm_mono(&[0u8; 64]), Err(WavError::OddPcm)));
    assert!(matches!(
        decode_gsm(&[0u8; MS_BLOCK], 0, &DecodeOptions::speech()),
        Err(WavError::UnsupportedSampleRate { rate: 0, .. })
    ));
    assert!(matches!(
        decode_gsm(&[0u8; MS_BLOCK], 16_000, &DecodeOptions::speech()),
        Err(WavError::UnsupportedSampleRate { .. })
    ));
    let tiny = DecodeOptions::speech().with_max_duration_secs(0.0);
    assert!(matches!(
        decode_gsm(&[0u8; MS_BLOCK], RATE, &tiny),
        Err(WavError::TooLong { .. })
    ));
    let ram = DecodeOptions::speech().with_max_output_bytes(4);
    assert!(matches!(
        decode_gsm(&[0u8; MS_BLOCK], RATE, &ram),
        Err(WavError::OutputTooLarge { .. })
    ));
}

#[test]
fn empty_gsm_wave_is_empty() -> crate::Result<()> {
    let wav = wrap_gsm(&[], true, MS_BLOCK as u16, 1);
    let d = decode_bytes(&wav, DecodeOptions::unbounded())?;
    assert_eq!(d.num_channels(), 1);
    assert_eq!(d.frames(), 0);
    assert!(matches!(crate::decode_f32(&wav), Err(WavError::Empty)));
    Ok(())
}

#[test]
fn wave_matches_headerless() -> crate::Result<()> {
    let raw = pattern_blocks(2);
    let from_raw = decode_gsm_mono(&raw)?;
    for extra in [true, false] {
        let wav = wrap_gsm(&raw, extra, MS_BLOCK as u16, 1);
        let p = probe(&mut ByteSource::from_slice(&wav))?;
        assert_eq!(p.codec, ProbeCodec::Gsm);
        assert_eq!(p.sample_rate, RATE);
        assert_eq!(p.channels, 1);
        assert_eq!(p.sample_width, MS_BLOCK);
        assert_eq!(p.declared_frames, Some((MS_SAMPLES * 2) as u64));
        let from_wav = decode_bytes(&wav, DecodeOptions::speech())?;
        assert_eq!(from_wav.sample_rate, RATE);
        assert_eq!(from_raw.channels[0], from_wav.channels[0], "extra={extra}");
    }
    let zero_ba = wrap_gsm(&raw, true, 0, 1);
    let from_zero = decode_bytes(&zero_ba, DecodeOptions::speech())?;
    assert_eq!(from_raw.channels[0], from_zero.channels[0]);
    Ok(())
}

#[test]
fn rifx_matches_headerless() -> crate::Result<()> {
    let raw = pattern_blocks(1);
    let from_raw = decode_gsm_mono(&raw)?;
    let wav = wrap_gsm_rifx(&raw);
    let p = probe(&mut ByteSource::from_slice(&wav))?;
    assert_eq!(p.codec, ProbeCodec::Gsm);
    assert_eq!(p.sample_rate, RATE);
    let from_wav = decode_bytes(&wav, DecodeOptions::default())?;
    assert_eq!(from_raw.channels[0], from_wav.channels[0]);
    Ok(())
}

#[test]
fn wave_rejects_layout() {
    let raw = [0u8; MS_BLOCK];
    let stereo = wrap_gsm(&raw, true, MS_BLOCK as u16, 2);
    assert!(matches!(
        decode_bytes(&stereo, DecodeOptions::speech()),
        Err(WavError::Format(FormatKind::ChannelLayout))
    ));
    let bad_ba = wrap_gsm(&raw, true, 64, 1);
    assert!(matches!(
        decode_bytes(&bad_ba, DecodeOptions::speech()),
        Err(WavError::Format(FormatKind::InvalidSize))
    ));
}

#[test]
fn wave_cursor_and_streaming() -> crate::Result<()> {
    let raw = pattern_blocks(3);
    let wav = wrap_gsm(&raw, true, MS_BLOCK as u16, 1);
    let mix = decode_bytes(&wav, DecodeOptions::speech())?;
    assert_eq!(mix.frames(), MS_SAMPLES * 3);
    assert_eq!(mix.num_channels(), 1);

    let n = wav.len() as u64;
    let mut cur = ByteSource::from_read_seek(Cursor::new(wav.clone()), Some(n));
    let cursor = decode_with(&mut cur, &DecodeOptions::speech())?;
    assert_eq!(cursor.channels[0], mix.channels[0]);

    let mut src = ByteSource::from_slice(&wav);
    let mut frames = 0usize;
    let info = decode_streaming(&mut src, &DecodeOptions::speech(), |b| {
        frames += b.frames;
        assert_eq!(b.planar.len(), 1);
        Ok(())
    })?;
    assert_eq!(info.sample_rate, RATE);
    assert_eq!(info.frames, MS_SAMPLES * 3);
    assert_eq!(frames, MS_SAMPLES * 3);
    Ok(())
}

#[test]
fn wave_leftover_byte_dropped() -> crate::Result<()> {
    let mut raw = pattern_blocks(1);
    raw.push(0xFF);
    let wav = wrap_gsm(&raw, true, MS_BLOCK as u16, 1);
    let d = decode_bytes(&wav, DecodeOptions::speech())?;
    assert_eq!(d.frames(), MS_SAMPLES);
    Ok(())
}

#[test]
fn ffmpeg_gsm_ms_wav_matches() -> crate::Result<()> {
    if !ffmpeg_available() {
        eprintln!("ffmpeg not available, skipping gsm oracle");
        return Ok(());
    }
    let raw = pattern_blocks(4);
    let wav = wrap_gsm(&raw, true, MS_BLOCK as u16, 1);
    let dir = tempfile::TempDir::new()?;
    let wav_path = dir.path().join("gsm.wav");
    let pcm_path = dir.path().join("gsm.f32");
    std::fs::write(&wav_path, &wav)?;
    let dec = std::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-i"])
        .arg(&wav_path)
        .args(["-f", "f32le", "-y"])
        .arg(&pcm_path)
        .status()?;
    assert!(dec.success(), "ffmpeg gsm_ms decode failed");
    let raw_pcm = std::fs::read(&pcm_path)?;
    let ff: Vec<f32> = raw_pcm
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| f32::from_le_bytes(*c))
        .collect();
    let own = decode_bytes(&wav, DecodeOptions::unbounded())?;
    assert_eq!(own.sample_rate, RATE);
    assert_eq!(own.num_channels(), 1);
    assert_eq!(own.frames(), ff.len());
    for (i, (a, b)) in own.channels[0].iter().zip(ff.iter()).enumerate() {
        assert_eq!(
            a.to_bits(),
            b.to_bits(),
            "gsm ffmpeg mismatch at {i}: {a} vs {b}"
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
