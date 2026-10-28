use super::tests::*;
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// Random well-formed WAVs: own output is always bit-exact with
    /// the ffmpeg oracle, and own never rejects a well-formed input.
    #[test]
    fn proptest_diff_own_vs_ffmpeg(
        rate_idx in 0..8usize,
        ch in 1..=8u16,
        codec_idx in 0..8usize,
        ext in any::<bool>(),
        frames in 0..300usize,
        seed in any::<u64>(),
    ) {
        let rates = [8000u32, 11025, 16000, 22050, 44100, 48000, 96000, 192000];
        let codec = TestCodec::ALL[codec_idx];
        let mut rng = XorShift64::new(seed);
        let payload = gen_payload(codec, &mut rng, frames, usize::from(ch));
        let wav = WavBuilder {
            sample_rate: rates[rate_idx],
            channels: ch,
            codec,
            extensible: ext,
            payload,
            ..WavBuilder::new(codec)
        }
        .build();
        assert_native_bit_exact_both_modes("proptest diff", &wav);
    }

    /// RIFF/WAVE-prefixed garbage must never panic the in-tree decoder,
    /// and decoding is deterministic.
    #[test]
    fn proptest_wav_garbage_never_panics(
        body in proptest::collection::vec(any::<u8>(), 0..2048),
    ) {
        let mut data = Vec::with_capacity(body.len() + 12);
        data.extend_from_slice(b"RIFF");
        let riff_len = 4u32.wrapping_add(body.len() as u32);
        data.extend_from_slice(&riff_len.to_le_bytes());
        data.extend_from_slice(b"WAVE");
        data.extend_from_slice(&body);

        let a = own_mono(&data);
        let b = own_mono(&data);
        match (a, b) {
            (Ok(x), Ok(y)) => assert_bit_exact("proptest garbage determinism", &x, &y),
            (Err(_), Err(_)) => {}
            _ => panic!("decode is non-deterministic on garbage input"),
        }
    }
}
