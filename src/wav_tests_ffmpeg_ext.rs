use super::*;
use crate::error::Result;

#[test]
fn test_diff_extensible_valid_bits() -> Result<()> {
    // valid < bits is accepted and ignored (the in-tree decoder decodes
    // the full container width, as the old symphonia pipeline did).
    // Recorded: ffmpeg instead respects valid_bits and zeroes the low
    // bits, so it cannot gate here - the gate is self-consistency with
    // the no-valid-bits form.
    for (codec, valid) in [
        (TestCodec::S24, 20u16),
        (TestCodec::S32, 24),
        (TestCodec::S16, 8),
    ] {
        let payload = gen_payload(codec, &mut XorShift64::new(19), 300, 1);
        let with = WavBuilder {
            extensible: true,
            valid_bits: Some(valid),
            payload: payload.clone(),
            ..WavBuilder::new(codec)
        }
        .build();
        let without = WavBuilder {
            extensible: true,
            payload,
            ..WavBuilder::new(codec)
        }
        .build();
        let own_with = own_mono(&with)?;
        let own_without = own_mono(&without)?;
        assert_bit_exact(
            &format!("valid {valid} in {codec:?} == no valid_bits"),
            &own_with,
            &own_without,
        );
    }
    // valid > bits is rejected by both.
    let payload = gen_payload(TestCodec::S16, &mut XorShift64::new(19), 16, 1);
    let wav = WavBuilder {
        extensible: true,
        valid_bits: Some(20),
        payload,
        ..WavBuilder::new(TestCodec::S16)
    }
    .build();
    assert_both_err("valid > bits", &wav);
    // IEEE float with valid != bits is rejected by both.
    let payload = gen_payload(TestCodec::F32, &mut XorShift64::new(19), 16, 1);
    let wav = WavBuilder {
        extensible: true,
        valid_bits: Some(24),
        payload,
        ..WavBuilder::new(TestCodec::F32)
    }
    .build();
    assert_both_err("float valid != bits", &wav);
    Ok(())
}

#[test]
fn test_diff_extensible_channel_masks() -> Result<()> {
    let cases: &[(&str, u16, Option<u32>, bool)] = &[
        ("mask 0 auto-fix", 2, Some(0), true),
        ("mask standard stereo", 2, Some(0x3), true),
        ("mask standard quad", 4, Some(0x33), true),
        ("mask 5.1", 6, Some(0x3F), true),
        ("mask popcount>ch fix-down", 2, Some(0xF), true),
        ("mask popcount<ch fix-up", 4, Some(0x3), true),
        ("mask 8ch full", 8, Some(0xFF), true),
        ("mask bit >= 18", 1, Some(0x4_0000), false),
    ];
    for &(label, ch, mask, expect_ok) in cases {
        let payload = gen_payload(
            TestCodec::S16,
            &mut XorShift64::new(23),
            200,
            usize::from(ch),
        );
        let wav = WavBuilder {
            channels: ch,
            extensible: true,
            channel_mask: mask,
            payload,
            ..WavBuilder::new(TestCodec::S16)
        }
        .build();
        if expect_ok {
            assert_bit_exact_both_modes(label, &wav);
        } else {
            assert_both_err(label, &wav);
        }
    }
    Ok(())
}

#[test]
fn test_diff_extensible_ambisonic() -> Result<()> {
    // Ambisonic B-format extensible GUIDs (WXYZ quad) decode like PCM.
    for (guid, ch) in [
        (KSDATAFORMAT_SUBTYPE_AMBISONIC_PCM, 4u16),
        (KSDATAFORMAT_SUBTYPE_AMBISONIC_IEEE_FLOAT, 4u16),
    ] {
        let codec = if guid == KSDATAFORMAT_SUBTYPE_AMBISONIC_PCM {
            TestCodec::S16
        } else {
            TestCodec::F32
        };
        let payload = gen_payload(codec, &mut XorShift64::new(29), 200, usize::from(ch));
        let width = codec.width() as u16;
        let mut fmt = Vec::new();
        fmt.extend_from_slice(&WAVE_FORMAT_EXTENSIBLE.to_le_bytes());
        fmt.extend_from_slice(&ch.to_le_bytes());
        fmt.extend_from_slice(&16000u32.to_le_bytes());
        fmt.extend_from_slice(&(16000u32 * u32::from(ch * width)).to_le_bytes());
        fmt.extend_from_slice(&(ch * width).to_le_bytes());
        fmt.extend_from_slice(&codec.bits().to_le_bytes());
        fmt.extend_from_slice(&22u16.to_le_bytes());
        fmt.extend_from_slice(&codec.bits().to_le_bytes());
        fmt.extend_from_slice(&0u32.to_le_bytes()); // mask ignored for ambisonic
        fmt.extend_from_slice(&guid);
        let mut body = Vec::new();
        body.extend_from_slice(b"fmt ");
        body.extend_from_slice(&40u32.to_le_bytes());
        body.extend_from_slice(&fmt);
        body.extend_from_slice(b"data");
        body.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        body.extend_from_slice(&payload);
        let mut file = Vec::new();
        file.extend_from_slice(b"RIFF");
        file.extend_from_slice(&(4 + body.len() as u32).to_le_bytes());
        file.extend_from_slice(b"WAVE");
        file.extend_from_slice(&body);
        assert_bit_exact_both_modes("ambisonic extensible", &file);
    }
    Ok(())
}

