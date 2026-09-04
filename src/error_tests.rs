use super::*;
use std::error::Error;
use std::io;

#[test]
fn display_and_helpers_cover_all_variants() {
    assert_eq!(
        WavError::format(FormatKind::Truncated).to_string(),
        "truncated WAVE chunk"
    );
    assert_eq!(
        WavError::too_long(10.0, 5.0).to_string(),
        "Audio file too long (10s). Maximum supported: 5s."
    );
    assert_eq!(
        WavError::sample_rate(96_000, 48_000).to_string(),
        "Unsupported sample rate: 96000Hz (max 48000Hz)"
    );
    assert_eq!(
        WavError::output_too_large(10, 5).to_string(),
        "wav: decoded output too large (10 bytes, max 5)"
    );
    assert_eq!(WavError::NotWave.to_string(), "not a WAVE container");
    assert_eq!(
        WavError::unsupported_codec(0).to_string(),
        "unsupported audio codec"
    );
    assert_eq!(
        WavError::unsupported_codec(0x0007).to_string(),
        "unsupported audio codec (format tag 0x0007)"
    );
    assert_eq!(
        WavError::StreamLengthUnknown.to_string(),
        "wav: stream length unknown"
    );
    assert_eq!(
        WavError::FeatureDisabled { feature: "adpcm" }.to_string(),
        "wav: feature `adpcm` is not enabled in this build"
    );
    assert_eq!(
        WavError::OddPcm.to_string(),
        "PCM length is not a whole number of frames"
    );
    assert_eq!(WavError::Empty.to_string(), "WAVE data chunk is empty");
    assert_eq!(
        WavError::RiffTooLarge.to_string(),
        "WAVE payload does not fit in a RIFF u32"
    );

    let io_err: WavError = io::Error::other("disk").into();
    assert!(io_err.to_string().contains("disk"));
    assert!(io_err.source().is_some());
    assert!(WavError::NotWave.source().is_none());

    assert!(WavError::NotWave.is_format_class());
    assert!(WavError::format(FormatKind::MalformedFmt).is_format_class());
    assert!(WavError::StreamLengthUnknown.is_format_class());
    assert!(WavError::OddPcm.is_format_class());
    assert!(WavError::Empty.is_format_class());
    assert!(!WavError::RiffTooLarge.is_format_class());
    assert!(!WavError::unsupported_codec(0).is_format_class());
    assert!(!WavError::sample_rate(1, 1).is_format_class());
    assert!(!WavError::output_too_large(1, 1).is_format_class());

    assert!(matches!(
        WavError::packet_io(io::Error::new(io::ErrorKind::UnexpectedEof, "short")),
        WavError::Format(FormatKind::Truncated)
    ));
    assert!(matches!(
        WavError::packet_io(io::Error::other("disk")),
        WavError::Io(_)
    ));
}
