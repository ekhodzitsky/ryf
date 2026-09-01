use super::*;
use crate::ChannelMode;
use crate::error::Result;
use crate::source::ByteSource;

#[test]
fn test_bw64_pcm_s16_decode() -> Result<()> {
    let wav = build_bw64_s16_seed(&mut XorShift64::new(0xB064));
    let (rate, out) = own_mono_native(&wav)?;
    assert_eq!(rate, 16_000);
    assert_eq!(out.len(), 32);
    Ok(())
}

#[test]
fn test_rf64_missing_ds64_rejected() {
    let mut fmt = Vec::new();
    fmt.extend_from_slice(&WAVE_FORMAT_PCM.to_le_bytes());
    fmt.extend_from_slice(&1u16.to_le_bytes());
    fmt.extend_from_slice(&16_000u32.to_le_bytes());
    fmt.extend_from_slice(&(16_000u32 * 2).to_le_bytes());
    fmt.extend_from_slice(&2u16.to_le_bytes());
    fmt.extend_from_slice(&16u16.to_le_bytes());
    let payload = [0u8; 8];
    let mut body = Vec::new();
    body.extend_from_slice(b"fmt ");
    body.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
    body.extend_from_slice(&fmt);
    body.extend_from_slice(b"data");
    body.extend_from_slice(&u32::MAX.to_le_bytes());
    body.extend_from_slice(&payload);
    let mut file = Vec::new();
    file.extend_from_slice(b"RF64");
    file.extend_from_slice(&u32::MAX.to_le_bytes());
    file.extend_from_slice(b"WAVE");
    file.extend_from_slice(&body);
    assert_both_err("RF64 missing ds64", &file);
}

#[test]
fn test_rf64_duplicate_ds64_rejected() {
    let wav = build_rf64_s16_seed(&mut XorShift64::new(1));
    // Insert a second ds64 (28 zero bytes) immediately after the first
    // ds64 chunk: tag(4)+len(4)+body(28) = 36 bytes starting at offset 12.
    let mut dup = wav;
    let second = {
        let mut c = b"ds64".to_vec();
        c.extend_from_slice(&28u32.to_le_bytes());
        c.extend_from_slice(&[0u8; 28]);
        c
    };
    dup.splice(12 + 36..12 + 36, second);
    assert_both_err("RF64 duplicate ds64", &dup);
}

#[test]
fn test_rifx_f32_and_s24_mono() -> Result<()> {
    // f32 BE
    let vals = [0.0f32, 0.5, -0.25, 1.0];
    let mut payload = Vec::new();
    for v in vals {
        payload.extend_from_slice(&v.to_be_bytes());
    }
    let mut fmt = Vec::new();
    fmt.extend_from_slice(&WAVE_FORMAT_IEEE_FLOAT.to_be_bytes());
    fmt.extend_from_slice(&1u16.to_be_bytes());
    fmt.extend_from_slice(&16_000u32.to_be_bytes());
    fmt.extend_from_slice(&(16_000u32 * 4).to_be_bytes());
    fmt.extend_from_slice(&4u16.to_be_bytes());
    fmt.extend_from_slice(&32u16.to_be_bytes());
    let file = riff_wrap(b"RIFX", true, &fmt, &payload);
    let (rate, out) = own_mono_native(&file)?;
    assert_eq!(rate, 16_000);
    for (i, &v) in vals.iter().enumerate() {
        assert_eq!(out[i].to_bits(), v.to_bits(), "f32 i={i}");
    }

    // packed s24 BE
    let s24 = [0i32, 1000, -1000];
    let mut payload = Vec::new();
    for s in s24 {
        let u = (s as u32) & 0x00ff_ffff;
        payload.push(((u >> 16) & 0xff) as u8);
        payload.push(((u >> 8) & 0xff) as u8);
        payload.push((u & 0xff) as u8);
    }
    let mut fmt = Vec::new();
    fmt.extend_from_slice(&WAVE_FORMAT_PCM.to_be_bytes());
    fmt.extend_from_slice(&1u16.to_be_bytes());
    fmt.extend_from_slice(&16_000u32.to_be_bytes());
    fmt.extend_from_slice(&(16_000u32 * 3).to_be_bytes());
    fmt.extend_from_slice(&3u16.to_be_bytes());
    fmt.extend_from_slice(&24u16.to_be_bytes());
    let file = riff_wrap(b"RIFX", true, &fmt, &payload);
    let (_, out) = own_mono_native(&file)?;
    assert_eq!(out.len(), 3);
    assert!((out[1] - 1000.0 / 8_388_608.0).abs() < 1e-9);
    assert!(out[2] < 0.0);
    Ok(())
}

