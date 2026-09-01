use super::*;

#[test]
#[allow(deprecated)]
fn channel_mode_default_and_max_decode_samples() {
    assert_eq!(ChannelMode::default(), ChannelMode::Mono);
    assert_eq!(
        max_decode_samples(16_000),
        DecodeOptions::default().max_frames(16_000)
    );
    assert_eq!(
        DecodeOptions::speech().max_duration_secs,
        DEFAULT_MAX_DURATION_SECS
    );
    assert_eq!(MAX_DURATION_S, DEFAULT_MAX_DURATION_SECS);
    assert_eq!(MAX_SAMPLE_RATE, DEFAULT_MAX_SAMPLE_RATE);
    assert_eq!(MAX_DECODE_SAMPLE_RATE, DEFAULT_MAX_DECODE_SAMPLE_RATE);
}

#[test]
fn read_and_decoded_helpers() -> crate::Result<()> {
    let pcm = crate::f32_to_s16le(&[0.1, -0.2, 0.0]);
    let wav = crate::encode_s16(&pcm, 16_000)?;
    let tmp = tempfile::NamedTempFile::with_suffix(".wav")?;
    crate::write_s16(tmp.path(), &pcm, 16_000)?;
    let decoded = crate::read(tmp.path())?;
    assert_eq!(decoded.sample_rate, 16_000);
    assert_eq!(decoded.num_channels(), 1);
    assert_eq!(decoded.frames(), 3);
    let split = crate::read_with(
        tmp.path(),
        &DecodeOptions::speech().with_channel_mode(ChannelMode::Split),
    )?;
    assert_eq!(split.num_channels(), 1);
    let from_bytes = crate::decode_bytes(&wav, DecodeOptions::speech())?;
    assert_eq!(from_bytes.frames(), decoded.frames());
    Ok(())
}
