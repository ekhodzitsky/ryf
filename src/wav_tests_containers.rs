use super::*;
use crate::ChannelMode;
use crate::error::Result;
use crate::source::ByteSource;

#[test]
fn test_s24_le4_mono_roundtrip() -> Result<()> {
    let samples_i24 = [0i32, 1000, -1000, 8_388_607, -8_388_608];
    let mut payload = Vec::with_capacity(samples_i24.len() * 4);
    for &s in &samples_i24 {
        let u = (s as u32) & 0x00ff_ffff;
        payload.extend_from_slice(&u.to_le_bytes());
    }
    let mut fmt = Vec::new();
    fmt.extend_from_slice(&WAVE_FORMAT_PCM.to_le_bytes());
    fmt.extend_from_slice(&1u16.to_le_bytes());
    fmt.extend_from_slice(&16_000u32.to_le_bytes());
    fmt.extend_from_slice(&(16_000u32 * 4).to_le_bytes());
    fmt.extend_from_slice(&4u16.to_le_bytes());
    fmt.extend_from_slice(&24u16.to_le_bytes());
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

    let (rate, out) = own_mono_native(&file)?;
    assert_eq!(rate, 16_000);
    assert_eq!(out.len(), samples_i24.len());
    for (i, &s) in samples_i24.iter().enumerate() {
        let expect = s as f32 / 8_388_608.0;
        let d = (out[i] - expect).abs();
        assert!(
            d < 1e-6,
            "sample {i}: got {} expect {expect} (i24={s})",
            out[i]
        );
    }
    Ok(())
}

#[test]
fn test_probe_s16_mono() -> Result<()> {
    let payload = gen_payload(TestCodec::S16, &mut XorShift64::new(1), 100, 1);
    let wav = WavBuilder {
        payload,
        ..WavBuilder::new(TestCodec::S16)
    }
    .build();
    let mut source = ByteSource::from_slice(&wav);
    let info = crate::probe(&mut source)?;
    assert_eq!(info.sample_rate, 16_000);
    assert_eq!(info.channels, 1);
    assert_eq!(info.sample_width, 2);
    assert_eq!(info.codec, crate::ProbeCodec::PcmS16);
    assert_eq!(info.declared_frames, Some(100));
    Ok(())
}

/// Pull-stream must be bit-exact with full `decode_with` and deliver
/// multiple blocks for longer files (O(block) scratch, not one allocation).
#[test]
fn test_streaming_mono_s16_bit_exact_and_chunked() -> Result<()> {
    // block_frames_for(2) = 256 KiB / 2 = 131_072 frames per pull block.
    let frames = 200_000usize;
    let payload = gen_payload(TestCodec::S16, &mut XorShift64::new(0x57EA), frames, 1);
    let wav = WavBuilder {
        payload,
        ..WavBuilder::new(TestCodec::S16)
    }
    .build();

    let full = {
        let mut s = ByteSource::from_slice(&wav);
        crate::decode_with(
            &mut s,
            &crate::DecodeOptions::default().with_channel_mode(ChannelMode::Mono),
        )?
    };
    assert_eq!(full.channels[0].len(), frames);

    let mut streamed = Vec::with_capacity(frames);
    let mut block_count = 0usize;
    let mut s = ByteSource::from_slice(&wav);
    let info = crate::decode_streaming(
        &mut s,
        &crate::DecodeOptions::default().with_channel_mode(ChannelMode::Mono),
        |block| {
            block_count += 1;
            assert_eq!(block.planar.len(), 1);
            assert_eq!(block.frames, block.planar[0].len());
            streamed.extend_from_slice(block.planar[0]);
            Ok(())
        },
    )?;
    assert_eq!(info.frames, frames);
    assert_eq!(info.channels, 1);
    assert!(
        block_count > 1,
        "expected multiple pull blocks for {frames} frames, got {block_count}"
    );
    assert_bit_exact("stream == full mono s16", &streamed, &full.channels[0]);
    Ok(())
}

