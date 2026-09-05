use super::*;
use crate::ChannelMode;
use crate::error::Result;
use crate::header::SampleCodec;
use crate::options::DecodeOptions;
use crate::source::ByteSource;

#[test]
fn block_and_duration_helpers() {
    assert_eq!(block_frames_for(2), (1 << 18) / 2);
    // frame_bytes 0 is clamped to 1: full 256 KiB block.
    assert_eq!(block_frames_for(0), 1 << 18);
    assert_eq!(scratch_frames(2, 10), 10);
    assert_eq!(scratch_frames(2, 0), 1);
    assert!(check_duration(10, 20, 16_000).is_ok());
    assert!(check_duration(30, 20, 16_000).is_err());
    assert!(!need_duration_check(10, 20));
    assert!(need_duration_check(30, 20));
    // IMA mono ba=36: 1 + 2*(36-4) = 65. MS mono ba=32: 2 + 2*(32-7) = 52.
    assert_eq!(adpcm_est_frames(36, 36, 1, true), 65);
    assert_eq!(adpcm_est_frames(72, 36, 1, true), 130);
    assert_eq!(adpcm_est_frames(32, 32, 1, false), 52);
    assert_eq!(adpcm_est_frames(32, 0, 1, true), 0);
    // IMA stereo: 8-byte header + 8-byte L/R groups. ba=12 leftover 4 dropped.
    assert_eq!(adpcm_est_frames(8, 8, 2, true), 1);
    assert_eq!(adpcm_est_frames(12, 12, 2, true), 1);
    assert_eq!(adpcm_est_frames(16, 16, 2, true), 9);
    assert_eq!(adpcm_est_frames(40, 40, 2, true), 33);
    assert_eq!(adpcm_frames_capped(32, 32, 1, false, Some(10)), 10);
    assert_eq!(adpcm_frames_capped(32, 32, 1, false, Some(1_000)), 52);
    assert_eq!(adpcm_frames_capped(32, 32, 1, false, Some(0)), 52);
}

#[test]
fn ensure_adpcm_enabled_default_features() {
    #[cfg(feature = "adpcm")]
    assert!(ensure_adpcm_enabled().is_ok());
    #[cfg(not(feature = "adpcm"))]
    assert!(matches!(
        ensure_adpcm_enabled(),
        Err(WavError::FeatureDisabled { feature: "adpcm" })
    ));
}

#[test]
fn decode_adpcm_wrong_codec_errors() {
    let fmt = crate::header::FmtFields {
        codec: SampleCodec::S16,
        channels: 1,
        sample_rate: 16_000,
        sample_width: 2,
        adpcm_ms: None,
        adpcm_ima: None,
        big_endian: false,
        format_tag: 1,
    };
    let err = decode_adpcm_interleaved(&mut ByteSource::from_slice(&[]), &fmt, 0, 10);
    #[cfg(feature = "adpcm")]
    assert!(matches!(err, Err(WavError::UnsupportedCodec { .. })));
    #[cfg(not(feature = "adpcm"))]
    assert!(matches!(
        err,
        Err(WavError::FeatureDisabled { feature: "adpcm" })
    ));
}

#[test]
fn stream_info_eq() {
    let a = StreamInfo {
        sample_rate: 16_000,
        channels: 1,
        frames: 10,
    };
    let b = a;
    assert_eq!(a, b);
}

#[test]
fn short_read_is_truncated() {
    use crate::error::FormatKind;
    assert!(matches!(
        pcm_short("wav: short s16 data"),
        WavError::Format(FormatKind::Truncated)
    ));
}

#[test]
fn slice_and_cursor_stereo_mix_and_split_bit_exact() -> Result<()> {
    use std::io::Cursor;

    let mut payload = Vec::new();
    for i in 0..257i16 {
        payload.extend_from_slice(&i.to_le_bytes());
        payload.extend_from_slice(&(-i).to_le_bytes());
    }
    let wav = riff_pcm(1, 16_000, 2, 16, &payload);

    let slice_mix = crate::decode_bytes(&wav, DecodeOptions::speech())?;
    let mut cursor =
        ByteSource::from_read_seek(Cursor::new(wav.as_slice()), Some(wav.len() as u64));
    let cursor_mix = crate::decode(&mut cursor, ChannelMode::Mono, "cursor")?;
    assert_eq!(slice_mix.channels[0].len(), cursor_mix.channels[0].len());
    for (i, (a, b)) in slice_mix.channels[0]
        .iter()
        .zip(cursor_mix.channels[0].iter())
        .enumerate()
    {
        assert_eq!(a.to_bits(), b.to_bits(), "mix i={i}");
    }

    let opts = DecodeOptions::default().with_channel_mode(ChannelMode::Split);
    let slice_split = crate::decode_bytes(&wav, opts.clone())?;
    let mut cursor =
        ByteSource::from_read_seek(Cursor::new(wav.as_slice()), Some(wav.len() as u64));
    let cursor_split = crate::decode_with(&mut cursor, &opts)?;
    assert_eq!(slice_split.channels.len(), 2);
    assert_eq!(cursor_split.channels.len(), 2);
    for c in 0..2 {
        for (i, (a, b)) in slice_split.channels[c]
            .iter()
            .zip(cursor_split.channels[c].iter())
            .enumerate()
        {
            assert_eq!(a.to_bits(), b.to_bits(), "split ch{c} i={i}");
        }
    }
    Ok(())
}

