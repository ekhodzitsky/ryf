use super::*;
use crate::error::Result;

// --- 15.3: the differential matrix ---

#[test]
#[cfg_attr(miri, ignore = "differential sweeps are too slow under Miri")]
fn test_diff_matrix_bit_exact() -> Result<()> {
    let rates = [8000u32, 11025, 16000, 22050, 44100, 48000, 96000, 192000];
    let channel_counts = [1u16, 2, 4, 8];
    let mut rng = XorShift64::new(0xC0FF_EE12_3456_7890);
    let mut cells = 0usize;

    for &rate in &rates {
        for &ch in &channel_counts {
            for &codec in &TestCodec::ALL {
                for ext in [false, true] {
                    // 257 frames keep every cell cheap (the per-resample
                    // sinc-filter setup dominates at extreme rates);
                    // packet-boundary coverage lives in
                    // `test_diff_packet_boundary_crossing`.
                    let payload = gen_payload(codec, &mut rng, 257, usize::from(ch));
                    let label = format!("rate={rate} ch={ch} codec={codec:?} ext={ext}");
                    if ext && matches!(codec, TestCodec::ALaw | TestCodec::MuLaw) {
                        // Honest extensible g711 (valid_bits=8) is accepted;
                        // ffmpeg rejects it, so self-gate against plain form
                        // with the same payload.
                        let ext_wav = WavBuilder {
                            sample_rate: rate,
                            channels: ch,
                            codec,
                            extensible: true,
                            payload: payload.clone(),
                            ..WavBuilder::new(codec)
                        }
                        .build();
                        let plain_wav = WavBuilder {
                            sample_rate: rate,
                            channels: ch,
                            codec,
                            payload,
                            ..WavBuilder::new(codec)
                        }
                        .build();
                        let (_, a) = own_mono_native(&ext_wav)?;
                        let (_, b) = own_mono_native(&plain_wav)?;
                        assert_bit_exact(&format!("{label} ext==plain"), &a, &b);
                    } else {
                        let wav = WavBuilder {
                            sample_rate: rate,
                            channels: ch,
                            codec,
                            extensible: ext,
                            payload,
                            ..WavBuilder::new(codec)
                        }
                        .build();
                        // Native-rate comparison: conversion bit-exactness
                        // is rate-independent, and the shared resample
                        // stage is covered by the end-to-end subset in
                        // `test_diff_resample_integration`.
                        assert_native_bit_exact_both_modes(&label, &wav);
                    }
                    cells += 1;
                }
            }
        }
    }
    assert_eq!(cells, 512, "matrix size drifted");
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore = "differential sweeps are too slow under Miri")]
fn test_diff_packet_boundary_crossing() -> Result<()> {
    // More frames than symphonia's 1152-frames-per-packet, so the
    // symphonia path decodes several packets while the in-tree decoder
    // reads its own blocks - output must still match bit-exactly.
    let mut rng = XorShift64::new(0xABCD_EF01_2345_6789);
    for (codec, rate, ch) in [
        (TestCodec::S16, 48000u32, 2u16),
        (TestCodec::F64, 44100, 2),
        (TestCodec::MuLaw, 8000, 1),
        (TestCodec::S24, 96000, 4),
    ] {
        let payload = gen_payload(codec, &mut rng, 2500, usize::from(ch));
        let wav = WavBuilder {
            sample_rate: rate,
            channels: ch,
            payload,
            ..WavBuilder::new(codec)
        }
        .build();
        assert_bit_exact_both_modes(&format!("boundary codec={codec:?} rate={rate}"), &wav);
    }
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore = "multi-rate differential runs are too slow under Miri")]
fn test_diff_multi_rate() -> Result<()> {
    // Subset across the rate spectrum at native sample rate (product
    // 16 kHz resampling is covered in the consumer).
    let mut rng = XorShift64::new(0x1E57_1E57_1E57_1E57);
    for (codec, rate) in [
        (TestCodec::S16, 8000u32),
        (TestCodec::S16, 44100),
        (TestCodec::S16, 192000),
        (TestCodec::F32, 48000),
        (TestCodec::F32, 96000),
        (TestCodec::MuLaw, 8000),
        (TestCodec::F64, 22050),
    ] {
        for ch in [1u16, 2] {
            for ext in [false, true] {
                if ext && matches!(codec, TestCodec::MuLaw) {
                    continue; // ext g711 covered by the quirk tests
                }
                let payload = gen_payload(codec, &mut rng, 321, usize::from(ch));
                let wav = WavBuilder {
                    sample_rate: rate,
                    channels: ch,
                    extensible: ext,
                    payload,
                    ..WavBuilder::new(codec)
                }
                .build();
                assert_bit_exact_both_modes(
                    &format!("multi-rate {codec:?} rate={rate} ch={ch} ext={ext}"),
                    &wav,
                );
            }
        }
    }
    // Split-mode per-channel at non-16k rates.
    for (codec, rate) in [(TestCodec::S16, 48000u32), (TestCodec::F32, 44100)] {
        let payload = gen_payload(codec, &mut rng, 321, 4);
        let wav = WavBuilder {
            sample_rate: rate,
            channels: 4,
            payload,
            ..WavBuilder::new(codec)
        }
        .build();
        assert_channels_bit_exact(&format!("split multi-rate {codec:?} rate={rate}"), &wav);
    }
    Ok(())
}

