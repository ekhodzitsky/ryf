use super::{G726Decoder, G726Packing, G726Rate, decode_g726, decode_g726_mono, decode_i16};
use crate::convert::I16_SCALE;
use crate::error::{FormatKind, WavError};
use crate::options::DecodeOptions;

fn synth(n: usize) -> Vec<i16> {
    let mut pcm = vec![0i16; n];
    let step_at = n / 4;
    let step_len = (n / 16).clamp(1, 64);
    for sample in pcm.iter_mut().take(step_at + step_len).skip(step_at) {
        *sample = 12_000;
    }
    let tone = step_at + step_len;
    for (i, sample) in pcm.iter_mut().enumerate().skip(tone) {
        let t = (i - tone) as f64;
        let s = (t * 440.0 * std::f64::consts::TAU / 8_000.0).sin();
        *sample = (s * 8_000.0).round() as i16;
    }
    pcm
}

fn s16le(bytes: &[u8]) -> Vec<i16> {
    bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| i16::from_le_bytes(*c))
        .collect()
}

fn ffmpeg_available() -> bool {
    std::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

fn ffmpeg_pair(pcm: &[i16], codec: &str, bits: u8) -> crate::Result<(Vec<u8>, Vec<i16>)> {
    let dir = tempfile::TempDir::new()?;
    let pcm_path = dir.path().join("in.pcm");
    let bin_path = dir.path().join("out.bin");
    let s16_path = dir.path().join("out.s16");
    let mut raw = Vec::with_capacity(pcm.len() * 2);
    for sample in pcm {
        raw.extend_from_slice(&sample.to_le_bytes());
    }
    std::fs::write(&pcm_path, raw)?;
    let bits = bits.to_string();
    let enc = std::process::Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "s16le",
            "-ar",
            "8000",
            "-ac",
            "1",
            "-i",
        ])
        .arg(&pcm_path)
        .args(["-c:a", codec, "-code_size", &bits, "-f", codec, "-y"])
        .arg(&bin_path)
        .status()?;
    assert!(enc.success(), "ffmpeg {codec} encode failed");
    let dec = std::process::Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            codec,
            "-code_size",
            &bits,
            "-i",
        ])
        .arg(&bin_path)
        .args(["-f", "s16le", "-y"])
        .arg(&s16_path)
        .status()?;
    assert!(dec.success(), "ffmpeg {codec} decode failed");
    Ok((std::fs::read(&bin_path)?, s16le(&std::fs::read(&s16_path)?)))
}

#[test]
#[cfg_attr(miri, ignore = "Miri cannot spawn the external ffmpeg oracle")]
fn ffmpeg_i16_matches_all_rates_and_packings() -> crate::Result<()> {
    if !ffmpeg_available() {
        eprintln!("ffmpeg not available, skipping g726 oracle");
        return Ok(());
    }
    let opts = DecodeOptions::unbounded();
    for n in [160usize, 16_000] {
        let pcm = synth(n);
        for (bits, rate) in rates() {
            for (codec, packing) in packings() {
                let (bin, reference) = ffmpeg_pair(&pcm, codec, bits)?;
                let got = decode_i16(&bin, rate, packing, None, &opts)?;
                assert_eq!(got, reference, "{codec} code_size={bits} n={n}");
                if n == 160 {
                    let frozen = frozen_bin(bits, packing);
                    assert_eq!(bin, frozen, "fixture drifted from ffmpeg 7.0.2");
                }
            }
        }
    }
    Ok(())
}

fn rates() -> [(u8, G726Rate); 4] {
    [
        (2, G726Rate::Kbps16),
        (3, G726Rate::Kbps24),
        (4, G726Rate::Kbps32),
        (5, G726Rate::Kbps40),
    ]
}

fn packings() -> [(&'static str, G726Packing); 2] {
    [
        ("g726", G726Packing::Aal2),
        ("g726le", G726Packing::Rfc3551),
    ]
}

fn frozen_bin(bits: u8, packing: G726Packing) -> &'static [u8] {
    match (bits, packing) {
        (2, G726Packing::Aal2) => include_bytes!("../fixtures/g726/aal2-2.bin"),
        (3, G726Packing::Aal2) => include_bytes!("../fixtures/g726/aal2-3.bin"),
        (4, G726Packing::Aal2) => include_bytes!("../fixtures/g726/aal2-4.bin"),
        (5, G726Packing::Aal2) => include_bytes!("../fixtures/g726/aal2-5.bin"),
        (2, G726Packing::Rfc3551) => include_bytes!("../fixtures/g726/rfc3551-2.bin"),
        (3, G726Packing::Rfc3551) => include_bytes!("../fixtures/g726/rfc3551-3.bin"),
        (4, G726Packing::Rfc3551) => include_bytes!("../fixtures/g726/rfc3551-4.bin"),
        (5, G726Packing::Rfc3551) => include_bytes!("../fixtures/g726/rfc3551-5.bin"),
        _ => &[],
    }
}