/// Exercise pull_mono_f32 / pull_generic / ADPCM collect via public streaming.
#[test]
fn streaming_f32_and_adpcm_seeds() -> Result<()> {
    // Minimal mono f32 WAVE.
    let mut payload = Vec::new();
    for v in [0.0f32, 0.5, -0.5, 1.0] {
        payload.extend_from_slice(&v.to_le_bytes());
    }
    let wav = riff_pcm(3, 16_000, 1, 32, &payload); // IEEE float, 32-bit samples
    let mut src = ByteSource::from_slice(&wav);
    let mut frames = 0usize;
    let info = decode_streaming(&mut src, &DecodeOptions::default(), |b| {
        frames += b.frames;
        assert_eq!(b.planar.len(), 1);
        Ok(())
    })?;
    assert_eq!(info.frames, 4);
    assert_eq!(frames, 4);

    #[cfg(feature = "adpcm")]
    {
        let ms = ms_adpcm_file();
        let mut src = ByteSource::from_slice(&ms);
        let info = decode_streaming(&mut src, &DecodeOptions::default(), |_| Ok(()))?;
        assert!(info.frames > 0);

        let mut src = ByteSource::from_slice(&ms);
        let plan = open_decode(&mut src, &DecodeOptions::default())?;
        let planar = decode_collect(&mut src, &plan)?;
        assert_eq!(planar.len(), 1);
        assert!(!planar[0].is_empty());
    }

    // Split mode on stereo s16.
    let stereo_payload: Vec<u8> = (0..20u16).flat_map(|i| (i as i16).to_le_bytes()).collect();
    let wav = riff_pcm(1, 16_000, 2, 16, &stereo_payload);
    let opts = DecodeOptions::default().with_channel_mode(ChannelMode::Split);
    let mut src = ByteSource::from_slice(&wav);
    let info = decode_streaming(&mut src, &opts, |b| {
        assert_eq!(b.planar.len(), 2);
        Ok(())
    })?;
    assert_eq!(info.channels, 2);
    assert_eq!(info.frames, 10);

    // Duration ceiling: actual frames above the cap must be TooLong.
    let long = riff_pcm(1, 16_000, 1, 16, &vec![0u8; 2000]);
    let opts = DecodeOptions::default().with_max_duration_secs(0.01); // 160 frames @16k
    let mut src = ByteSource::from_slice(&long);
    let err = decode_streaming(&mut src, &opts, |_| Ok(())).unwrap_err();
    assert!(matches!(err, WavError::TooLong { .. }));
    Ok(())
}

#[path = "pull_tests_cov.rs"]
mod cov;

fn riff_pcm(format_tag: u16, rate: u32, ch: u16, bits: u16, payload: &[u8]) -> Vec<u8> {
    let block_align = ch * (bits / 8);
    let byte_rate = rate * u32::from(block_align);
    let mut fmt = Vec::new();
    fmt.extend_from_slice(&format_tag.to_le_bytes());
    fmt.extend_from_slice(&ch.to_le_bytes());
    fmt.extend_from_slice(&rate.to_le_bytes());
    fmt.extend_from_slice(&byte_rate.to_le_bytes());
    fmt.extend_from_slice(&block_align.to_le_bytes());
    fmt.extend_from_slice(&bits.to_le_bytes());
    let mut body = Vec::new();
    body.extend_from_slice(b"fmt ");
    body.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
    body.extend_from_slice(&fmt);
    body.extend_from_slice(b"data");
    body.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    body.extend_from_slice(payload);
    let mut file = Vec::new();
    file.extend_from_slice(b"RIFF");
    file.extend_from_slice(&(4 + body.len() as u32).to_le_bytes());
    file.extend_from_slice(b"WAVE");
    file.extend_from_slice(&body);
    file
}

#[cfg(feature = "adpcm")]
fn ms_adpcm_file() -> Vec<u8> {
    let block_align = 32u16;
    let samples_per_block = 50u16;
    let mut fmt = Vec::new();
    fmt.extend_from_slice(&0x0002u16.to_le_bytes());
    fmt.extend_from_slice(&1u16.to_le_bytes());
    fmt.extend_from_slice(&16_000u32.to_le_bytes());
    fmt.extend_from_slice(&8000u32.to_le_bytes());
    fmt.extend_from_slice(&block_align.to_le_bytes());
    fmt.extend_from_slice(&4u16.to_le_bytes());
    fmt.extend_from_slice(&32u16.to_le_bytes()); // cbSize
    fmt.extend_from_slice(&samples_per_block.to_le_bytes());
    fmt.extend_from_slice(&7u16.to_le_bytes());
    for (a, b) in [
        (256i16, 0i16),
        (512, -256),
        (0, 0),
        (192, 64),
        (240, 0),
        (460, -208),
        (392, -232),
    ] {
        fmt.extend_from_slice(&a.to_le_bytes());
        fmt.extend_from_slice(&b.to_le_bytes());
    }
    let mut block = vec![0u8; block_align as usize];
    block[0] = 0;
    block[1] = 16;
    block[2] = 0;
    let mut body = Vec::new();
    body.extend_from_slice(b"fmt ");
    body.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
    body.extend_from_slice(&fmt);
    if fmt.len() % 2 == 1 {
        body.push(0);
    }
    body.extend_from_slice(b"data");
    body.extend_from_slice(&(block.len() as u32).to_le_bytes());
    body.extend_from_slice(&block);
    let mut file = Vec::new();
    file.extend_from_slice(b"RIFF");
    file.extend_from_slice(&(4 + body.len() as u32).to_le_bytes());
    file.extend_from_slice(b"WAVE");
    file.extend_from_slice(&body);
    file
}