// --- full-domain sweeps ---

#[test]
fn test_diff_u8_full_sweep() -> Result<()> {
    for ext in [false, true] {
        let payload: Vec<u8> = (0..=255).collect();
        let wav = WavBuilder {
            extensible: ext,
            payload,
            ..WavBuilder::new(TestCodec::U8)
        }
        .build();
        assert_bit_exact_both_modes(&format!("u8 sweep ext={ext}"), &wav);
    }
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore = "differential sweeps are too slow under Miri")]
fn test_diff_s16_full_sweep() -> Result<()> {
    let payload: Vec<u8> = (0..=65535u32)
        .flat_map(|v| (v as u16 as i16).to_le_bytes())
        .collect();
    let wav = WavBuilder {
        payload,
        ..WavBuilder::new(TestCodec::S16)
    }
    .build();
    assert_mono_bit_exact("s16 sweep", &wav);
    // Interleave the same sweep across two channels for the split path.
    let stereo: Vec<u8> = (0..=65535u32)
        .flat_map(|v| {
            let a = (v as u16 as i16).to_le_bytes();
            let b = (-(v as i32)) as i16 as u16;
            [a[0], a[1], b as u8, (b >> 8) as u8]
        })
        .collect();
    let wav = WavBuilder {
        channels: 2,
        payload: stereo,
        ..WavBuilder::new(TestCodec::S16)
    }
    .build();
    assert_bit_exact_both_modes("s16 sweep stereo", &wav);
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore = "differential sweeps are too slow under Miri")]
fn test_diff_s24_sweep() -> Result<()> {
    let mut rng = XorShift64::new(0x2424_2424_2424_2424);
    let payload = gen_payload(TestCodec::S24, &mut rng, 20_000, 1);
    let wav = WavBuilder {
        payload,
        ..WavBuilder::new(TestCodec::S24)
    }
    .build();
    assert_bit_exact_both_modes("s24 sweep", &wav);
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore = "differential sweeps are too slow under Miri")]
fn test_diff_s32_sweep() -> Result<()> {
    let mut rng = XorShift64::new(0x3232_3232_3232_3232);
    let payload = gen_payload(TestCodec::S32, &mut rng, 20_000, 1);
    let wav = WavBuilder {
        payload,
        ..WavBuilder::new(TestCodec::S32)
    }
    .build();
    assert_bit_exact_both_modes("s32 sweep", &wav);
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore = "differential sweeps are too slow under Miri")]
fn test_diff_float_random_bit_patterns() -> Result<()> {
    // Random raw bit patterns (incl. NaN payloads, infinities,
    // subnormals) - conversion must agree bit-exactly on every pattern.
    let mut rng = XorShift64::new(0xF32F_32F3_2F32_F32F);
    let payload = gen_payload(TestCodec::F32, &mut rng, 10_000, 1);
    let wav = WavBuilder {
        payload,
        ..WavBuilder::new(TestCodec::F32)
    }
    .build();
    assert_bit_exact_both_modes("f32 random bits", &wav);
    let payload = gen_payload(TestCodec::F64, &mut rng, 10_000, 1);
    let wav = WavBuilder {
        payload,
        ..WavBuilder::new(TestCodec::F64)
    }
    .build();
    assert_bit_exact_both_modes("f64 random bits", &wav);
    Ok(())
}