fn frozen_pcm(bits: u8) -> &'static [u8] {
    match bits {
        2 => include_bytes!("../fixtures/g726/pcm-2.s16"),
        3 => include_bytes!("../fixtures/g726/pcm-3.s16"),
        4 => include_bytes!("../fixtures/g726/pcm-4.s16"),
        5 => include_bytes!("../fixtures/g726/pcm-5.s16"),
        _ => &[],
    }
}

#[test]
fn fixtures_match_integers_then_f32_scale() -> crate::Result<()> {
    let tone = s16le(include_bytes!("../fixtures/g726/tone.s16"));
    assert_eq!(tone, synth(160));
    let opts = DecodeOptions::unbounded();
    for (bits, rate) in rates() {
        let reference = s16le(frozen_pcm(bits));
        assert_eq!(reference.len(), 160);
        for packing in [G726Packing::Aal2, G726Packing::Rfc3551] {
            let bin = frozen_bin(bits, packing);
            let got = decode_i16(bin, rate, packing, None, &opts)?;
            assert_eq!(got, reference, "bits={bits} {packing:?}");
            let wav = decode_g726(bin, rate, packing, None, &opts)?;
            assert_eq!(wav.sample_rate, 8_000);
            assert_eq!(wav.num_channels(), 1);
            assert_eq!(wav.frames(), 160);
            for (sample, pcm) in wav.channels[0].iter().zip(&reference) {
                let scaled = f32::from(*pcm) * I16_SCALE;
                assert_eq!(sample.to_bits(), scaled.to_bits());
            }
            let swapped = if packing == G726Packing::Aal2 {
                G726Packing::Rfc3551
            } else {
                G726Packing::Aal2
            };
            let wrong = decode_i16(bin, rate, swapped, None, &opts)?;
            assert_ne!(wrong, reference);
        }
    }
    let mono = decode_g726_mono(
        frozen_bin(4, G726Packing::Rfc3551),
        G726Rate::Kbps32,
        G726Packing::Rfc3551,
    )?;
    assert_eq!(mono.frames(), 160);
    Ok(())
}

#[test]
fn raw_count_padding_and_limits() -> crate::Result<()> {
    let opts = DecodeOptions::unbounded();
    let bin = frozen_bin(4, G726Packing::Aal2);
    let rate = G726Rate::Kbps32;
    let packing = G726Packing::Aal2;
    assert!(matches!(
        decode_g726(&[], rate, packing, None, &opts),
        Err(WavError::Empty)
    ));
    assert!(matches!(
        decode_g726(bin, rate, packing, Some(0), &opts),
        Err(WavError::Format(FormatKind::InvalidSize))
    ));
    assert!(matches!(
        decode_g726(bin, rate, packing, Some(161), &opts),
        Err(WavError::Format(FormatKind::InvalidSize))
    ));
    let mut padded = bin.to_vec();
    padded.extend_from_slice(&[0xff, 0x00]);
    let full = decode_g726(bin, rate, packing, None, &opts)?;
    let capped = decode_g726(&padded, rate, packing, Some(160), &opts)?;
    assert_eq!(capped.frames(), 160);
    assert_eq!(capped.channels[0], full.channels[0]);
    let mut odd = frozen_bin(3, G726Packing::Aal2).to_vec();
    odd.push(0xa5);
    assert!(matches!(
        decode_g726(&odd, G726Rate::Kbps24, G726Packing::Aal2, None, &opts),
        Err(WavError::Format(FormatKind::InvalidSize))
    ));
    let kept = decode_g726(&odd, G726Rate::Kbps24, G726Packing::Aal2, Some(160), &opts)?;
    assert_eq!(kept.frames(), 160);
    let mut five = frozen_bin(5, G726Packing::Rfc3551).to_vec();
    five.push(0x01);
    assert!(matches!(
        decode_g726(&five, G726Rate::Kbps40, G726Packing::Rfc3551, None, &opts),
        Err(WavError::Format(FormatKind::InvalidSize))
    ));
    let short = DecodeOptions::speech().with_max_duration_secs(0.001);
    assert!(matches!(
        decode_g726(bin, rate, packing, None, &short),
        Err(WavError::TooLong { .. })
    ));
    let ram = DecodeOptions::speech().with_max_output_bytes(8);
    assert!(matches!(
        decode_g726(bin, rate, packing, None, &ram),
        Err(WavError::OutputTooLarge { .. })
    ));
    let slow = DecodeOptions::speech().with_max_sample_rate(1_000);
    assert!(matches!(
        decode_g726(bin, rate, packing, None, &slow),
        Err(WavError::UnsupportedSampleRate { rate: 8_000, .. })
    ));
    Ok(())
}