#[test]
fn test_diff_zero_length_data() -> Result<()> {
    for rate in [16000u32, 48000] {
        let wav = WavBuilder {
            sample_rate: rate,
            ..WavBuilder::new(TestCodec::S16)
        }
        .build();
        let own = own_mono(&wav)?;
        assert!(own.is_empty(), "own must decode zero frames");
    }
    Ok(())
}

#[test]
fn test_diff_declared_data_shorter_than_actual() -> Result<()> {
    // Header declares fewer bytes than present: both decode exactly the
    // declared frames, trailing bytes are never touched.
    let mut rng = XorShift64::new(31);
    let payload = gen_payload(TestCodec::S16, &mut rng, 500, 1);
    let declared = 400u32; // 200 of 500 frames
    let wav = WavBuilder {
        declared_data_len: Some(declared),
        riff_len: Some(36 + declared),
        payload,
        ..WavBuilder::new(TestCodec::S16)
    }
    .build();
    assert_bit_exact_both_modes("declared < actual", &wav);
    Ok(())
}

#[test]
fn test_diff_multiple_data_chunks_first_wins() -> Result<()> {
    let mut rng = XorShift64::new(37);
    let payload = gen_payload(TestCodec::S16, &mut rng, 100, 1);
    let first = WavBuilder {
        payload: payload.clone(),
        ..WavBuilder::new(TestCodec::S16)
    }
    .build();
    // Append a second data chunk; the in-tree decoder ignores it
    // (first wins, old-pipeline parity). Recorded: ffmpeg does not
    // first-win here, so the gate is self-consistency.
    let second_payload = gen_payload(TestCodec::S16, &mut rng, 100, 1);
    let mut two = first.clone();
    two.extend_from_slice(b"data");
    two.extend_from_slice(&(second_payload.len() as u32).to_le_bytes());
    two.extend_from_slice(&second_payload);
    // Fix the riff length to cover both data chunks.
    let riff_len = (two.len() - 8) as u32;
    two[4..8].copy_from_slice(&riff_len.to_le_bytes());
    let own_two = own_mono(&two)?;
    let own_first = own_mono(&first)?;
    assert_bit_exact("multiple data chunks: two == first", &own_two, &own_first);
    Ok(())
}

#[test]
fn test_diff_partial_final_frame_dropped() -> Result<()> {
    // Odd data length on a 2-byte codec: the trailing half-frame is
    // dropped (old-pipeline parity). ffmpeg errors on it instead
    // (recorded), so the gate is self-consistency with the even form
    // (itself gated vs ffmpeg in the sweeps).
    let mut payload = gen_payload(TestCodec::S16, &mut XorShift64::new(41), 100, 1);
    payload.push(0xAB);
    let odd = WavBuilder {
        payload: payload.clone(),
        ..WavBuilder::new(TestCodec::S16)
    }
    .build();
    let even = WavBuilder {
        payload: payload[..payload.len() - 1].to_vec(),
        ..WavBuilder::new(TestCodec::S16)
    }
    .build();
    let own_odd = own_mono(&odd)?;
    let own_even = own_mono(&even)?;
    assert_bit_exact("partial final frame == even form", &own_odd, &own_even);
    Ok(())
}

#[test]
fn test_diff_streaming_riff_len() -> Result<()> {
    // riff_len = u32::MAX (ffmpeg-to-stdout convention) with an honest
    // data length: both paths decode fine.
    let payload = gen_payload(TestCodec::S16, &mut XorShift64::new(43), 300, 1);
    let wav = WavBuilder {
        riff_len: Some(u32::MAX),
        payload,
        ..WavBuilder::new(TestCodec::S16)
    }
    .build();
    assert_bit_exact_both_modes("streaming riff_len", &wav);
    Ok(())
}

// --- documented divergences (malformed input only) ---

