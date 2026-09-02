use super::*;
use crate::FormatKind;
use crate::error::Result;

#[test]
fn test_diff_chunk_len_exceeds_riff_parent() {
    // Honest riff length, but the data chunk declares more than the
    // parent allows: both reject before decoding.
    let payload = gen_payload(TestCodec::S16, &mut XorShift64::new(79), 16, 1);
    let wav = WavBuilder {
        declared_data_len: Some(4096),
        riff_len: Some(36 + 32), // parent ends after 32 data bytes
        payload,
        ..WavBuilder::new(TestCodec::S16)
    }
    .build();
    assert_both_err("chunk exceeds parent", &wav);
}

#[test]
fn test_diff_file_backed_decode_paths() -> Result<()> {
    // `decode_audio_file` / `load_audio_channels` use a File-backed
    // ByteSource (vs the in-memory Bytes one) - the sniff, chunk walk,
    // and clamping must behave identically on both sources.
    let payload = gen_payload(TestCodec::S16, &mut XorShift64::new(83), 1000, 2);
    let wav = WavBuilder {
        channels: 2,
        payload,
        ..WavBuilder::new(TestCodec::S16)
    }
    .build();

    let mut tmp = tempfile::NamedTempFile::with_suffix(".wav")?;
    std::io::Write::write_all(&mut tmp, &wav)?;
    let path = tmp
        .path()
        .to_str()
        .ok_or_else(|| crate::WavError::format(FormatKind::InvalidOperation))?
        .to_owned();

    let via_file = decode_file_mono(&path)?;
    let via_bytes = own_mono(&wav)?;
    assert_bit_exact("file vs bytes mono", &via_file, &via_bytes);

    let chans_file = decode_file_channels(&path)?;
    let chans_bytes = own_channels(&wav)?;
    assert_eq!(chans_file.len(), chans_bytes.len());
    for (c, (a, b)) in chans_file.iter().zip(chans_bytes.iter()).enumerate() {
        assert_bit_exact(&format!("file vs bytes ch{c}"), a, b);
    }
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore = "decodes real speech files - too slow under Miri")]
fn test_diff_golos_fixtures_bit_exact() -> Result<()> {
    // Optional real-speech corpus from the product test fixtures
    // (sibling crate). No serde: just every `.wav` in the fixtures dir.
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/speech");
    if !dir.is_dir() {
        eprintln!("golos fixtures not present alongside the crate, skipping");
        return Ok(());
    }
    let mut files: Vec<_> = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("wav"))
        .collect();
    files.sort();
    if files.is_empty() {
        eprintln!("no golos wav fixtures, skipping");
        return Ok(());
    }
    for path in &files {
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("fixture.wav");
        let data = std::fs::read(path)?;
        assert_bit_exact_both_modes(&format!("golos {name}"), &data);
    }
    Ok(())
}

// --- fuzz-seed corpus (15.4) ---