#[test]
fn incremental_matches_one_shot_and_finish_reset() -> crate::Result<()> {
    let opts = DecodeOptions::unbounded();
    for (bits, rate) in rates() {
        for packing in [G726Packing::Aal2, G726Packing::Rfc3551] {
            let bin = frozen_bin(bits, packing);
            let one = decode_i16(bin, rate, packing, None, &opts)?;
            for step in [1usize, 2, 3, 5, 7] {
                let mut dec = G726Decoder::new(rate, packing);
                let mut out = vec![0i16; one.len()];
                let mut at = 0usize;
                let mut filled = 0usize;
                while at < bin.len() {
                    let end = (at + step).min(bin.len());
                    let (n, consumed) = dec.push(&bin[at..end], &mut out[filled..]);
                    assert_eq!(consumed, end - at);
                    filled += n;
                    at = end;
                }
                assert_eq!(filled, one.len());
                assert_eq!(out, one, "step {step} bits={bits}");
                assert_eq!(dec.finish(), 0);
            }
            let mut dec = G726Decoder::new(rate, packing);
            let mut window = [0i16; 3];
            let mut got = Vec::new();
            let mut rest = bin;
            loop {
                let (n, consumed) = dec.push(rest, &mut window);
                if n == 0 && consumed == 0 {
                    break;
                }
                got.extend_from_slice(&window[..n]);
                rest = &rest[consumed..];
            }
            assert_eq!(got, one);
            assert_eq!(dec.push(bin, &mut []), (0, 0));
            assert_eq!(dec.pending_bits(), 0);
        }
    }
    let bin = frozen_bin(3, G726Packing::Aal2);
    let mut dec = G726Decoder::new(G726Rate::Kbps24, G726Packing::Aal2);
    let mut buf = [0i16; 8];
    let (n, consumed) = dec.push(&bin[..1], &mut buf);
    assert_eq!((n, consumed), (2, 1));
    assert_eq!(dec.pending_bits(), 2);
    assert_eq!(dec.finish(), 2);
    assert_eq!(dec.pending_bits(), 0);
    let mut kept = G726Decoder::new(G726Rate::Kbps40, G726Packing::Rfc3551);
    let mut wiped = G726Decoder::new(G726Rate::Kbps40, G726Packing::Rfc3551);
    let five = frozen_bin(5, G726Packing::Rfc3551);
    let mut scratch = [0i16; 64];
    kept.push(&five[..31], &mut scratch);
    wiped.push(&five[..31], &mut scratch);
    let pending = kept.pending_bits();
    assert_eq!(pending, 3);
    assert_eq!(kept.finish(), pending);
    assert_eq!(kept.pending_bits(), 0);
    wiped.finish();
    wiped.reset();
    let mut a = [0i16; 32];
    let mut b = [0i16; 32];
    let (na, _) = kept.push(&five[31..51], &mut a);
    let (nb, _) = wiped.push(&five[31..51], &mut b);
    assert!(na > 0 && nb > 0);
    assert_ne!(a[..na], b[..nb]);
    wiped.reset();
    let mut again = [0i16; 64];
    let (n1, _) = wiped.push(&five[..31], &mut scratch);
    let (n2, _) = {
        let mut fresh = G726Decoder::new(G726Rate::Kbps40, G726Packing::Rfc3551);
        fresh.push(&five[..31], &mut again)
    };
    assert_eq!(n1, n2);
    assert_eq!(scratch[..n1], again[..n2]);
    Ok(())
}

fn append_sine(pcm: &mut Vec<i16>, n: usize, hz: f64, amp: f64) {
    let start = pcm.len();
    for i in 0..n {
        let t = (start + i) as f64;
        let s = (t * hz * std::f64::consts::TAU / 8_000.0).sin();
        pcm.push((s * amp).round() as i16);
    }
}

/// Full-scale random signs. xorshift32, seed `0xA5A5_1234`.
fn full_scale_signs(n: usize) -> Vec<i16> {
    let mut x = 0xA5A5_1234u32;
    let mut pcm = Vec::with_capacity(n);
    for _ in 0..n {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        pcm.push(if (x & 1) == 0 { 30_000 } else { -30_000 });
    }
    pcm
}