#[test]
fn test_streaming_split_s16_bit_exact() -> Result<()> {
    let frames = 8_000usize;
    let payload = gen_payload(TestCodec::S16, &mut XorShift64::new(0x5B17), frames, 2);
    let wav = WavBuilder {
        channels: 2,
        payload,
        ..WavBuilder::new(TestCodec::S16)
    }
    .build();

    let full = {
        let mut s = ByteSource::from_slice(&wav);
        crate::decode_with(
            &mut s,
            &crate::DecodeOptions::default().with_channel_mode(ChannelMode::Split),
        )?
    };

    let mut ch0 = Vec::new();
    let mut ch1 = Vec::new();
    let mut s = ByteSource::from_slice(&wav);
    let info = crate::decode_streaming(
        &mut s,
        &crate::DecodeOptions::default().with_channel_mode(ChannelMode::Split),
        |block| {
            assert_eq!(block.planar.len(), 2);
            ch0.extend_from_slice(block.planar[0]);
            ch1.extend_from_slice(block.planar[1]);
            Ok(())
        },
    )?;
    assert_eq!(info.channels, 2);
    assert_eq!(info.frames, frames);
    assert_bit_exact("stream ch0", &ch0, &full.channels[0]);
    assert_bit_exact("stream ch1", &ch1, &full.channels[1]);
    Ok(())
}

#[test]
fn test_extensible_valid_bits_zero_fallback() -> Result<()> {
    let payload = gen_payload(TestCodec::S16, &mut XorShift64::new(2), 64, 1);
    let with_zero = WavBuilder {
        extensible: true,
        valid_bits: Some(0),
        payload: payload.clone(),
        ..WavBuilder::new(TestCodec::S16)
    }
    .build();
    let plain = WavBuilder {
        extensible: true,
        payload,
        ..WavBuilder::new(TestCodec::S16)
    }
    .build();
    let a = own_mono(&with_zero)?;
    let b = own_mono(&plain)?;
    assert_bit_exact("valid_bits=0 == default", &a, &b);
    Ok(())
}

#[test]
fn test_extensible_mask_zero_uses_nchannels() -> Result<()> {
    let payload = gen_payload(TestCodec::S16, &mut XorShift64::new(3), 40, 2);
    let wav = WavBuilder {
        channels: 2,
        extensible: true,
        channel_mask: Some(0),
        payload,
        ..WavBuilder::new(TestCodec::S16)
    }
    .build();
    let mut source = ByteSource::from_slice(&wav);
    let d = crate::decode(&mut source, ChannelMode::Split, "mask0")?;
    assert_eq!(d.sample_rate, 16_000);
    assert_eq!(d.channels.len(), 2);
    assert_eq!(d.channels[0].len(), 40);
    Ok(())
}

#[test]
fn test_bulk_s16_stereo_mix_finite() -> Result<()> {
    let mut rng = XorShift64::new(0xB01D);
    let payload = gen_payload(TestCodec::S16, &mut rng, 1024, 2);
    let wav = WavBuilder {
        channels: 2,
        payload,
        ..WavBuilder::new(TestCodec::S16)
    }
    .build();
    let (_, mono) = own_mono_native(&wav)?;
    assert_eq!(mono.len(), 1024);
    for (i, &s) in mono.iter().enumerate() {
        assert!(s.is_finite(), "non-finite at {i}");
        assert!((-1.0..=1.0).contains(&s), "out of range at {i}: {s}");
    }
    Ok(())
}

/// RF64: sniff + decode PCM via ds64-promoted data size.
#[test]
fn test_rf64_pcm_s16_decode() -> Result<()> {
    let samples: Vec<i16> = (0..100).map(|i| (i * 300) as i16).collect();
    let mut payload = Vec::with_capacity(samples.len() * 2);
    for s in &samples {
        payload.extend_from_slice(&s.to_le_bytes());
    }

    // ds64: riffSize, dataSize, sampleCount, tableLength=0
    let mut ds64 = Vec::new();
    // riffSize = 4 (WAVE) + fmt(8+16) + ds64(8+28) + data(8+payload) = computed below
    let fmt_body = {
        let mut f = Vec::new();
        f.extend_from_slice(&WAVE_FORMAT_PCM.to_le_bytes());
        f.extend_from_slice(&1u16.to_le_bytes());
        f.extend_from_slice(&16_000u32.to_le_bytes());
        f.extend_from_slice(&(16_000u32 * 2).to_le_bytes());
        f.extend_from_slice(&2u16.to_le_bytes());
        f.extend_from_slice(&16u16.to_le_bytes());
        f
    };
    let ds64_body_len = 28u64;
    let data_payload_len = payload.len() as u64;
    // Body after RIFF header size field: WAVE(4) + chunks
    let riff_size = 4u64 + 8 + ds64_body_len + 8 + fmt_body.len() as u64 + 8 + data_payload_len;
    ds64.extend_from_slice(&riff_size.to_le_bytes());
    ds64.extend_from_slice(&data_payload_len.to_le_bytes());
    ds64.extend_from_slice(&(samples.len() as u64).to_le_bytes()); // sampleCount
    ds64.extend_from_slice(&0u32.to_le_bytes()); // tableLength

    let mut body = Vec::new();
    body.extend_from_slice(b"ds64");
    body.extend_from_slice(&(ds64.len() as u32).to_le_bytes());
    body.extend_from_slice(&ds64);
    body.extend_from_slice(b"fmt ");
    body.extend_from_slice(&(fmt_body.len() as u32).to_le_bytes());
    body.extend_from_slice(&fmt_body);
    body.extend_from_slice(b"data");
    body.extend_from_slice(&u32::MAX.to_le_bytes()); // RF64 sentinel
    body.extend_from_slice(&payload);

    let mut file = Vec::new();
    file.extend_from_slice(b"RF64");
    file.extend_from_slice(&u32::MAX.to_le_bytes());
    file.extend_from_slice(b"WAVE");
    file.extend_from_slice(&body);

    assert!(
        {
            let mut s = ByteSource::from_slice(&file);
            crate::sniff_is_riff_wave(&mut s)?
        },
        "RF64 must sniff as wave"
    );
    let (rate, out) = own_mono_native(&file)?;
    assert_eq!(rate, 16_000);
    assert_eq!(out.len(), samples.len());
    for (i, &s) in samples.iter().enumerate() {
        let expect = s as f32 / 32_768.0;
        assert!(
            (out[i] - expect).abs() < 1e-6,
            "sample {i}: got {} expect {expect}",
            out[i]
        );
    }
    Ok(())
}