/// Deterministic seed corpus for `cargo fuzz run audio_decode`: one file
/// per codec and layout corner plus structural mutants (truncation,
/// odd chunks, lying lengths). Also exercised by
/// [`test_seed_corpus_deterministic`].
pub fn seed_corpus_entries() -> Vec<(String, Vec<u8>)> {
    let mut rng = XorShift64::new(0x5EED_5EED_5EED_5EED);
    let mut out: Vec<(String, Vec<u8>)> = Vec::new();

    for codec in TestCodec::ALL {
        for ext in [false, true] {
            let name = format!("seed_{codec:?}_ch2_16k{}", if ext { "_ext" } else { "" });
            let payload = gen_payload(codec, &mut rng, 64, 2);
            out.push((
                name,
                WavBuilder {
                    channels: 2,
                    extensible: ext,
                    payload,
                    ..WavBuilder::new(codec)
                }
                .build(),
            ));
        }
    }

    // g711 telephony corners.
    for codec in [TestCodec::ALaw, TestCodec::MuLaw] {
        out.push((
            format!("seed_{codec:?}_8k_sweep"),
            WavBuilder {
                sample_rate: 8000,
                payload: (0..=255).collect(),
                ..WavBuilder::new(codec)
            }
            .build(),
        ));
    }

    // Structural mutants.
    let base = WavBuilder {
        channels: 2,
        payload: gen_payload(TestCodec::S16, &mut rng, 64, 2),
        ..WavBuilder::new(TestCodec::S16)
    };
    out.push((
        "seed_odd_chunks".into(),
        WavBuilder {
            chunks_before_fmt: vec![(*b"JUNK", vec![0xAA; 5])],
            chunks_before_data: vec![(*b"bext", vec![0xBB; 101])],
            payload: base.payload.clone(),
            ..WavBuilder::new(TestCodec::S16)
        }
        .build(),
    ));
    out.push((
        "seed_truncated_data".into(),
        WavBuilder {
            declared_data_len: Some(base.payload.len() as u32 + 1024),
            riff_len: Some(36 + base.payload.len() as u32 + 1024),
            payload: base.payload.clone(),
            ..WavBuilder::new(TestCodec::S16)
        }
        .build(),
    ));
    out.push((
        "seed_truncated_file".into(),
        WavBuilder {
            truncate_file: Some(40),
            payload: base.payload.clone(),
            ..WavBuilder::new(TestCodec::S16)
        }
        .build(),
    ));
    out.push((
        "seed_streaming_lens".into(),
        WavBuilder {
            declared_data_len: Some(u32::MAX),
            riff_len: Some(u32::MAX),
            payload: base.payload.clone(),
            ..WavBuilder::new(TestCodec::S16)
        }
        .build(),
    ));
    out.push((
        "seed_partial_frame".into(),
        WavBuilder {
            payload: {
                let mut p = base.payload.clone();
                p.push(0xAB);
                p
            },
            ..WavBuilder::new(TestCodec::S16)
        }
        .build(),
    ));
    out.push((
        "seed_empty_data".into(),
        WavBuilder::new(TestCodec::S16).build(),
    ));
    out.push((
        "seed_adversarial_rate".into(),
        WavBuilder {
            sample_rate: 1_000_000_000,
            payload: base.payload.clone(),
            ..WavBuilder::new(TestCodec::S16)
        }
        .build(),
    ));
    // Random garbage with a RIFF/WAVE prefix.
    let mut garbage = b"RIFF\xFF\xFF\xFF\xFFWAVE".to_vec();
    garbage.extend((0..256).map(|_| rng.next_u8()));
    out.push(("seed_riff_garbage".into(), garbage));

    // --- SOTA container / layout seeds (RIFX, RF64, BW64, W64, S24_LE) ---
    out.push(("seed_s24_le4".into(), build_s24_le4_seed(&mut rng)));
    out.push(("seed_rifx_s16".into(), build_rifx_s16_seed()));
    out.push(("seed_rf64_s16".into(), build_rf64_s16_seed(&mut rng)));
    out.push(("seed_bw64_s16".into(), build_bw64_s16_seed(&mut rng)));
    out.push(("seed_w64_s16".into(), build_w64_s16_seed()));
    out.push(("seed_ms_adpcm_min".into(), build_ms_adpcm_minimal_seed()));
    out.push(("seed_ima_adpcm_min".into(), build_ima_adpcm_minimal_seed()));
    // Valid_bits=0 extensible (wild files).
    out.push((
        "seed_ext_valid_bits_zero".into(),
        WavBuilder {
            extensible: true,
            valid_bits: Some(0),
            payload: gen_payload(TestCodec::S16, &mut rng, 32, 1),
            ..WavBuilder::new(TestCodec::S16)
        }
        .build(),
    ));

    out
}

/// 24-bit in 4-byte containers (S24_LE).
pub fn build_s24_le4_seed(rng: &mut XorShift64) -> Vec<u8> {
    let mut payload = Vec::with_capacity(64);
    for _ in 0..16 {
        let s = (rng.next_u64() as i32) & 0x00ff_ffff;
        payload.extend_from_slice(&(s as u32).to_le_bytes());
    }
    let mut fmt = Vec::new();
    fmt.extend_from_slice(&WAVE_FORMAT_PCM.to_le_bytes());
    fmt.extend_from_slice(&1u16.to_le_bytes());
    fmt.extend_from_slice(&16_000u32.to_le_bytes());
    fmt.extend_from_slice(&(16_000u32 * 4).to_le_bytes());
    fmt.extend_from_slice(&4u16.to_le_bytes());
    fmt.extend_from_slice(&24u16.to_le_bytes());
    riff_wrap(b"RIFF", false, &fmt, &payload)
}