#[test]
fn test_divergence_data_declared_longer_than_file_clamped() -> Result<()> {
    // Declared data length exceeds the actual bytes: symphonia errors on
    // the short read; the in-tree decoder clamps and decodes the frames
    // that exist. Divergence class 1 (documented).
    let mut rng = XorShift64::new(47);
    let payload = gen_payload(TestCodec::S16, &mut rng, 100, 1);
    let actual_len = payload.len() as u32;
    let wav = WavBuilder {
        declared_data_len: Some(actual_len + 4096),
        riff_len: Some(36 + actual_len + 4096), // parent check must pass
        payload,
        ..WavBuilder::new(TestCodec::S16)
    }
    .build();

    // The old symphonia pipeline errored on the short read; the in-tree
    // decoder clamps (documented divergence class 1).
    let own = own_mono(&wav)?;
    assert_eq!(own.len(), 100, "clamped decode must yield the real frames");

    // The clamped output must be bit-identical to decoding the honest
    // version of the same payload.
    let honest = WavBuilder {
        payload: gen_payload(TestCodec::S16, &mut XorShift64::new(47), 100, 1),
        ..WavBuilder::new(TestCodec::S16)
    }
    .build();
    let honest_own = own_mono(&honest)?;
    assert_bit_exact("clamped vs honest", &own, &honest_own);
    Ok(())
}

#[test]
fn test_divergence_streaming_data_len() -> Result<()> {
    // data len = u32::MAX (streaming marker): the old symphonia pipeline
    // read until the short final read and errored; the in-tree decoder
    // clamps to the real bytes and succeeds. Divergence class 2
    // (documented).
    let payload = gen_payload(TestCodec::S16, &mut XorShift64::new(53), 120, 1);
    let wav = WavBuilder {
        declared_data_len: Some(u32::MAX),
        riff_len: Some(u32::MAX),
        payload,
        ..WavBuilder::new(TestCodec::S16)
    }
    .build();
    let own = own_mono(&wav)?;
    assert_eq!(own.len(), 120);
    Ok(())
}

#[test]
fn test_divergence_extensible_fmt_len_over_40() -> Result<()> {
    // fmt_ext with a 4-byte private tail: the old symphonia pipeline
    // left it unread and desynchronized; the in-tree decoder skips it.
    // Divergence class 3 (documented).
    let payload = gen_payload(TestCodec::S16, &mut XorShift64::new(59), 100, 1);
    let mut wav = WavBuilder {
        extensible: true,
        payload,
        ..WavBuilder::new(TestCodec::S16)
    }
    .build();
    // Splice 4 extra bytes into the fmt chunk (after the GUID), fix the
    // fmt chunk length and riff length.
    let fmt_body_end = 12 + 8 + 40;
    wav.splice(fmt_body_end..fmt_body_end, [0xDE, 0xAD, 0xBE, 0xEF]);
    wav[12 + 4..12 + 8].copy_from_slice(&44u32.to_le_bytes()); // fmt len
    let riff_len = (wav.len() - 8) as u32;
    wav[4..8].copy_from_slice(&riff_len.to_le_bytes());

    let own = own_mono(&wav)?;
    assert_eq!(own.len(), 100);
    Ok(())
}

// --- malformed inputs: both paths must fail ---

#[test]
fn test_diff_invalid_channel_counts() {
    for ch in [0u16, 27, 100] {
        let payload = gen_payload(TestCodec::S16, &mut XorShift64::new(61), 8, 1);
        let wav = WavBuilder {
            channels: ch,
            payload,
            ..WavBuilder::new(TestCodec::S16)
        }
        .build();
        assert_both_err(&format!("channels={ch}"), &wav);
    }
}

#[test]
fn test_diff_invalid_sample_rates() {
    for rate in [0u32, 192_001, 1_000_000_000] {
        let payload = gen_payload(TestCodec::S16, &mut XorShift64::new(67), 8, 1);
        let wav = WavBuilder {
            sample_rate: rate,
            payload,
            ..WavBuilder::new(TestCodec::S16)
        }
        .build();
        assert_both_err(&format!("rate={rate}"), &wav);
    }
}

