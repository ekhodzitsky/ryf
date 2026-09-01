use super::{encode_f32, encode_s16, write_s16};
use crate::convert::f32_to_s16le;
use crate::error::{Result, WavError};
use crate::{ChannelMode, DecodeOptions, decode_bytes};

#[test]
fn encode_s16_riff_header() -> Result<()> {
    let wav = encode_s16(&f32_to_s16le(&[0.1]), 16_000)?;
    assert_eq!(&wav[..4], b"RIFF");
    assert_eq!(&wav[8..12], b"WAVE");
    Ok(())
}

#[test]
fn encode_s16_rejects_odd_empty_zero_rate() -> Result<()> {
    assert!(matches!(encode_s16(&[0], 16_000), Err(WavError::OddPcm)));
    let empty = encode_s16(&[], 16_000)?;
    assert_eq!(&empty[..4], b"RIFF");
    assert_eq!(&empty[40..44], &[0, 0, 0, 0]);
    assert!(matches!(
        encode_s16(&f32_to_s16le(&[0.1]), 0),
        Err(WavError::UnsupportedSampleRate { rate: 0, .. })
    ));
    Ok(())
}

#[test]
fn encode_f32_rejects_bad_channels_empty_odd_zero_rate() {
    assert!(matches!(
        encode_f32(&[0.1], 16_000, 0),
        Err(WavError::UnsupportedCodec)
    ));
    assert!(matches!(
        encode_f32(&[0.1], 16_000, 3),
        Err(WavError::UnsupportedCodec)
    ));
    assert!(encode_f32(&[], 16_000, 1).is_ok());
    assert!(matches!(
        encode_f32(&[0.1], 16_000, 2),
        Err(WavError::OddPcm)
    ));
    assert!(matches!(
        encode_f32(&[0.1], 0, 1),
        Err(WavError::UnsupportedSampleRate { rate: 0, .. })
    ));
}

#[test]
fn encode_rejects_byte_rate_overflow() {
    assert!(matches!(
        encode_s16(&f32_to_s16le(&[0.1]), u32::MAX),
        Err(WavError::RiffTooLarge)
    ));
    assert!(matches!(
        encode_f32(&[0.1], u32::MAX, 1),
        Err(WavError::RiffTooLarge)
    ));
}

#[test]
fn encode_f32_has_fact_and_roundtrips() -> Result<()> {
    let samples = [0.0f32, 0.5, -0.5];
    let wav = encode_f32(&samples, 24_000, 1)?;
    assert!(wav.windows(4).any(|w| w == b"fact"));
    assert_eq!(&wav[16..20], &[18, 0, 0, 0]);
    let decoded = decode_bytes(
        &wav,
        DecodeOptions::unbounded().with_channel_mode(ChannelMode::Mono),
    )?;
    assert_eq!(decoded.sample_rate, 24_000);
    assert_eq!(decoded.channels[0].len(), 3);
    for (g, e) in decoded.channels[0].iter().zip(samples.iter()) {
        assert_eq!(g.to_bits(), e.to_bits());
    }
    Ok(())
}

#[test]
fn write_s16_roundtrip_bytes() -> Result<()> {
    let path = std::env::temp_dir().join(format!("ryf-enc-w-{}.wav", std::process::id()));
    let pcm = f32_to_s16le(&[0.5, -0.5]);
    write_s16(&path, &pcm, 16_000)?;
    let bytes = std::fs::read(&path)?;
    let _ = std::fs::remove_file(&path);
    assert_eq!(bytes, encode_s16(&pcm, 16_000)?);
    Ok(())
}

#[test]
fn write_s16_io_error() {
    let dir = std::env::temp_dir().join(format!("ryf-enc-notdir-{}", std::process::id()));
    let err = write_s16(
        &dir.join("no").join("out.wav"),
        &f32_to_s16le(&[0.1]),
        16_000,
    );
    assert!(matches!(err, Err(WavError::Io(_))));
}

#[test]
fn encode_s16_payload_is_pcm_bytes() -> Result<()> {
    let pcm = f32_to_s16le(&[0.1, 0.2]);
    let wav = encode_s16(&pcm, 16_000)?;
    assert_eq!(&wav[44..], pcm.as_slice());
    Ok(())
}