#[test]
fn test_w64_fact_and_odd_payload_pad() -> Result<()> {
    // 3 s16 samples → 6 data bytes, pad to 8.
    let samples = [10i16, -10, 20];
    let mut payload = Vec::new();
    for &s in &samples {
        payload.extend_from_slice(&s.to_le_bytes());
    }
    let mut fmt = Vec::new();
    fmt.extend_from_slice(&1u16.to_le_bytes());
    fmt.extend_from_slice(&1u16.to_le_bytes());
    fmt.extend_from_slice(&16_000u32.to_le_bytes());
    fmt.extend_from_slice(&(16_000u32 * 2).to_le_bytes());
    fmt.extend_from_slice(&2u16.to_le_bytes());
    fmt.extend_from_slice(&16u16.to_le_bytes());

    let push_chunk = |out: &mut Vec<u8>, guid: &[u8; 16], data: &[u8]| {
        let mut body = data.to_vec();
        let pad = (8 - (body.len() % 8)) % 8;
        body.extend(std::iter::repeat_n(0u8, pad));
        let chunk_size = 24u64 + body.len() as u64;
        out.extend_from_slice(guid);
        out.extend_from_slice(&chunk_size.to_le_bytes());
        out.extend_from_slice(&body);
    };

    let mut file = Vec::new();
    file.extend_from_slice(&W64_GUID_RIFF);
    let size_pos = file.len();
    file.extend_from_slice(&0u64.to_le_bytes());
    file.extend_from_slice(&W64_GUID_WAVE);
    push_chunk(&mut file, &W64_GUID_FMT, &fmt);
    push_chunk(&mut file, &W64_GUID_FACT, &3u32.to_le_bytes());
    {
        let data_bytes = payload.len() as u64;
        let pad = (8 - (data_bytes % 8)) % 8;
        let chunk_size = 24 + data_bytes + pad;
        file.extend_from_slice(&W64_GUID_DATA);
        file.extend_from_slice(&chunk_size.to_le_bytes());
        file.extend_from_slice(&payload);
        file.extend(std::iter::repeat_n(0u8, pad as usize));
    }
    let total = file.len() as u64;
    file[size_pos..size_pos + 8].copy_from_slice(&total.to_le_bytes());

    let (rate, out) = own_mono_native(&file)?;
    assert_eq!(rate, 16_000);
    assert_eq!(out.len(), 3);
    assert!((out[0] - 10.0 / 32_768.0).abs() < 1e-9);
    Ok(())
}

#[test]
fn test_lying_fact_does_not_too_long_when_data_is_short() -> Result<()> {
    // fact claims ~17 hours @ 16 kHz; payload is 100 frames. Duration
    // follows the bytes on disk, not the header boast.
    let payload = gen_payload(TestCodec::S16, &mut XorShift64::new(101), 100, 1);
    let wav = WavBuilder {
        chunks_before_data: vec![(*b"fact", 1_000_000_000u32.to_le_bytes().to_vec())],
        payload,
        ..WavBuilder::new(TestCodec::S16)
    }
    .build();
    let (rate, out) = own_mono_native(&wav)?;
    assert_eq!(rate, 16_000);
    assert_eq!(out.len(), 100);
    Ok(())
}

#[test]
fn test_streaming_marker_over_duration_is_too_long() {
    let payload = gen_payload(TestCodec::S16, &mut XorShift64::new(99), 2000, 1);
    let wav = WavBuilder {
        declared_data_len: Some(u32::MAX),
        riff_len: Some(u32::MAX),
        payload,
        ..WavBuilder::new(TestCodec::S16)
    }
    .build();
    let opts = crate::DecodeOptions::default().with_max_duration_secs(0.01);
    let err = crate::decode_bytes(&wav, opts).expect_err("must reject over-budget stream");
    assert!(matches!(err, crate::WavError::TooLong { .. }), "{err}");
}