/// RIFX (big-endian) mono s16.
#[test]
fn test_rifx_s16_mono_decode() -> Result<()> {
    let samples: [i16; 5] = [0, 1000, -1000, i16::MAX, i16::MIN];
    let mut payload = Vec::new();
    for &s in &samples {
        payload.extend_from_slice(&s.to_be_bytes());
    }
    let mut fmt = Vec::new();
    fmt.extend_from_slice(&1u16.to_be_bytes()); // PCM
    fmt.extend_from_slice(&1u16.to_be_bytes());
    fmt.extend_from_slice(&16_000u32.to_be_bytes());
    fmt.extend_from_slice(&(16_000u32 * 2).to_be_bytes());
    fmt.extend_from_slice(&2u16.to_be_bytes());
    fmt.extend_from_slice(&16u16.to_be_bytes());

    let mut body = Vec::new();
    body.extend_from_slice(b"fmt ");
    body.extend_from_slice(&(fmt.len() as u32).to_be_bytes());
    body.extend_from_slice(&fmt);
    body.extend_from_slice(b"data");
    body.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    body.extend_from_slice(&payload);

    let mut file = Vec::new();
    file.extend_from_slice(b"RIFX");
    file.extend_from_slice(&(4 + body.len() as u32).to_be_bytes());
    file.extend_from_slice(b"WAVE");
    file.extend_from_slice(&body);

    assert!(
        {
            let mut s = ByteSource::from_slice(&file);
            crate::sniff_is_riff_wave(&mut s)?
        },
        "RIFX must sniff"
    );
    let (rate, out) = own_mono_native(&file)?;
    assert_eq!(rate, 16_000);
    assert_eq!(out.len(), samples.len());
    for (i, &s) in samples.iter().enumerate() {
        let expect = s as f32 / 32_768.0;
        assert!(
            (out[i] - expect).abs() < 1e-6,
            "sample {i}: got {} expect {expect}",
            out[i]
        );
    }
    Ok(())
}

/// Sony Wave64 mono s16.
#[test]
fn test_w64_s16_mono_decode() -> Result<()> {
    let samples: [i16; 4] = [0, 500, -500, 12000];
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

    // chunk size = 24 + body, pad body to 8.
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
    // Outer riff: size includes entire file from GUID start.
    file.extend_from_slice(&W64_GUID_RIFF);
    let size_pos = file.len();
    file.extend_from_slice(&0u64.to_le_bytes()); // placeholder
    file.extend_from_slice(&W64_GUID_WAVE);
    push_chunk(&mut file, &W64_GUID_FMT, &fmt);
    // data chunk: only real sample bytes in size; pad after payload.
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

    assert!(
        {
            let mut s = ByteSource::from_slice(&file);
            crate::sniff_is_riff_wave(&mut s)?
        },
        "W64 must sniff"
    );
    let (rate, out) = own_mono_native(&file)?;
    assert_eq!(rate, 16_000);
    assert_eq!(out.len(), samples.len());
    for (i, &s) in samples.iter().enumerate() {
        let expect = s as f32 / 32_768.0;
        assert!(
            (out[i] - expect).abs() < 1e-6,
            "sample {i}: got {} expect {expect}",
            out[i]
        );
    }
    Ok(())
}
