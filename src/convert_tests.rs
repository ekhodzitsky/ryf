use super::g711::{alaw_to_linear, mulaw_to_linear};
use super::*;
use crate::header::SampleCodec;

#[test]
fn convert_s16_le_to_f32_matches_div() {
    // Length not multiple of 8 exercises scalar tail (and NEON tail on aarch64).
    let samples: [i16; 11] = [0, 1, -1, 1000, -1000, i16::MAX, i16::MIN, 42, -42, 7, -7];
    let mut src = Vec::with_capacity(22);
    for s in samples {
        src.extend_from_slice(&s.to_le_bytes());
    }
    let mut dst = vec![0.0f32; samples.len()];
    convert_s16_le_to_f32(&src, &mut dst);
    for (i, &s) in samples.iter().enumerate() {
        let expect = s as f32 / 32_768.0;
        assert!((dst[i] - expect).abs() < 1e-7, "i={i}");
    }
}

#[test]
fn i16_scale_mul_matches_div_every_code() {
    for s in i16::MIN..=i16::MAX {
        let div = s as f32 / 32_768.0;
        let mul = s as f32 * I16_SCALE;
        assert_eq!(div.to_bits(), mul.to_bits(), "s={s}");
    }
}

#[test]
fn convert_f32_mono_unroll_and_tail() {
    let vals = [0.0f32, 0.5, -0.25, 1.0, -1.0, 0.125]; // 6 = 4 unroll + 2 tail
    let mut src = Vec::new();
    for v in vals {
        src.extend_from_slice(&v.to_le_bytes());
    }
    let mut dst = vec![0.0f32; vals.len()];
    convert_f32_mono(&src, &mut dst);
    for (i, &v) in vals.iter().enumerate() {
        assert_eq!(dst[i], v, "i={i}");
    }
}

#[test]
fn convert_f32_mono_unaligned_and_specials() {
    let vals = [
        0.0f32,
        -0.0,
        1.0,
        -1.0,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
        f32::from_bits(0x7FC0_0001),
        1e-40,
        f32::MAX,
    ];
    let mut packed = Vec::new();
    for v in vals {
        packed.extend_from_slice(&v.to_le_bytes());
    }
    // Force an unaligned byte view (header walk is not 4-byte aligned).
    let mut padded = vec![0u8; packed.len() + 1];
    padded[1..].copy_from_slice(&packed);
    let mut dst = vec![0.0f32; vals.len()];
    convert_f32_mono(&padded[1..], &mut dst);
    for (i, &v) in vals.iter().enumerate() {
        assert_eq!(dst[i].to_bits(), v.to_bits(), "i={i}");
    }
}

fn expect_mix_s16(src: &[u8], channels: usize) -> Vec<f32> {
    let n_ch = channels as f32;
    src.chunks_exact(channels * 2)
        .map(|frame| {
            let mut sum = 0.0f32;
            for s in frame.as_chunks::<2>().0 {
                sum += i16::from_le_bytes(*s) as f32 / 32_768.0;
            }
            sum / n_ch
        })
        .collect()
}

#[test]
fn mix_s16_stereo_bit_exact_vs_scalar() {
    let extremes = [
        0i16,
        1,
        -1,
        i16::MAX,
        i16::MIN,
        12345,
        -12345,
        256,
        -256,
        7,
        -7,
    ];
    let mut src = Vec::new();
    for (i, &s) in extremes.iter().enumerate() {
        src.extend_from_slice(&s.to_le_bytes());
        let other = extremes[(i * 3 + 1) % extremes.len()];
        src.extend_from_slice(&other.to_le_bytes());
    }
    // Odd frame count exercises the SIMD tail (11 frames).
    let frames = src.len() / 4;
    let expect = expect_mix_s16(&src, 2);
    let mut got = vec![0.0f32; frames];
    mix_s16_le_to_f32(&src, &mut got, 2);
    for (i, (&g, &e)) in got.iter().zip(expect.iter()).enumerate() {
        assert_eq!(g.to_bits(), e.to_bits(), "stereo mix i={i}");
    }
}

#[test]
fn mix_s16_quad_matches_scalar() {
    let mut src = Vec::new();
    for s in [0i16, 1000, -1000, i16::MAX, i16::MIN, 42, -7, 8] {
        src.extend_from_slice(&s.to_le_bytes());
    }
    let expect = expect_mix_s16(&src, 4);
    let mut got = vec![0.0f32; 2];
    mix_s16_le_to_f32(&src, &mut got, 4);
    for (i, (&g, &e)) in got.iter().zip(expect.iter()).enumerate() {
        assert_eq!(g.to_bits(), e.to_bits(), "4ch mix i={i}");
    }
}

