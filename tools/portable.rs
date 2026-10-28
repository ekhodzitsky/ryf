//! Dependency-free public API exercise for cross-target execution.
//! Run using the manifest and commands documented in docs/correctness.md.
use ryf::{ByteSource, ChannelMode, DecodeOptions, WriteSpec};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_pointer_width = "32")]
    check_large_collect_capacity()?;
    let mut hash = 14695981039346656037u64;
    for channels in [1u16, 2, 3, 26] {
        // Odd frame counts exercise SIMD tails; extrema exercise saturation.
        let pcm: Vec<u8> = (0..usize::from(channels) * 257)
            .flat_map(|i| (i as i16).wrapping_mul(319).to_le_bytes())
            .collect();
        for encode in [
            ryf::encode,
            ryf::encode_rifx,
            ryf::encode_rf64,
            ryf::encode_extensible,
        ] {
            let wav = encode(WriteSpec::s16(48000, channels), &pcm)?;
            for mode in [ChannelMode::Split, ChannelMode::Mono] {
                let options = DecodeOptions::default().with_channel_mode(mode);
                let decoded = ryf::decode_bytes(&wav, options.clone())?;
                let mut source = ByteSource::from_read_seek(std::io::Cursor::new(&wav), None);
                let mut streamed = vec![Vec::new(); decoded.channels.len()];
                ryf::decode_streaming(&mut source, &options, |block| {
                    for (dst, plane) in streamed.iter_mut().zip(block.planar) {
                        dst.extend_from_slice(plane);
                    }
                    Ok(())
                })?;
                assert_eq!(streamed, decoded.channels);
                for frame in 0..257 {
                    let values: Vec<f32> = pcm[frame * usize::from(channels) * 2..]
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .take(usize::from(channels))
                        .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.0)
                        .collect();
                    if mode == ChannelMode::Mono {
                        let expected = values.iter().copied().sum::<f32>() / f32::from(channels);
                        assert_eq!(decoded.channels[0][frame].to_bits(), expected.to_bits());
                    } else {
                        for (plane, expected) in decoded.channels.iter().zip(values) {
                            assert_eq!(plane[frame].to_bits(), expected.to_bits());
                        }
                    }
                }
                for value in decoded.channels.iter().flatten() {
                    hash = (hash ^ u64::from(value.to_bits())).wrapping_mul(1099511628211);
                }
            }
        }
    }
    for wav in [
        include_bytes!("../fixtures/ms_adpcm_16k_mono.wav").as_slice(),
        include_bytes!("../fixtures/ms_adpcm_16k_stereo.wav").as_slice(),
        include_bytes!("../fixtures/ima_adpcm_16k_mono.wav").as_slice(),
        include_bytes!("../fixtures/ima_adpcm_16k_stereo.wav").as_slice(),
    ] {
        let decoded = ryf::decode_bytes(wav, DecodeOptions::default())?;
        for value in decoded.channels.iter().flatten() {
            hash = (hash ^ u64::from(value.to_bits())).wrapping_mul(1099511628211);
        }
    }
    let values: Vec<f32> = (i16::MIN..=i16::MAX)
        .map(|n| f32::from(n) / 32768.0)
        .collect();
    let packed = ryf::f32_to_s16le(&values);
    for (pair, expected) in packed.as_chunks::<2>().0.iter().zip(i16::MIN..=i16::MAX) {
        assert_eq!(i16::from_le_bytes([pair[0], pair[1]]), expected);
    }
    println!("{hash:016x}");
    Ok(())
}

#[cfg(target_pointer_width = "32")]
fn check_large_collect_capacity() -> Result<(), Box<dyn std::error::Error>> {
    let mut header = ryf::encode_s16(&[], 8000)?;
    header[4..8].fill(0);
    header[40..44].copy_from_slice(&(1u32 << 30).to_le_bytes());
    let mut source =
        ByteSource::from_read_seek(std::io::Cursor::new(header), Some((1u64 << 30) + 44));
    let options = DecodeOptions::default().with_max_duration_secs(1_000_000.0);
    assert!(matches!(
        ryf::decode_with(&mut source, &options),
        Err(ryf::WavError::OutputTooLarge { .. })
    ));
    Ok(())
}