#[test]
fn test_split_output_ram_budget() {
    let payload = gen_payload(TestCodec::S16, &mut XorShift64::new(3), 1000, 2);
    let wav = WavBuilder {
        channels: 2,
        payload,
        ..WavBuilder::new(TestCodec::S16)
    }
    .build();
    // 2 ch × 1000 frames × 4 bytes = 8000; budget 1000 → reject.
    let opts = crate::DecodeOptions::default()
        .with_channel_mode(ChannelMode::Split)
        .with_max_output_bytes(1000);
    let err = crate::decode_bytes(&wav, opts).expect_err("RAM cap");
    assert!(
        matches!(err, crate::WavError::OutputTooLarge { .. }),
        "{err}"
    );
}

#[test]
fn test_from_vec_matches_slice() -> Result<()> {
    let payload = gen_payload(TestCodec::S16, &mut XorShift64::new(5), 64, 1);
    let wav = WavBuilder {
        payload,
        ..WavBuilder::new(TestCodec::S16)
    }
    .build();
    let slice = {
        let mut s = ByteSource::from_slice(&wav);
        crate::decode(&mut s, ChannelMode::Mono, "slice")?
    };
    let owned = {
        let mut s = ByteSource::from_vec(wav.clone());
        assert!(s.contiguous_slice().is_some());
        crate::decode(&mut s, ChannelMode::Mono, "vec")?
    };
    assert_bit_exact(
        "from_vec == from_slice",
        &slice.channels[0],
        &owned.channels[0],
    );
    Ok(())
}

#[test]
fn test_streaming_stereo_mix_bit_exact() -> Result<()> {
    let frames = 4_000usize;
    let payload = gen_payload(TestCodec::S16, &mut XorShift64::new(0x51_1E), frames, 2);
    let wav = WavBuilder {
        channels: 2,
        payload,
        ..WavBuilder::new(TestCodec::S16)
    }
    .build();
    let full = {
        let mut s = ByteSource::from_slice(&wav);
        crate::decode_with(&mut s, &crate::DecodeOptions::default())?
    };
    let mut streamed = Vec::new();
    let mut s = ByteSource::from_slice(&wav);
    let info = crate::decode_streaming(&mut s, &crate::DecodeOptions::default(), |b| {
        assert_eq!(b.planar.len(), 1);
        streamed.extend_from_slice(b.planar[0]);
        Ok(())
    })?;
    assert_eq!(info.frames, frames);
    assert_eq!(info.channels, 1);
    assert_bit_exact("stream mix == collect", &streamed, &full.channels[0]);
    Ok(())
}

#[test]
fn test_rifx_s16_stereo_mix() -> Result<()> {
    let frames = [[100i16, -100], [1000, -1000], [0, 0], [i16::MAX, i16::MIN]];
    let mut payload = Vec::new();
    for [l, r] in frames {
        payload.extend_from_slice(&l.to_be_bytes());
        payload.extend_from_slice(&r.to_be_bytes());
    }
    let mut fmt = Vec::new();
    fmt.extend_from_slice(&1u16.to_be_bytes());
    fmt.extend_from_slice(&2u16.to_be_bytes());
    fmt.extend_from_slice(&16_000u32.to_be_bytes());
    fmt.extend_from_slice(&(16_000u32 * 4).to_be_bytes());
    fmt.extend_from_slice(&4u16.to_be_bytes());
    fmt.extend_from_slice(&16u16.to_be_bytes());
    let file = riff_wrap(b"RIFX", true, &fmt, &payload);
    let (_, out) = own_mono_native(&file)?;
    assert_eq!(out.len(), 4);
    let expect = (100.0 / 32_768.0 + -100.0 / 32_768.0) / 2.0;
    assert!((out[0] - expect).abs() < 1e-9);
    Ok(())
}