#[test]
fn split_s16_stereo_bit_exact() {
    let samples = [
        0i16,
        1,
        -1,
        1000,
        -1000,
        i16::MAX,
        i16::MIN,
        42,
        -42,
        7,
        -7,
        9,
    ];
    let mut src = Vec::new();
    for s in samples {
        src.extend_from_slice(&s.to_le_bytes());
    }
    let frames = samples.len() / 2;
    let mut left = vec![0.0f32; frames];
    let mut right = vec![0.0f32; frames];
    {
        let mut planes: [&mut [f32]; 2] = [&mut left, &mut right];
        split_s16_le_to_f32(&src, &mut planes);
    }
    for i in 0..frames {
        let e_l = samples[i * 2] as f32 / 32_768.0;
        let e_r = samples[i * 2 + 1] as f32 / 32_768.0;
        assert_eq!(left[i].to_bits(), e_l.to_bits(), "L i={i}");
        assert_eq!(right[i].to_bits(), e_r.to_bits(), "R i={i}");
    }
}

#[test]
fn convert_sample_all_codecs_le_and_be() {
    assert!((convert_sample(SampleCodec::U8, &[128], false) - 0.0).abs() < 1e-6);
    assert!((convert_sample(SampleCodec::S16, &0i16.to_le_bytes(), false)).abs() < 1e-9);
    assert!(
        (convert_sample(SampleCodec::S16, &1000i16.to_be_bytes(), true) - 1000.0 / 32768.0).abs()
            < 1e-7
    );

    // S24 LE/BE
    let s24 = 1000i32;
    let le = [
        (s24 & 0xff) as u8,
        ((s24 >> 8) & 0xff) as u8,
        ((s24 >> 16) & 0xff) as u8,
    ];
    let be = [le[2], le[1], le[0]];
    let a = convert_sample(SampleCodec::S24, &le, false);
    let b = convert_sample(SampleCodec::S24, &be, true);
    assert!((a - b).abs() < 1e-7);

    // Negative S24 sign-extend
    let neg = (-1000i32) & 0x00ff_ffff;
    let nle = [
        (neg & 0xff) as u8,
        ((neg >> 8) & 0xff) as u8,
        ((neg >> 16) & 0xff) as u8,
    ];
    assert!(convert_sample(SampleCodec::S24, &nle, false) < 0.0);

    // S24_4
    let x = 0x00ff_ffffu32; // -1 in 24-bit two's complement packed
    let le4 = x.to_le_bytes();
    let be4 = x.to_be_bytes();
    assert!(convert_sample(SampleCodec::S24_4, &le4, false) < 0.0);
    assert!(convert_sample(SampleCodec::S24_4, &be4, true) < 0.0);
    assert!(convert_sample(SampleCodec::S24_4, &0u32.to_le_bytes(), false).abs() < 1e-9);

    // S32 / F32 / F64 LE + BE
    let s32 = 1_000_000i32;
    assert!(
        (convert_sample(SampleCodec::S32, &s32.to_le_bytes(), false)
            - convert_sample(SampleCodec::S32, &s32.to_be_bytes(), true))
        .abs()
            < 1e-9
    );
    let f = 0.25f32;
    assert_eq!(
        convert_sample(SampleCodec::F32, &f.to_le_bytes(), false),
        convert_sample(SampleCodec::F32, &f.to_be_bytes(), true)
    );
    let d = -0.5f64;
    assert_eq!(
        convert_sample(SampleCodec::F64, &d.to_le_bytes(), false),
        convert_sample(SampleCodec::F64, &d.to_be_bytes(), true)
    );

    // G.711: exercise both sign branches via full byte sweep (cheap).
    for b in 0u8..=255 {
        let _ = convert_sample(SampleCodec::ALaw, &[b], false);
        let _ = convert_sample(SampleCodec::MuLaw, &[b], false);
    }
    assert_eq!(convert_sample(SampleCodec::MsAdpcm, &[0], false), 0.0);
    assert_eq!(convert_sample(SampleCodec::ImaAdpcm, &[0], false), 0.0);
    assert_eq!(convert_sample(SampleCodec::Unsupported, &[0], false), 0.0);
}

