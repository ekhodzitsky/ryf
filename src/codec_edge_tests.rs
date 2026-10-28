use crate::{DecodeOptions, WriteSpec, decode_bytes, decode_g711, encode_extensible};

#[cfg(feature = "adpcm")]
fn chunk(out: &mut Vec<u8>, tag: &[u8; 4], body: &[u8]) {
    out.extend_from_slice(tag);
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(body);
    if body.len() % 2 == 1 {
        out.push(0);
    }
}

#[cfg(feature = "adpcm")]
fn adpcm_wave(ima: bool, channels: u16, coefficient: i16, fact: Option<u32>) -> Vec<u8> {
    let mut fmt = Vec::new();
    let block = 8 * channels;
    for n in [if ima { 17u16 } else { 2 }, channels] {
        fmt.extend_from_slice(&n.to_le_bytes());
    }
    for n in [8000u32, 16000] {
        fmt.extend_from_slice(&n.to_le_bytes());
    }
    for n in [block, 4, if ima { 2 } else { 8 }, if ima { 9 } else { 4 }] {
        fmt.extend_from_slice(&n.to_le_bytes());
    }
    if !ima {
        for n in [1, coefficient, coefficient] {
            fmt.extend_from_slice(&n.to_le_bytes());
        }
    }
    let mut data = vec![0u8; usize::from(block)];
    if !ima {
        for c in 0..usize::from(channels) {
            data[usize::from(channels) + 2 * c..usize::from(channels) + 2 * c + 2]
                .copy_from_slice(&16i16.to_le_bytes());
            for start in [3 * usize::from(channels), 5 * usize::from(channels)] {
                data[start + 2 * c..start + 2 * c + 2].copy_from_slice(&i16::MIN.to_le_bytes());
            }
        }
    }
    let mut out = b"RIFF\0\0\0\0WAVE".to_vec();
    chunk(&mut out, b"fmt ", &fmt);
    if let Some(n) = fact {
        chunk(&mut out, b"fact", &n.to_le_bytes());
    }
    chunk(&mut out, b"data", &data);
    let size = out.len() as u32 - 8;
    out[4..8].copy_from_slice(&size.to_le_bytes());
    out
}

#[cfg(feature = "adpcm")]
#[test]
fn custom_adpcm_coefficients_do_not_overflow() -> crate::Result<()> {
    for ch in [1, 2] {
        let d = decode_bytes(
            &adpcm_wave(false, ch, i16::MIN, None),
            DecodeOptions::default(),
        )?;
        for plane in d.channels {
            assert_eq!(plane[2], f32::from(i16::MAX) / 32768.0);
        }
    }
    Ok(())
}

#[cfg(feature = "adpcm")]
#[test]
fn adpcm_fact_is_applied_before_duration() -> crate::Result<()> {
    for ima in [false, true] {
        for ch in [1, 2] {
            let bytes = adpcm_wave(ima, ch, 256, Some(1));
            let opts = DecodeOptions::default().with_max_duration_secs(1.0 / 8000.0);
            assert_eq!(decode_bytes(&bytes, opts.clone())?.frames(), 1);
            let mut src = crate::ByteSource::from_slice(&bytes);
            let mut got = 0;
            crate::decode_streaming(&mut src, &opts, |b| {
                got += b.frames;
                Ok(())
            })?;
            assert_eq!(got, 1);
        }
    }
    Ok(())
}

#[test]
fn standard_extensible_mulaw_and_legacy_input() -> crate::Result<()> {
    let mut bytes = encode_extensible(WriteSpec::mulaw(8000, 1), &[0xff, 0x80])?;
    assert_eq!(
        &bytes[44..60],
        &[7, 0, 0, 0, 0, 0, 16, 0, 128, 0, 0, 170, 0, 56, 155, 113]
    );
    let standard = decode_bytes(&bytes, DecodeOptions::default())?;
    bytes[44] = 4;
    assert_eq!(
        decode_bytes(&bytes, DecodeOptions::default())?.channels,
        standard.channels
    );
    Ok(())
}