#[test]
fn test_diff_unsupported_format_tags() {
    // Unknown tag (e.g. MPEG-in-WAV 0x0055): both reject at probe stage.
    for tag in [0x0055u16, 0x0002, 0x0011] {
        let bits = if tag == 0x0055 { 0u16 } else { 4u16 };
        let extra: Vec<u8> = match tag {
            WAVE_FORMAT_ADPCM_MS => {
                // len 50: cbSize=32 + 32 bytes of ADPCM coefficients
                let mut v = 32u16.to_le_bytes().to_vec();
                v.extend_from_slice(&[0u8; 32]);
                v
            }
            WAVE_FORMAT_ADPCM_IMA => 2u16.to_le_bytes().to_vec(),
            _ => Vec::new(),
        };
        let mut fmt = Vec::new();
        fmt.extend_from_slice(&tag.to_le_bytes());
        fmt.extend_from_slice(&1u16.to_le_bytes());
        fmt.extend_from_slice(&8000u32.to_le_bytes());
        fmt.extend_from_slice(&8000u32.to_le_bytes());
        fmt.extend_from_slice(&1u16.to_le_bytes());
        fmt.extend_from_slice(&bits.to_le_bytes());
        fmt.extend_from_slice(&extra);
        let mut body = Vec::new();
        body.extend_from_slice(b"fmt ");
        body.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
        body.extend_from_slice(&fmt);
        body.extend_from_slice(b"data");
        body.extend_from_slice(&16u32.to_le_bytes());
        body.extend_from_slice(&[0u8; 16]);
        let mut file = Vec::new();
        file.extend_from_slice(b"RIFF");
        file.extend_from_slice(&(4 + body.len() as u32).to_le_bytes());
        file.extend_from_slice(b"WAVE");
        file.extend_from_slice(&body);
        assert_both_err(&format!("tag={tag:#06x}"), &file);
    }
}

#[test]
fn test_diff_g711_fmt_len_16_accepted() -> Result<()> {
    // PCMWAVEFORMAT-sized (16-byte) g711 headers occur in the wild; accept
    // them and match the canonical 18-byte (cbSize=0) form sample-for-sample.
    for codec in [TestCodec::ALaw, TestCodec::MuLaw] {
        let payload = [0x55u8; 16];
        let mut fmt16 = Vec::new();
        fmt16.extend_from_slice(&codec.fmt_tag().to_le_bytes());
        fmt16.extend_from_slice(&1u16.to_le_bytes());
        fmt16.extend_from_slice(&8000u32.to_le_bytes());
        fmt16.extend_from_slice(&8000u32.to_le_bytes());
        fmt16.extend_from_slice(&1u16.to_le_bytes());
        fmt16.extend_from_slice(&8u16.to_le_bytes());
        let build = |fmt: &[u8]| {
            let mut body = Vec::new();
            body.extend_from_slice(b"fmt ");
            body.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
            body.extend_from_slice(fmt);
            body.extend_from_slice(b"data");
            body.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            body.extend_from_slice(&payload);
            let mut file = Vec::new();
            file.extend_from_slice(b"RIFF");
            file.extend_from_slice(&(4 + body.len() as u32).to_le_bytes());
            file.extend_from_slice(b"WAVE");
            file.extend_from_slice(&body);
            file
        };
        let mut fmt18 = fmt16.clone();
        fmt18.extend_from_slice(&0u16.to_le_bytes());
        let a = own_mono(&build(&fmt16))?;
        let b = own_mono(&build(&fmt18))?;
        assert_bit_exact(&format!("{codec:?} fmt16 == fmt18"), &a, &b);
    }
    Ok(())
}

#[test]
fn test_diff_data_before_fmt_rejected() {
    let mut body = Vec::new();
    body.extend_from_slice(b"data");
    body.extend_from_slice(&4u32.to_le_bytes());
    body.extend_from_slice(&[0u8; 4]);
    body.extend_from_slice(b"fmt ");
    body.extend_from_slice(&16u32.to_le_bytes());
    body.extend_from_slice(&[0u8; 16]);
    let mut file = Vec::new();
    file.extend_from_slice(b"RIFF");
    file.extend_from_slice(&(4 + body.len() as u32).to_le_bytes());
    file.extend_from_slice(b"WAVE");
    file.extend_from_slice(&body);
    assert_both_err("data before fmt", &file);
}

#[test]
fn test_diff_missing_data_chunk() {
    // riff length honest but no data chunk inside it.
    let fmt = WavBuilder::new(TestCodec::S16).fmt_body();
    let mut body = Vec::new();
    body.extend_from_slice(b"fmt ");
    body.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
    body.extend_from_slice(&fmt);
    let mut file = Vec::new();
    file.extend_from_slice(b"RIFF");
    file.extend_from_slice(&(4 + body.len() as u32).to_le_bytes());
    file.extend_from_slice(b"WAVE");
    file.extend_from_slice(&body);
    assert_both_err("missing data chunk", &file);
}

#[test]
fn test_diff_truncated_mid_header() {
    // RIFF/WAVE markers present, stream ends inside the fmt chunk.
    let wav = WavBuilder {
        truncate_file: Some(30),
        payload: gen_payload(TestCodec::S16, &mut XorShift64::new(73), 16, 1),
        ..WavBuilder::new(TestCodec::S16)
    }
    .build();
    assert_both_err("truncated mid-header", &wav);
}