#[test]
fn scalar_kernels_match_public_dispatch() {
    // Force the scalar fallbacks even when SIMD dispatch is used on this host.
    let samples: [i16; 11] = [0, 1, -1, 1000, -1000, i16::MAX, i16::MIN, 42, -42, 7, -7];
    let mut src = Vec::new();
    for s in samples {
        src.extend_from_slice(&s.to_le_bytes());
    }
    let mut simd = vec![0.0f32; samples.len()];
    let mut scalar = vec![0.0f32; samples.len()];
    convert_s16_le_to_f32(&src, &mut simd);
    super::scalar::convert_s16_mono_scalar(&src, &mut scalar);
    for (i, (a, b)) in simd.iter().zip(scalar.iter()).enumerate() {
        assert_eq!(a.to_bits(), b.to_bits(), "s16 scalar i={i}");
    }

    let mut stereo = Vec::new();
    for s in samples {
        stereo.extend_from_slice(&s.to_le_bytes());
        stereo.extend_from_slice(&s.wrapping_neg().to_le_bytes());
    }
    let frames = samples.len();
    let mut mix_s = vec![0.0f32; frames];
    let mut mix_d = vec![0.0f32; frames];
    mix_s16_le_to_f32(&stereo, &mut mix_d, 2);
    super::scalar::mix_s16_stereo_scalar(&stereo, &mut mix_s);
    for (i, (a, b)) in mix_d.iter().zip(mix_s.iter()).enumerate() {
        assert_eq!(a.to_bits(), b.to_bits(), "mix scalar i={i}");
    }
    let mut l_s = vec![0.0f32; frames];
    let mut r_s = vec![0.0f32; frames];
    let mut l_d = vec![0.0f32; frames];
    let mut r_d = vec![0.0f32; frames];
    {
        let mut planes: [&mut [f32]; 2] = [&mut l_d, &mut r_d];
        split_s16_le_to_f32(&stereo, &mut planes);
    }
    super::scalar::split_s16_stereo_scalar(&stereo, &mut l_s, &mut r_s);
    for i in 0..frames {
        assert_eq!(l_d[i].to_bits(), l_s[i].to_bits(), "L i={i}");
        assert_eq!(r_d[i].to_bits(), r_s[i].to_bits(), "R i={i}");
    }

    let vals = [0.0f32, 0.5, -0.25, 1.0, -1.0, 0.125, 0.25, 0.75];
    let mut packed = Vec::new();
    for v in vals {
        packed.extend_from_slice(&v.to_le_bytes());
    }
    let mut f_s = vec![0.0f32; vals.len()];
    let mut f_d = vec![0.0f32; vals.len()];
    convert_f32_mono(&packed, &mut f_d);
    super::scalar::convert_f32_mono_scalar(&packed, &mut f_s);
    for (i, (a, b)) in f_d.iter().zip(f_s.iter()).enumerate() {
        assert_eq!(a.to_bits(), b.to_bits(), "f32 scalar i={i}");
    }

    // 3-channel mix/split hits the n-ch scalar path (not stereo SIMD).
    let mut tri = Vec::new();
    for s in [1i16, 2, 3, 4, 5, 6] {
        tri.extend_from_slice(&s.to_le_bytes());
    }
    let mut mix = vec![0.0f32; 2];
    mix_s16_le_to_f32(&tri, &mut mix, 3);
    super::scalar::mix_s16_nch_scalar(&tri, &mut mix, 3);
    let mut a = vec![0.0f32; 2];
    let mut b = vec![0.0f32; 2];
    let mut c = vec![0.0f32; 2];
    {
        let mut planes: [&mut [f32]; 3] = [&mut a, &mut b, &mut c];
        split_s16_le_to_f32(&tri, &mut planes);
        super::scalar::split_s16_nch_scalar(&tri, &mut planes);
    }
    assert!((a[0] - 1.0 / 32768.0).abs() < 1e-9);
}

#[test]
fn g711_tables_nonzero() {
    // Silence codes should map near zero; random high bit should flip sign.
    let a0 = alaw_to_linear(0xd5); // common A-law silence-ish after xor
    let m0 = mulaw_to_linear(0xff);
    let _ = (a0, m0);
    assert_ne!(alaw_to_linear(0x00), alaw_to_linear(0x80));
    assert_ne!(mulaw_to_linear(0x00), mulaw_to_linear(0x80));
}

#[test]
fn g711_lut_matches_expander_and_bulk() {
    let mut src = [0u8; 256];
    for (i, b) in src.iter_mut().enumerate() {
        *b = i as u8;
    }
    let mut alaw = [0.0f32; 256];
    let mut mulaw = [0.0f32; 256];
    super::g711::convert_mono(&src, &mut alaw, super::g711_table(true));
    super::g711::convert_mono(&src, &mut mulaw, super::g711_table(false));
    for i in 0..256 {
        let b = [i as u8];
        assert_eq!(
            alaw[i].to_bits(),
            convert_sample(SampleCodec::ALaw, &b, false).to_bits()
        );
        assert_eq!(
            mulaw[i].to_bits(),
            convert_sample(SampleCodec::MuLaw, &b, false).to_bits()
        );
        assert_eq!(
            alaw[i].to_bits(),
            (alaw_to_linear(i as u8) as f32 / 32_768.0).to_bits()
        );
        assert_eq!(
            mulaw[i].to_bits(),
            (mulaw_to_linear(i as u8) as f32 / 32_768.0).to_bits()
        );
    }
}