#[test]
fn test_probe_rejects_rate_and_codec() {
    let wav = WavBuilder {
        sample_rate: 0,
        payload: gen_payload(TestCodec::S16, &mut XorShift64::new(11), 8, 1),
        ..WavBuilder::new(TestCodec::S16)
    }
    .build();
    let mut s = ByteSource::from_slice(&wav);
    assert!(matches!(
        crate::probe(&mut s),
        Err(crate::WavError::UnsupportedSampleRate { .. })
    ));
}

#[test]
fn test_rifx_f64_mono() -> Result<()> {
    let vals = [0.0f64, 0.5, -0.25, 1.0, -1.0];
    let mut payload = Vec::new();
    for v in vals {
        payload.extend_from_slice(&v.to_be_bytes());
    }
    let mut fmt = Vec::new();
    fmt.extend_from_slice(&WAVE_FORMAT_IEEE_FLOAT.to_be_bytes());
    fmt.extend_from_slice(&1u16.to_be_bytes());
    fmt.extend_from_slice(&16_000u32.to_be_bytes());
    fmt.extend_from_slice(&(16_000u32 * 8).to_be_bytes());
    fmt.extend_from_slice(&8u16.to_be_bytes());
    fmt.extend_from_slice(&64u16.to_be_bytes());
    let file = riff_wrap(b"RIFX", true, &fmt, &payload);
    let (rate, out) = own_mono_native(&file)?;
    assert_eq!(rate, 16_000);
    assert_eq!(out.len(), vals.len());
    for (i, &v) in vals.iter().enumerate() {
        assert!((out[i] - v as f32).abs() < 1e-6, "i={i}");
    }
    Ok(())
}

#[test]
fn test_rf64_mulaw_and_alaw() -> Result<()> {
    for codec in [TestCodec::MuLaw, TestCodec::ALaw] {
        let payload: Vec<u8> = (0u8..=63).collect();
        let mut fmt = Vec::new();
        fmt.extend_from_slice(&codec.fmt_tag().to_le_bytes());
        fmt.extend_from_slice(&1u16.to_le_bytes());
        fmt.extend_from_slice(&8_000u32.to_le_bytes());
        fmt.extend_from_slice(&8_000u32.to_le_bytes());
        fmt.extend_from_slice(&1u16.to_le_bytes());
        fmt.extend_from_slice(&8u16.to_le_bytes());
        fmt.extend_from_slice(&0u16.to_le_bytes());

        let mut ds64 = Vec::new();
        ds64.extend_from_slice(&0u64.to_le_bytes());
        ds64.extend_from_slice(&(payload.len() as u64).to_le_bytes());
        ds64.extend_from_slice(&(payload.len() as u64).to_le_bytes());
        ds64.extend_from_slice(&0u32.to_le_bytes());

        let mut body = Vec::new();
        body.extend_from_slice(b"ds64");
        body.extend_from_slice(&(ds64.len() as u32).to_le_bytes());
        body.extend_from_slice(&ds64);
        body.extend_from_slice(b"fmt ");
        body.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
        body.extend_from_slice(&fmt);
        if fmt.len() % 2 == 1 {
            body.push(0);
        }
        body.extend_from_slice(b"data");
        body.extend_from_slice(&u32::MAX.to_le_bytes());
        body.extend_from_slice(&payload);
        let riff_size = 4u64 + body.len() as u64;
        body[8..16].copy_from_slice(&riff_size.to_le_bytes());

        let mut file = Vec::new();
        file.extend_from_slice(b"RF64");
        file.extend_from_slice(&u32::MAX.to_le_bytes());
        file.extend_from_slice(b"WAVE");
        file.extend_from_slice(&body);

        let (rate, out) = own_mono_native(&file).unwrap_or_else(|e| panic!("{codec:?}: {e}"));
        assert_eq!(rate, 8_000);
        assert_eq!(out.len(), payload.len());
        let plain = WavBuilder {
            sample_rate: 8_000,
            codec,
            payload: payload.clone(),
            ..WavBuilder::new(codec)
        }
        .build();
        let (_, expect) = own_mono_native(&plain)?;
        assert_bit_exact(&format!("RF64 {codec:?} == RIFF"), &out, &expect);
    }
    Ok(())
}