#[test]
fn test_diff_g711_full_sweep() -> Result<()> {
    for codec in [TestCodec::ALaw, TestCodec::MuLaw] {
        for ext in [false, true] {
            let payload: Vec<u8> = (0..=255).collect();
            let wav = WavBuilder {
                sample_rate: 8000,
                extensible: ext,
                payload,
                ..WavBuilder::new(codec)
            }
            .build();
            let label = format!("{codec:?} sweep ext={ext}");
            // Honest extensible g711 (valid_bits=8) is accepted; plain and
            // extensible must match. ffmpeg rejects extensible g711, so
            // extensible is self-gated against the plain form.
            if ext {
                let plain = WavBuilder {
                    sample_rate: 8000,
                    payload: (0..=255).collect(),
                    ..WavBuilder::new(codec)
                }
                .build();
                let own_ext = own_mono(&wav)?;
                let own_plain = own_mono(&plain)?;
                assert_bit_exact(&format!("{label} == plain"), &own_ext, &own_plain);
            } else {
                assert_bit_exact_both_modes(&label, &wav);
            }
        }
    }
    Ok(())
}

#[test]
fn test_diff_g711_extensible_valid16() -> Result<()> {
    // The one extensible g711 form the old symphonia pipeline decoded:
    // valid_bits == 16 (its decoder demands the stated width equal the
    // 16-bit decoded width). Nonsensical per the WAV spec, but kept for
    // parity. ffmpeg rejects extensible mu-law/A-law outright, so the
    // gate is self-consistency: the extensible parse must yield the same
    // samples as the plain form (itself gated vs ffmpeg in the sweeps).
    for codec in [TestCodec::ALaw, TestCodec::MuLaw] {
        let payload: Vec<u8> = (0..=255).collect();
        let ext = WavBuilder {
            sample_rate: 8000,
            extensible: true,
            valid_bits: Some(16),
            payload: payload.clone(),
            ..WavBuilder::new(codec)
        }
        .build();
        let plain = WavBuilder {
            sample_rate: 8000,
            payload,
            ..WavBuilder::new(codec)
        }
        .build();
        let own_ext = own_mono(&ext)?;
        let own_plain = own_mono(&plain)?;
        assert_bit_exact(
            &format!("{codec:?} ext valid=16 == plain"),
            &own_ext,
            &own_plain,
        );
    }
    Ok(())
}

// --- structural cases: chunks, padding, fmt variants ---

#[test]
fn test_diff_odd_size_chunks_and_padding() -> Result<()> {
    let mut rng = XorShift64::new(0x0DD0_0DD0_0DD0_0DD0);
    let payload = gen_payload(TestCodec::S16, &mut rng, 500, 2);
    let wav = WavBuilder {
        channels: 2,
        chunks_before_fmt: vec![(*b"JUNK", vec![0xAA; 5])], // odd length, pad byte
        chunks_before_data: vec![
            (*b"bext", vec![0xBB; 101]), // odd length, pad byte
            (*b"cue ", vec![0xCC; 24]),
        ],
        payload,
        ..WavBuilder::new(TestCodec::S16)
    }
    .build();
    assert_bit_exact_both_modes("odd chunks + padding", &wav);
    Ok(())
}