/// Loud random signs, then a 2600 Hz tone, then the same tone much louder.
fn tone_reset_pcm() -> Vec<i16> {
    let mut pcm = full_scale_signs(800);
    append_sine(&mut pcm, 504, 2_600.0, 4_000.0);
    append_sine(&mut pcm, 320, 2_600.0, 32_000.0);
    pcm
}

fn reset_bin(bits: u8, packing: G726Packing) -> &'static [u8] {
    match (bits, packing) {
        (2, G726Packing::Aal2) => include_bytes!("../fixtures/g726/reset-aal2-2.bin"),
        (5, G726Packing::Aal2) => include_bytes!("../fixtures/g726/reset-aal2-5.bin"),
        (2, G726Packing::Rfc3551) => include_bytes!("../fixtures/g726/reset-rfc3551-2.bin"),
        (5, G726Packing::Rfc3551) => include_bytes!("../fixtures/g726/reset-rfc3551-5.bin"),
        _ => &[],
    }
}

fn reset_pcm_bytes(bits: u8) -> &'static [u8] {
    match bits {
        2 => include_bytes!("../fixtures/g726/reset-pcm-2.s16"),
        5 => include_bytes!("../fixtures/g726/reset-pcm-5.s16"),
        _ => &[],
    }
}

#[test]
fn tone_reset_matches_ffmpeg() -> crate::Result<()> {
    // 800 full-scale signs, then 504 samples of 2600 Hz at amplitude 4000,
    // then 320 samples of that tone at 32000. The jump sets tr once.
    // ylint > 9 does not run for legal codewords: yu is clipped to 5120,
    // and yl + yu + ((-yl) >> 6) cannot pass 327617. That value shifted
    // right by 15 is 9. The high arm needs yl >= 327680. This vector
    // reaches yl 327515 (16 kbit/s) and 323211 (40 kbit/s). An 8000-sample
    // run of the same signs, below, sits on 327617 and 324079.
    let pcm = tone_reset_pcm();
    assert_eq!(pcm.len(), 1_624);
    assert_eq!(pcm, s16le(include_bytes!("../fixtures/g726/reset.s16")));
    let opts = DecodeOptions::unbounded();
    // Miri cannot spawn processes; fixture and decoder-state checks still run.
    let ffmpeg = !cfg!(miri) && ffmpeg_available();
    let expect = [
        (2u8, G726Rate::Kbps16, 327_515, 1_308),
        (5, G726Rate::Kbps40, 323_211, 1_305),
    ];
    for (bits, rate, max_yl, first_tr) in expect {
        let reference = s16le(reset_pcm_bytes(bits));
        assert_eq!(reference.len(), pcm.len());
        for (codec, packing) in packings() {
            let bin = reset_bin(bits, packing);
            super::decode::tone_counters_reset();
            let got = decode_i16(bin, rate, packing, None, &opts)?;
            assert_eq!(got, reference, "{codec} bits={bits}");
            let (tr, hi, yl, first, hi_at, n) = super::decode::tone_counters();
            assert_eq!(n, 1_624, "{codec} bits={bits}");
            assert_eq!(tr, 1, "{codec} bits={bits}");
            assert_eq!(first, first_tr, "{codec} bits={bits}");
            assert_eq!(hi, 0, "{codec} bits={bits}");
            assert_eq!(hi_at, -1, "{codec} bits={bits}");
            assert_eq!(yl, max_yl, "{codec} bits={bits}");
            assert_eq!(yl >> 15, 9, "{codec} bits={bits}");
            if ffmpeg {
                let (encoded, decoded) = ffmpeg_pair(&pcm, codec, bits)?;
                assert_eq!(encoded, bin, "{codec} bits={bits} bitstream drifted");
                assert_eq!(decoded, reference, "{codec} bits={bits} ffmpeg drifted");
            }
        }
    }
    if ffmpeg {
        let hot = full_scale_signs(8_000);
        let ceiling = [
            (2u8, G726Rate::Kbps16, 327_617),
            (5, G726Rate::Kbps40, 324_079),
        ];
        for (bits, rate, max_yl) in ceiling {
            let (bin, reference) = ffmpeg_pair(&hot, "g726", bits)?;
            super::decode::tone_counters_reset();
            let got = decode_i16(&bin, rate, G726Packing::Aal2, None, &opts)?;
            assert_eq!(got, reference, "signs bits={bits}");
            let (tr, hi, yl, _, _, n) = super::decode::tone_counters();
            assert_eq!(n, 8_000, "signs bits={bits}");
            assert_eq!(tr, 0, "signs bits={bits}");
            assert_eq!(hi, 0, "signs bits={bits}");
            assert_eq!(yl, max_yl, "signs bits={bits}");
            assert_eq!(yl >> 15, 9, "signs bits={bits}");
        }
    }
    Ok(())
}
