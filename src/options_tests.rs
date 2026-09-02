use super::*;
use crate::ChannelMode;

#[test]
fn builders_and_max_frames() {
    let d = DecodeOptions::speech();
    assert_eq!(d.channel_mode, ChannelMode::Mono);
    assert_eq!(d.source_label_str(), "wav");

    let u = DecodeOptions::unbounded()
        .with_channel_mode(ChannelMode::Split)
        .with_max_duration_secs(1.5)
        .with_max_sample_rate(8_000)
        .with_max_decode_sample_rate(8_000)
        .with_source_label("upload-1");
    assert_eq!(u.channel_mode, ChannelMode::Split);
    assert_eq!(u.max_sample_rate, 8_000);
    assert_eq!(u.source_label_str(), "upload-1");
    assert_eq!(
        DecodeOptions::speech().max_output_bytes,
        DEFAULT_MAX_OUTPUT_BYTES
    );
    assert!(DecodeOptions::unbounded().max_output_bytes > DEFAULT_MAX_OUTPUT_BYTES);
    assert_eq!(
        DecodeOptions::default()
            .with_max_output_bytes(99)
            .max_output_bytes,
        99
    );
    assert_eq!(u.max_frames(8_000), 12_000);
    // Rate is clamped by max_decode_sample_rate.
    assert_eq!(u.max_frames(48_000), 12_000);
    // Non-finite / non-positive → 0.
    assert_eq!(
        DecodeOptions::default()
            .with_max_duration_secs(f64::NAN)
            .max_frames(16_000),
        0
    );
    assert_eq!(
        DecodeOptions::default()
            .with_max_duration_secs(0.0)
            .max_frames(16_000),
        0
    );
    assert_eq!(
        DecodeOptions::default()
            .with_max_duration_secs(-1.0)
            .max_frames(16_000),
        0
    );
    // sample_rate 0 still uses max(1).
    assert!(DecodeOptions::default().max_frames(0) > 0);
}