#[test]
fn g711_encoder_full_pcm16_domain() -> crate::Result<()> {
    let samples: Vec<f32> = (i16::MIN..=i16::MAX)
        .map(|x| f32::from(x) / 32768.0)
        .collect();
    // FNV-1a of all encoded i16 values in ascending order, ffmpeg 7.0.2.
    for (encode, expected) in [
        (
            crate::encode_alaw as fn(&[f32], u32, u16) -> crate::Result<Vec<u8>>,
            0xb692762fb973cb11u64,
        ),
        (crate::encode_mulaw, 0xa73378df9212a731u64),
    ] {
        let bytes = encode(&samples, 8000, 1)?;
        let hash = bytes[58..].iter().fold(14695981039346656037u64, |h, b| {
            (h ^ u64::from(*b)).wrapping_mul(1099511628211)
        });
        assert_eq!(hash, expected);
    }
    for (law, input, expected) in [
        (crate::G711Law::ALaw, -1, -8),
        (crate::G711Law::MuLaw, 121, 120),
    ] {
        let samples = [input as f32 / 32768.0];
        let bytes = if law == crate::G711Law::ALaw {
            crate::encode_alaw(&samples, 8000, 1)?
        } else {
            crate::encode_mulaw(&samples, 8000, 1)?
        };
        let decoded = decode_g711(&bytes[58..], law, 8000, 1, &DecodeOptions::default())?;
        assert_eq!(decoded.channels[0][0], expected as f32 / 32768.0);
    }
    Ok(())
}

#[cfg(all(feature = "adpcm", not(miri)))]
proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(128))]
    #[test]
    fn proptest_structured_ms_coefficients_do_not_panic(
        first in proptest::num::i16::ANY,
        second in proptest::num::i16::ANY,
        history in proptest::num::i16::ANY,
        nibble in proptest::num::u8::ANY,
        stereo in proptest::bool::ANY,
    ) {
        let channels=if stereo {2u16}else{1};
        let mut bytes=adpcm_wave(false,channels,first,None);
        bytes[44..46].copy_from_slice(&second.to_le_bytes());
        let data_start=54usize;
        for c in 0..usize::from(channels) {
            for start in [3*usize::from(channels),5*usize::from(channels)] {
                bytes[data_start+start+2*c..data_start+start+2*c+2].copy_from_slice(&history.to_le_bytes());
            }
        }
        *bytes.last_mut().ok_or_else(||proptest::test_runner::TestCaseError::fail("missing block"))?=nibble;
        let decoded=decode_bytes(&bytes,DecodeOptions::default()).map_err(|e|proptest::test_runner::TestCaseError::fail(e.to_string()))?;
        proptest::prop_assert_eq!(decoded.channels.len(),usize::from(channels));
        proptest::prop_assert!(decoded.channels.iter().flatten().all(|s|s.is_finite()));
    }
}

#[test]
fn mono_s16_collect_all_values() -> crate::Result<()> {
    let pcm: Vec<u8> = (i16::MIN..=i16::MAX).flat_map(i16::to_le_bytes).collect();
    let wave = crate::encode_s16(&pcm, 8000)?;
    let decoded = decode_bytes(&wave, DecodeOptions::speech())?;
    assert_eq!(decoded.frames(), 65536);
    for (sample, expected) in decoded.channels[0].iter().zip(i16::MIN..=i16::MAX) {
        assert_eq!(sample.to_bits(), (f32::from(expected) / 32768.0).to_bits());
    }
    Ok(())
}

#[cfg(not(feature = "adpcm"))]
#[test]
fn disabled_adpcm_rejects_before_decode_budgets() {
    let bytes = include_bytes!("../fixtures/ima_adpcm_16k_mono.wav");
    let options = DecodeOptions::default()
        .with_max_duration_secs(0.0)
        .with_max_output_bytes(0);
    assert!(matches!(
        decode_bytes(bytes, options.clone()),
        Err(crate::WavError::FeatureDisabled { feature: "adpcm" })
    ));
    assert!(matches!(
        crate::probe_with(&mut crate::ByteSource::from_slice(bytes), &options),
        Err(crate::WavError::FeatureDisabled { feature: "adpcm" })
    ));
}
