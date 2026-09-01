use super::*;
use crate::ChannelMode;
use crate::error::Result;
use crate::source::ByteSource;

#[cfg(feature = "adpcm")]
#[test]
fn test_adpcm_split_stereo_fixture() -> Result<()> {
    for name in ["ms_adpcm_16k_stereo.wav", "ima_adpcm_16k_stereo.wav"] {
        let data = adpcm_fixture(name);
        let opts = crate::DecodeOptions::default().with_channel_mode(ChannelMode::Split);
        let decoded = crate::decode_bytes(&data, opts).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(decoded.sample_rate, 16_000, "{name}");
        assert_eq!(decoded.channels.len(), 2, "{name}");
        assert_eq!(
            decoded.channels[0].len(),
            decoded.channels[1].len(),
            "{name}"
        );
        assert!(decoded.channels[0].len() > 50, "{name}");
        let mut streamed = [Vec::new(), Vec::new()];
        let mut src = ByteSource::from_slice(&data);
        crate::decode_streaming(
            &mut src,
            &crate::DecodeOptions::default().with_channel_mode(ChannelMode::Split),
            |b| {
                assert_eq!(b.planar.len(), 2);
                streamed[0].extend_from_slice(b.planar[0]);
                streamed[1].extend_from_slice(b.planar[1]);
                Ok(())
            },
        )?;
        assert_bit_exact(
            &format!("{name} stream L"),
            &streamed[0],
            &decoded.channels[0],
        );
        assert_bit_exact(
            &format!("{name} stream R"),
            &streamed[1],
            &decoded.channels[1],
        );
    }
    Ok(())
}

#[cfg(not(feature = "adpcm"))]
#[test]
fn test_adpcm_feature_disabled() {
    let data = include_bytes!("../fixtures/ms_adpcm_16k_mono.wav");
    let err = own_mono(data).expect_err("adpcm disabled");
    assert!(
        matches!(err, crate::WavError::FeatureDisabled { feature: "adpcm" }),
        "{err}"
    );
}

#[cfg(feature = "adpcm")]
pub fn adpcm_fixture(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// MS-ADPCM / IMA-ADPCM: decode ffmpeg-produced fixtures from the crate.
#[cfg(feature = "adpcm")]
#[test]
fn test_adpcm_ffmpeg_files_smoke() {
    for name in [
        "ms_adpcm_16k_mono.wav",
        "ima_adpcm_16k_mono.wav",
        "ms_adpcm_16k_stereo.wav",
        "ima_adpcm_16k_stereo.wav",
    ] {
        let data = adpcm_fixture(name);
        let path = name;
        let (rate, mono) =
            own_mono_native(&data).unwrap_or_else(|e| panic!("{path}: decode failed: {e:#}"));
        assert_eq!(rate, 16_000, "{path} rate");
        assert!(
            mono.len() > 100,
            "{path}: expected speech-length samples, got {}",
            mono.len()
        );
        // Energy should be non-trivial for a sine.
        let energy: f32 = mono.iter().map(|s| s * s).sum::<f32>() / mono.len() as f32;
        assert!(
            energy > 1e-6,
            "{path}: near-silent decode (energy={energy})"
        );
        for (i, &s) in mono.iter().enumerate() {
            assert!(s.is_finite(), "{path}: non-finite at {i}");
        }
    }
}

/// ADPCM: max |Δ| vs ffmpeg f32 oracle stays within a few LSBs of s16.
/// ADPCM is lossy and implementers differ slightly; we gate on a tight
/// absolute error rather than bit-exact equality.
#[cfg(feature = "adpcm")]
#[test]
fn test_adpcm_ffmpeg_max_abs_parity() -> Result<()> {
    if !ffmpeg_available() {
        eprintln!("ffmpeg/ffprobe not available, skipping ADPCM parity");
        return Ok(());
    }
    for name in [
        "ms_adpcm_16k_mono.wav",
        "ima_adpcm_16k_mono.wav",
        "ms_adpcm_16k_stereo.wav",
        "ima_adpcm_16k_stereo.wav",
    ] {
        let data = adpcm_fixture(name);
        let path = name;
        let (rate_o, own) = own_mono_native(&data)?;
        let (rate_f, ff_ch) = ffmpeg_native_channels(path, &data)?;
        assert_eq!(rate_o, rate_f, "{path} rate");
        let ff = mix_mono(&ff_ch);
        // Allow small length skew from block padding (≤ one ADPCM block).
        let n = own.len().min(ff.len());
        assert!(n > 100, "{path}: too short");
        let len_delta = own.len().abs_diff(ff.len());
        assert!(
            len_delta <= 2048,
            "{path}: length skew too large own={} ff={} delta={len_delta}",
            own.len(),
            ff.len()
        );
        let mut max_abs = 0.0f32;
        for i in 0..n {
            max_abs = max_abs.max((own[i] - ff[i]).abs());
        }
        // ~3/32768 ≈ 9e-5; allow headroom for predictor state drift.
        assert!(
            max_abs < 5e-3,
            "{path}: max |Δ|={max_abs} exceeds 5e-3 vs ffmpeg"
        );
    }
    Ok(())
}

/// Offline micro-bench (no Criterion / no network): prints ns/sample for
/// the s16→f32 kernel. Run with:
/// `cargo test test_microbench_s16_convert -- --ignored --nocapture`
#[test]
#[ignore = "micro-bench: run explicitly with --ignored --nocapture"]
fn test_microbench_s16_convert() -> Result<()> {
    let frames = 1_000_000usize;
    let mut src = vec![0u8; frames * 2];
    let mut rng = XorShift64::new(0xBEEF);
    for chunk in src.as_chunks_mut::<2>().0 {
        let s = rng.next_u64() as i16;
        chunk.copy_from_slice(&s.to_le_bytes());
    }
    let mut dst = vec![0.0f32; frames];
    // Warmup
    for _ in 0..3 {
        crate::convert::convert_s16_mono(&src, &mut dst);
    }
    let t0 = std::time::Instant::now();
    let iters = 20usize;
    for _ in 0..iters {
        crate::convert::convert_s16_mono(&src, &mut dst);
        std::hint::black_box(&dst);
    }
    let elapsed = t0.elapsed();
    let ns_per_sample = elapsed.as_nanos() as f64 / (iters as f64 * frames as f64);
    eprintln!(
        "s16→f32: {ns_per_sample:.3} ns/sample  ({frames} frames × {iters} iters, {elapsed:?})"
    );
    assert!(dst.iter().any(|&x| x != 0.0));
    Ok(())
}

#[test]
fn test_convert_s16_mono_matches_scalar_reference() {
    // Every i16 codepoint step — including extremes — must match the
    // historical scalar formula even when NEON is active.
    let mut src = Vec::with_capacity(512 * 2);
    let mut expect = Vec::with_capacity(512);
    for (k, s) in [0i16, 1, -1, 256, -256, i16::MAX, i16::MIN, 12345, -12345]
        .into_iter()
        .chain((-250..250).map(|x| (x * 131) as i16))
        .enumerate()
    {
        let _ = k;
        src.extend_from_slice(&s.to_le_bytes());
        expect.push(s as f32 / 32_768.0);
    }
    let mut got = vec![0.0f32; expect.len()];
    crate::convert::convert_s16_mono(&src, &mut got);
    for (i, (&g, &e)) in got.iter().zip(expect.iter()).enumerate() {
        assert_eq!(
            g.to_bits(),
            e.to_bits(),
            "sample {i}: got {g} ({:x}) expect {e} ({:x})",
            g.to_bits(),
            e.to_bits()
        );
    }
}