pub fn build_rifx_s16_seed() -> Vec<u8> {
    let samples = [0i16, 100, -100, 1000];
    let mut payload = Vec::new();
    for &s in &samples {
        payload.extend_from_slice(&s.to_be_bytes());
    }
    let mut fmt = Vec::new();
    fmt.extend_from_slice(&1u16.to_be_bytes());
    fmt.extend_from_slice(&1u16.to_be_bytes());
    fmt.extend_from_slice(&16_000u32.to_be_bytes());
    fmt.extend_from_slice(&(16_000u32 * 2).to_be_bytes());
    fmt.extend_from_slice(&2u16.to_be_bytes());
    fmt.extend_from_slice(&16u16.to_be_bytes());
    riff_wrap(b"RIFX", true, &fmt, &payload)
}

pub fn build_rf64_s16_seed(rng: &mut XorShift64) -> Vec<u8> {
    build_rf64_family_seed(b"RF64", rng)
}

pub fn build_bw64_s16_seed(rng: &mut XorShift64) -> Vec<u8> {
    build_rf64_family_seed(b"BW64", rng)
}

pub fn build_rf64_family_seed(magic: &[u8; 4], rng: &mut XorShift64) -> Vec<u8> {
    let payload = gen_payload(TestCodec::S16, rng, 32, 1);
    let mut fmt = Vec::new();
    fmt.extend_from_slice(&WAVE_FORMAT_PCM.to_le_bytes());
    fmt.extend_from_slice(&1u16.to_le_bytes());
    fmt.extend_from_slice(&16_000u32.to_le_bytes());
    fmt.extend_from_slice(&(16_000u32 * 2).to_le_bytes());
    fmt.extend_from_slice(&2u16.to_le_bytes());
    fmt.extend_from_slice(&16u16.to_le_bytes());

    let mut ds64 = Vec::new();
    let data_payload_len = payload.len() as u64;
    // riffSize filled after body is assembled.
    ds64.extend_from_slice(&0u64.to_le_bytes());
    ds64.extend_from_slice(&data_payload_len.to_le_bytes());
    ds64.extend_from_slice(&32u64.to_le_bytes()); // sampleCount frames
    ds64.extend_from_slice(&0u32.to_le_bytes());

    let mut body = Vec::new();
    body.extend_from_slice(b"ds64");
    body.extend_from_slice(&(ds64.len() as u32).to_le_bytes());
    body.extend_from_slice(&ds64);
    body.extend_from_slice(b"fmt ");
    body.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
    body.extend_from_slice(&fmt);
    body.extend_from_slice(b"data");
    body.extend_from_slice(&u32::MAX.to_le_bytes());
    body.extend_from_slice(&payload);

    let riff_size = 4u64 + body.len() as u64;
    // Patch ds64 riffSize (first 8 bytes of ds64 body at offset 8 after ds64 tag+len).
    // body: "ds64"(4) + len(4) + riffSize(8) ...
    body[8..16].copy_from_slice(&riff_size.to_le_bytes());

    let mut file = Vec::new();
    file.extend_from_slice(magic);
    file.extend_from_slice(&u32::MAX.to_le_bytes());
    file.extend_from_slice(b"WAVE");
    file.extend_from_slice(&body);
    file
}

pub fn build_w64_s16_seed() -> Vec<u8> {
    let samples = [0i16, 1, -1, 42];
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
    file
}

/// Minimal well-formed MS-ADPCM mono (one block of silence-ish coeffs).
pub fn build_ms_adpcm_minimal_seed() -> Vec<u8> {
    // block_align=256, samples_per_block ~ 500, 7 default coefs.
    let block_align = 256u16;
    let samples_per_block = 500u16;
    let mut fmt = Vec::new();
    fmt.extend_from_slice(&WAVE_FORMAT_ADPCM_MS.to_le_bytes());
    fmt.extend_from_slice(&1u16.to_le_bytes()); // mono
    fmt.extend_from_slice(&16_000u32.to_le_bytes());
    fmt.extend_from_slice(&8000u32.to_le_bytes()); // avg bytes/sec (approx)
    fmt.extend_from_slice(&block_align.to_le_bytes());
    fmt.extend_from_slice(&4u16.to_le_bytes()); // bits
    // cbSize = 32: samplesPerBlock(2) + nCoefs(2) + 7*4 coefs
    fmt.extend_from_slice(&32u16.to_le_bytes());
    fmt.extend_from_slice(&samples_per_block.to_le_bytes());
    fmt.extend_from_slice(&7u16.to_le_bytes());
    let coefs: [(i16, i16); 7] = [
        (256, 0),
        (512, -256),
        (0, 0),
        (192, 64),
        (240, 0),
        (460, -208),
        (392, -232),
    ];
    for (a, b) in coefs {
        fmt.extend_from_slice(&a.to_le_bytes());
        fmt.extend_from_slice(&b.to_le_bytes());
    }
    // One block: 7-byte mono header + nibble data.
    let mut block = vec![0u8; block_align as usize];
    block[0] = 0; // predictor
    block[1] = 16;
    block[2] = 0; // delta = 16
    // sample1 / sample2 zeros
    let mut body_payload = block;
    // Pad to even.
    if body_payload.len() % 2 == 1 {
        body_payload.push(0);
    }
    riff_wrap(b"RIFF", false, &fmt, &body_payload)
}

