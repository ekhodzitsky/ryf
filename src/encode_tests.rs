use super::{
    WavWriter, WriteFormat, WriteSpec, encode, encode_f32, encode_rf64, encode_s16, write,
    write_f32, write_rf64, write_s16,
};
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
        Err(WavError::UnsupportedCodec { .. })
    ));
    assert!(matches!(
        encode_f32(&[0.1], 16_000, 27),
        Err(WavError::UnsupportedCodec { .. })
    ));
    assert!(matches!(
        encode_f32(&[0.1], 16_000, 3),
        Err(WavError::OddPcm)
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
    assert_eq!(i16::from_le_bytes([pcm[4], pcm[5]]), i16::MIN);
    assert_eq!(i16::from_le_bytes([pcm[6], pcm[7]]), 32_767);

    let f = crate::s16le_to_f32(&crate::f32_to_s16le(&[1.0]))?;
    assert!((f[0] - 1.0).abs() < 0.001);
    assert!(matches!(crate::s16le_to_f32(&[0]), Err(WavError::OddPcm)));

    let min = crate::s16le_to_f32(&i16::MIN.to_le_bytes())?;
    assert_eq!(min[0], -1.0);
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
        Err(WavError::UnsupportedCodec { .. })
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

#[test]
fn encode_s16_stereo_and_write_spec() -> Result<()> {
    let mut pcm = Vec::new();
    for s in [1i16, -1, 2, -2] {
        pcm.extend_from_slice(&s.to_le_bytes());
    }
    let wav = encode(WriteSpec::s16(16_000, 2), &pcm)?;
    let decoded = decode_bytes(
        &wav,
        DecodeOptions::unbounded().with_channel_mode(ChannelMode::Split),
    )?;
    assert_eq!(decoded.channels.len(), 2);
    assert_eq!(decoded.channels[0].len(), 2);
    assert_eq!(WriteSpec::s16(16_000, 2).format, WriteFormat::S16);
    assert_eq!(WriteFormat::S24.bits(), 24);
    assert_eq!(WriteFormat::U8.bytes_per_sample(), 1);
    let tri = encode_f32(&[0.1, 0.2, 0.3], 16_000, 3)?;
    assert_eq!(
        decode_bytes(
            &tri,
            DecodeOptions::unbounded().with_channel_mode(ChannelMode::Split)
        )?
        .channels
        .len(),
        3
    );
    Ok(())
}

#[test]
fn encode_u8_s24_s32_roundtrip() -> Result<()> {
    let u8p = [0u8, 128, 255];
    let wav = encode(WriteSpec::u8(8_000, 1), &u8p)?;
    let (sr, mono) = crate::decode_f32(&wav)?;
    assert_eq!(sr, 8_000);
    assert_eq!(mono.len(), 3);

    let s24 = [0u8, 0, 0, 0x00, 0x00, 0x80]; // 0, min
    let wav = encode(WriteSpec::s24(16_000, 1), &s24)?;
    let (_, mono) = crate::decode_f32(&wav)?;
    assert_eq!(mono.len(), 2);

    let mut s32 = Vec::new();
    for v in [0i32, -1, i32::MAX] {
        s32.extend_from_slice(&v.to_le_bytes());
    }
    let wav = encode(WriteSpec::s32(16_000, 1), &s32)?;
    let (_, mono) = crate::decode_f32(&wav)?;
    assert_eq!(mono.len(), 3);
    Ok(())
}

#[test]
fn write_f32_file_roundtrip() -> Result<()> {
    let path = std::env::temp_dir().join(format!("ryf-enc-f32-{}.wav", std::process::id()));
    let samples = [0.0f32, 0.5, -0.25, 1.0];
    write_f32(&path, &samples, 24_000, 2)?;
    let bytes = std::fs::read(&path)?;
    let _ = std::fs::remove_file(&path);
    assert_eq!(bytes, encode_f32(&samples, 24_000, 2)?);
    assert_eq!(u16::from_le_bytes([bytes[22], bytes[23]]), 2);
    let mut src = crate::ByteSource::from_slice(&bytes);
    let decoded = crate::decode(&mut src, ChannelMode::Split, "write_f32")?;
    assert_eq!(decoded.channels.len(), 2);
    assert_eq!(decoded.channels[0].len(), 2);
    Ok(())
}

#[test]
fn encode_rejects_bad_spec_and_odd_frames() {
    assert!(matches!(
        encode(WriteSpec::s16(0, 1), &[0, 1]),
        Err(WavError::UnsupportedSampleRate { rate: 0, .. })
    ));
    assert!(matches!(
        encode(WriteSpec::s16(16_000, 0), &[]),
        Err(WavError::UnsupportedCodec { .. })
    ));
    assert!(matches!(
        encode(WriteSpec::s16(16_000, 27), &[]),
        Err(WavError::UnsupportedCodec { .. })
    ));
    assert!(matches!(
        encode(WriteSpec::s16(16_000, 2), &[0, 1]),
        Err(WavError::OddPcm)
    ));
    assert!(encode(WriteSpec::s16(16_000, 2), &[]).is_ok());
}

#[test]
fn wav_writer_chunks_finalize_and_drop() -> Result<()> {
    use std::io::Cursor;

    let pcm = f32_to_s16le(&[0.1, 0.2, 0.3, 0.4]);
    let spec = WriteSpec::s16(16_000, 1);
    let mut cur = Cursor::new(Vec::new());
    {
        let mut w = WavWriter::new(&mut cur, spec)?;
        w.write_pcm(&pcm[..4])?;
        w.write_pcm(&pcm[4..])?;
        assert_eq!(w.data_bytes(), pcm.len() as u64);
        w.finalize()?;
        w.finalize()?;
        assert!(w.write_pcm(&pcm).is_err());
    }
    let wav = cur.into_inner();
    assert_eq!(wav, encode_s16(&pcm, 16_000)?);

    let mut cur = Cursor::new(Vec::new());
    {
        let mut w = WavWriter::new(&mut cur, spec)?;
        w.write_pcm(&pcm)?;
    } // drop patches
    let wav = cur.into_inner();
    assert_eq!(wav, encode_s16(&pcm, 16_000)?);

    let mut w = WavWriter::new(Cursor::new(Vec::new()), WriteSpec::s16(16_000, 1))?;
    assert!(matches!(w.write_pcm(&[0]), Err(WavError::OddPcm)));
    assert!(matches!(
        w.write_f32_samples(&[0.1]),
        Err(WavError::UnsupportedCodec { .. })
    ));

    let samples = [0.25f32, -0.5];
    let mut cur = Cursor::new(Vec::new());
    {
        let mut w = WavWriter::new(&mut cur, WriteSpec::f32(12_000, 1))?;
        w.write_f32_samples(&samples)?;
        w.finalize()?;
    }
    let wav = cur.into_inner();
    assert_eq!(wav, encode_f32(&samples, 12_000, 1)?);
    Ok(())
}

#[test]
fn write_generic_matches_encode() -> Result<()> {
    let path = std::env::temp_dir().join(format!("ryf-enc-g-{}.wav", std::process::id()));
    let pcm = f32_to_s16le(&[0.1]);
    write(&path, WriteSpec::s16(8_000, 1), &pcm)?;
    let bytes = std::fs::read(&path)?;
    let _ = std::fs::remove_file(&path);
    assert_eq!(bytes, encode(WriteSpec::s16(8_000, 1), &pcm)?);
    Ok(())
}

#[test]
fn encode_rf64_roundtrips_pcm_and_float() -> Result<()> {
    let pcm = f32_to_s16le(&[0.1, -0.2, 0.0]);
    let wav = encode_rf64(WriteSpec::s16(16_000, 1), &pcm)?;
    assert_eq!(&wav[..4], b"RF64");
    assert!(wav.windows(4).any(|w| w == b"ds64"));
    assert_eq!(&wav[4..8], &u32::MAX.to_le_bytes());
    let decoded = decode_bytes(
        &wav,
        DecodeOptions::unbounded().with_channel_mode(ChannelMode::Mono),
    )?;
    assert_eq!(decoded.sample_rate, 16_000);
    assert_eq!(decoded.frames(), 3);

    let samples = [0.0f32, 0.5, -0.25];
    let f = encode_rf64(WriteSpec::f32(24_000, 1), &{
        let mut b = Vec::new();
        for s in samples {
            b.extend_from_slice(&s.to_le_bytes());
        }
        b
    })?;
    assert_eq!(&f[..4], b"RF64");
    let got = decode_bytes(
        &f,
        DecodeOptions::unbounded().with_channel_mode(ChannelMode::Mono),
    )?;
    assert_eq!(got.sample_rate, 24_000);
    for (g, e) in got.channels[0].iter().zip(samples.iter()) {
        assert_eq!(g.to_bits(), e.to_bits());
    }

    let path = std::env::temp_dir().join(format!("ryf-rf64-{}.wav", std::process::id()));
    write_rf64(&path, WriteSpec::s16(8_000, 1), &pcm)?;
    let on_disk = std::fs::read(&path)?;
    let _ = std::fs::remove_file(&path);
    assert_eq!(on_disk, encode_rf64(WriteSpec::s16(8_000, 1), &pcm)?);

    let small = encode_s16(&pcm, 16_000)?;
    assert_eq!(&small[..4], b"RIFF");
    Ok(())
}

#[test]
fn wav_writer_promotes_riff_to_rf64() -> Result<()> {
    use std::io::Cursor;

    let pcm = f32_to_s16le(&[0.1, -0.2, 0.0]);
    let spec = WriteSpec::s16(16_000, 1);
    let mut cur = Cursor::new(Vec::new());
    {
        let mut w = WavWriter::new(&mut cur, spec)?;
        w.write_pcm(&pcm)?;
        w.force_rf64()?;
        w.finalize()?;
    }
    let wav = cur.into_inner();
    assert_eq!(&wav[..4], b"RF64");
    let decoded = crate::decode_bytes(&wav, crate::DecodeOptions::default())?;
    assert_eq!(decoded.frames(), 3);
    Ok(())
}

#[test]
fn wav_writer_rf64_matches_encode_rf64() -> Result<()> {
    use std::io::Cursor;

    let pcm = f32_to_s16le(&[0.25, -0.5]);
    let spec = WriteSpec::s16(16_000, 1);
    let mut cur = Cursor::new(Vec::new());
    {
        let mut w = WavWriter::new_rf64(&mut cur, spec)?;
        w.write_pcm(&pcm)?;
        w.finalize()?;
    }
    let wav = cur.into_inner();
    assert_eq!(wav, encode_rf64(spec, &pcm)?);

    let samples = [0.5f32, -0.25];
    let mut pcm_f = Vec::new();
    for s in samples {
        pcm_f.extend_from_slice(&s.to_le_bytes());
    }
    let mut cur = Cursor::new(Vec::new());
    {
        let mut w = WavWriter::new_rf64(&mut cur, WriteSpec::f32(12_000, 1))?;
        w.write_f32_samples(&samples)?;
        w.finalize()?;
    }
    assert_eq!(
        cur.into_inner(),
        encode_rf64(WriteSpec::f32(12_000, 1), &pcm_f)?
    );
    Ok(())
}

#[test]
fn encode_rifx_s16_roundtrips() -> Result<()> {
    let pcm = f32_to_s16le(&[0.25, -0.5, 0.0]);
    let wav = crate::encode_rifx(WriteSpec::s16(16_000, 1), &pcm)?;
    assert_eq!(&wav[..4], b"RIFX");
    let d = decode_bytes(&wav, DecodeOptions::unbounded())?;
    assert_eq!(d.sample_rate, 16_000);
    assert_eq!(d.frames(), 3);
    let le = encode_s16(&pcm, 16_000)?;
    let dle = decode_bytes(&le, DecodeOptions::unbounded())?;
    assert_eq!(d.channels[0], dle.channels[0]);
    Ok(())
}

#[test]
fn encode_extensible_pcm_roundtrips() -> Result<()> {
    let pcm = f32_to_s16le(&[0.1, -0.1, 0.2, -0.2]);
    let wav = crate::encode_extensible(WriteSpec::s16(16_000, 2), &pcm)?;
    assert_eq!(&wav[20..22], &0xFFFEu16.to_le_bytes());
    let p = crate::probe(&mut crate::ByteSource::from_slice(&wav))?;
    assert_eq!(p.codec, crate::ProbeCodec::PcmS16);
    assert_eq!(p.channels, 2);
    let d = decode_bytes(
        &wav,
        DecodeOptions::unbounded().with_channel_mode(ChannelMode::Split),
    )?;
    assert_eq!(d.num_channels(), 2);
    assert_eq!(d.frames(), 2);
    Ok(())
}