#[test]
fn f32_to_s16le_clips_and_s16le_to_f32_peak() -> Result<()> {
    let pcm = crate::f32_to_s16le(&[0.0, 1.0, -1.0, 2.0]);
    assert_eq!(i16::from_le_bytes([pcm[0], pcm[1]]), 0);
    assert_eq!(i16::from_le_bytes([pcm[2], pcm[3]]), 32_767);
    assert_eq!(i16::from_le_bytes([pcm[4], pcm[5]]), -32_767);
    assert_eq!(i16::from_le_bytes([pcm[6], pcm[7]]), 32_767);

    let f = crate::s16le_to_f32(&crate::f32_to_s16le(&[1.0]))?;
    assert!((f[0] - 1.0).abs() < 0.001);
    assert!(matches!(crate::s16le_to_f32(&[0]), Err(WavError::OddPcm)));

    let min = crate::s16le_to_f32(&i16::MIN.to_le_bytes())?;
    assert!((-1.001..-1.0).contains(&min[0]));
    Ok(())
}

#[test]
fn sniff_wav_slice_and_garbage() -> Result<()> {
    let wav = encode_s16(&f32_to_s16le(&[0.1]), 16_000)?;
    assert!(crate::sniff_wav(&wav));
    assert!(!crate::sniff_wav(b"not a wave"));
    assert!(!crate::sniff_wav(&[]));

    struct Boom;
    impl std::io::Read for Boom {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("boom"))
        }
    }
    impl std::io::Seek for Boom {
        fn seek(&mut self, _: std::io::SeekFrom) -> std::io::Result<u64> {
            Ok(0)
        }
    }
    let mut src = crate::ByteSource::from_read_seek(Boom, Some(40));
    assert!(matches!(
        crate::sniff_is_riff_wave(&mut src),
        Err(WavError::Io(_))
    ));
    Ok(())
}

#[test]
fn decode_s16_roundtrip_and_rejects() -> Result<()> {
    let pcm = f32_to_s16le(&[0.5, -0.5, 0.0]);
    let wav = encode_s16(&pcm, 16_000)?;
    let (sr, data) = crate::decode_s16(&wav)?;
    assert_eq!(sr, 16_000);
    assert_eq!(data, pcm);

    let empty = encode_s16(&[], 16_000)?;
    assert!(matches!(crate::decode_s16(&empty), Err(WavError::Empty)));

    let f = encode_f32(&[0.1], 16_000, 1)?;
    assert!(matches!(
        crate::decode_s16(&f),
        Err(WavError::UnsupportedCodec)
    ));

    let mut odd = wav;
    odd[40..44].copy_from_slice(&1u32.to_le_bytes());
    assert!(matches!(crate::decode_s16(&odd), Err(WavError::OddPcm)));
    Ok(())
}

#[test]
fn decode_f32_and_read_paths() -> Result<()> {
    let samples = [0.0f32, 0.25, -0.5];
    let wav = encode_f32(&samples, 24_000, 1)?;
    let (sr, mono) = crate::decode_f32(&wav)?;
    assert_eq!(sr, 24_000);
    assert_eq!(mono.len(), 3);
    for (g, e) in mono.iter().zip(samples.iter()) {
        assert_eq!(g.to_bits(), e.to_bits());
    }

    let empty = encode_f32(&[], 16_000, 1)?;
    assert!(matches!(crate::decode_f32(&empty), Err(WavError::Empty)));

    let path = std::env::temp_dir().join(format!("ryf-io-{}.wav", std::process::id()));
    let pcm = f32_to_s16le(&[0.1, 0.2]);
    write_s16(&path, &pcm, 16_000)?;
    let (sr, data) = crate::read_s16(&path)?;
    assert_eq!(sr, 16_000);
    assert_eq!(data, pcm);
    let (sr, f) = crate::read_f32(&path)?;
    assert_eq!(sr, 16_000);
    assert_eq!(f.len(), 2);
    let _ = std::fs::remove_file(&path);
    Ok(())
}