#[test]
fn test_probe_rf64_w64_adpcm_and_rifx() -> Result<()> {
    let rf64 = build_rf64_s16_seed(&mut XorShift64::new(2));
    let mut s = ByteSource::from_slice(&rf64);
    let p = crate::probe(&mut s)?;
    assert_eq!(p.sample_rate, 16_000);
    assert_eq!(p.codec, crate::ProbeCodec::PcmS16);
    assert_eq!(p.declared_frames, Some(32));
    assert!(p.data_pos > 0);

    let w64 = build_w64_s16_seed();
    let mut s = ByteSource::from_slice(&w64);
    let p = crate::probe(&mut s)?;
    assert_eq!(p.codec, crate::ProbeCodec::PcmS16);
    assert_eq!(p.declared_frames, Some(4));

    let rifx = build_rifx_s16_seed();
    let mut s = ByteSource::from_slice(&rifx);
    let p = crate::probe(&mut s)?;
    assert_eq!(p.codec, crate::ProbeCodec::PcmS16);

    #[cfg(feature = "adpcm")]
    {
        let ms = adpcm_fixture("ms_adpcm_16k_mono.wav");
        let mut s = ByteSource::from_slice(&ms);
        let p = crate::probe(&mut s)?;
        assert_eq!(p.sample_rate, 16_000);
        assert_eq!(p.codec, crate::ProbeCodec::MsAdpcm);
        assert!(p.declared_frames.unwrap_or(0) > 0);
    }
    Ok(())
}

#[test]
fn test_sniff_containers_and_rejects() -> Result<()> {
    for bytes in [
        build_rf64_s16_seed(&mut XorShift64::new(3)),
        build_bw64_s16_seed(&mut XorShift64::new(4)),
        build_w64_s16_seed(),
        build_rifx_s16_seed(),
    ] {
        let mut s = ByteSource::from_slice(&bytes);
        assert!(crate::sniff_is_riff_wave(&mut s)?);
        assert_eq!(s.pos(), 0);
    }
    let mut s = ByteSource::from_slice(b"not a wave");
    assert!(!crate::sniff_is_riff_wave(&mut s)?);
    let mut s = ByteSource::from_slice(b"RIFF");
    assert!(!crate::sniff_is_riff_wave(&mut s)?);
    Ok(())
}

#[test]
fn test_output_too_large_mono() {
    let payload = gen_payload(TestCodec::S16, &mut XorShift64::new(8), 500, 1);
    let wav = WavBuilder {
        payload,
        ..WavBuilder::new(TestCodec::S16)
    }
    .build();
    // 500 frames × 4 bytes = 2000.
    let opts = crate::DecodeOptions::default().with_max_output_bytes(100);
    let err = crate::decode_bytes(&wav, opts).expect_err("RAM cap mono");
    assert!(
        matches!(err, crate::WavError::OutputTooLarge { .. }),
        "{err}"
    );
}

#[test]
fn test_stream_length_unknown() {
    use std::io::Cursor;
    let payload = gen_payload(TestCodec::S16, &mut XorShift64::new(9), 16, 1);
    let wav = WavBuilder {
        payload,
        ..WavBuilder::new(TestCodec::S16)
    }
    .build();
    let mut src = ByteSource::from_read_seek(Cursor::new(wav), None);
    let err = crate::decode(&mut src, ChannelMode::Mono, "nolength").unwrap_err();
    assert!(matches!(err, crate::WavError::StreamLengthUnknown), "{err}");
}

#[test]
fn test_streaming_callback_error_propagates() {
    let payload = gen_payload(TestCodec::S16, &mut XorShift64::new(10), 64, 1);
    let wav = WavBuilder {
        payload,
        ..WavBuilder::new(TestCodec::S16)
    }
    .build();
    let mut src = ByteSource::from_slice(&wav);
    let err = crate::decode_streaming(&mut src, &crate::DecodeOptions::default(), |_| {
        Err(crate::WavError::format("callback boom"))
    })
    .unwrap_err();
    assert!(err.to_string().contains("callback boom"));
}