#[test]
fn test_diff_list_info_chunk_skipped() -> Result<()> {
    // Well-formed LIST/INFO with an odd-length sub-chunk (IART, padded).
    let mut info = Vec::new();
    info.extend_from_slice(b"INFO");
    info.extend_from_slice(b"IART");
    info.extend_from_slice(&7u32.to_le_bytes());
    info.extend_from_slice(b"artist\0");
    info.push(0); // pad
    let payload = gen_payload(TestCodec::S16, &mut XorShift64::new(7), 400, 1);
    let wav = WavBuilder {
        chunks_before_data: vec![(*b"LIST", info)],
        payload,
        ..WavBuilder::new(TestCodec::S16)
    }
    .build();
    assert_bit_exact_both_modes("LIST/INFO skipped", &wav);
    Ok(())
}

#[test]
fn test_diff_fact_chunk() -> Result<()> {
    let payload = gen_payload(TestCodec::S16, &mut XorShift64::new(11), 300, 1);
    let wav = WavBuilder {
        chunks_before_data: vec![(*b"fact", 300u32.to_le_bytes().to_vec())],
        payload,
        ..WavBuilder::new(TestCodec::S16)
    }
    .build();
    assert_bit_exact_both_modes("fact chunk", &wav);
    // Longer fact (BWF tail / 8 zero bytes): first 4 are 0 (unknown), skip rest.
    let payload = gen_payload(TestCodec::S16, &mut XorShift64::new(11), 300, 1);
    let wav = WavBuilder {
        chunks_before_data: vec![(*b"fact", vec![0; 8])],
        payload,
        ..WavBuilder::new(TestCodec::S16)
    }
    .build();
    assert_bit_exact_both_modes("fact len 8", &wav);
    Ok(())
}

#[test]
fn test_diff_pcm_fmt_len_18_20_and_40() -> Result<()> {
    for pcm_fmt_len in [18u32, 20, 40] {
        let payload = gen_payload(TestCodec::S16, &mut XorShift64::new(13), 300, 1);
        let wav = WavBuilder {
            pcm_fmt_len,
            payload,
            ..WavBuilder::new(TestCodec::S16)
        }
        .build();
        assert_bit_exact_both_modes(&format!("pcm fmt len {pcm_fmt_len}"), &wav);
    }
    Ok(())
}

#[test]
fn test_diff_ieee_fmt_len_18() -> Result<()> {
    // 18-byte IEEE fmt with cbSize = 0 is accepted by both paths.
    let payload = gen_payload(TestCodec::F32, &mut XorShift64::new(17), 300, 1);
    let mut fmt = Vec::new();
    fmt.extend_from_slice(&WAVE_FORMAT_IEEE_FLOAT.to_le_bytes());
    fmt.extend_from_slice(&1u16.to_le_bytes());
    fmt.extend_from_slice(&16000u32.to_le_bytes());
    fmt.extend_from_slice(&64000u32.to_le_bytes());
    fmt.extend_from_slice(&4u16.to_le_bytes());
    fmt.extend_from_slice(&32u16.to_le_bytes());
    fmt.extend_from_slice(&0u16.to_le_bytes()); // cbSize = 0
    let mut body = Vec::new();
    body.extend_from_slice(b"fmt ");
    body.extend_from_slice(&18u32.to_le_bytes());
    body.extend_from_slice(&fmt);
    body.extend_from_slice(b"data");
    body.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    body.extend_from_slice(&payload);
    let mut file = Vec::new();
    file.extend_from_slice(b"RIFF");
    file.extend_from_slice(&(4 + body.len() as u32).to_le_bytes());
    file.extend_from_slice(b"WAVE");
    file.extend_from_slice(&body);
    assert_bit_exact_both_modes("ieee fmt len 18", &file);
    file[36] = 1; // cbSize != 0; ffmpeg still decodes
    assert_bit_exact_both_modes("ieee fmt 18 cbSize 1", &file);
    Ok(())
}