pub fn build_ima_adpcm_minimal_seed() -> Vec<u8> {
    let block_align = 256u16;
    let samples_per_block = 505u16; // typical: 1 + (block-4)*2 for mono
    let mut fmt = Vec::new();
    fmt.extend_from_slice(&WAVE_FORMAT_ADPCM_IMA.to_le_bytes());
    fmt.extend_from_slice(&1u16.to_le_bytes());
    fmt.extend_from_slice(&16_000u32.to_le_bytes());
    fmt.extend_from_slice(&8000u32.to_le_bytes());
    fmt.extend_from_slice(&block_align.to_le_bytes());
    fmt.extend_from_slice(&4u16.to_le_bytes());
    fmt.extend_from_slice(&2u16.to_le_bytes()); // cbSize
    fmt.extend_from_slice(&samples_per_block.to_le_bytes());
    let mut block = vec![0u8; block_align as usize];
    // predictor i16 + step index
    block[0] = 0;
    block[1] = 0;
    block[2] = 0; // step index 0
    riff_wrap(b"RIFF", false, &fmt, &block)
}

/// Wrap fmt+data into a RIFF/RIFX file (`be` selects endian for sizes).
pub fn riff_wrap(magic: &[u8; 4], be: bool, fmt: &[u8], payload: &[u8]) -> Vec<u8> {
    let enc_u32 = |v: u32| {
        if be { v.to_be_bytes() } else { v.to_le_bytes() }
    };
    let mut body = Vec::new();
    body.extend_from_slice(b"fmt ");
    body.extend_from_slice(&enc_u32(fmt.len() as u32));
    body.extend_from_slice(fmt);
    if fmt.len() % 2 == 1 {
        body.push(0);
    }
    body.extend_from_slice(b"data");
    body.extend_from_slice(&enc_u32(payload.len() as u32));
    body.extend_from_slice(payload);
    if payload.len() % 2 == 1 {
        body.push(0);
    }
    let mut file = Vec::new();
    file.extend_from_slice(magic);
    file.extend_from_slice(&enc_u32(4 + body.len() as u32));
    file.extend_from_slice(b"WAVE");
    file.extend_from_slice(&body);
    file
}

#[test]
fn test_seed_corpus_deterministic() {
    // The fuzz property, natively: no panics, deterministic outcome.
    for (name, data) in seed_corpus_entries() {
        let a = own_mono(&data);
        let b = own_mono(&data);
        match (a, b) {
            (Ok(x), Ok(y)) => assert_bit_exact(&format!("seed {name} determinism"), &x, &y),
            (Err(_), Err(_)) => {}
            _ => panic!("seed {name}: decode is non-deterministic"),
        }
    }
}

/// Writes the seed corpus to `$GIGASTT_WAV_SEED_DIR` (one file per
/// entry). Canonical regenerator for `fuzz/fuzz_targets/audio_decode.rs`:
///
/// ```sh
/// ./scripts/gen_wav_fuzz_corpus.sh
/// cargo +nightly fuzz run audio_decode
/// ```
#[test]
#[ignore = "writes files; run explicitly to regenerate the fuzz corpus"]
fn test_wav_seed_corpus_write() -> Result<()> {
    let dir = std::env::var("GIGASTT_WAV_SEED_DIR")
        .map_err(|_| crate::WavError::format(FormatKind::InvalidOperation))?;
    std::fs::create_dir_all(&dir)?;
    for (name, data) in seed_corpus_entries() {
        std::fs::write(format!("{dir}/{name}.wav"), &data)?;
    }
    Ok(())
}
