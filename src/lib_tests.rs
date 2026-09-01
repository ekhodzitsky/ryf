use super::*;

#[test]
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
}
