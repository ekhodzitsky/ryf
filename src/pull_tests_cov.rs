use super::super::stream;
use super::super::*;
use super::riff_pcm;
use crate::ChannelMode;
use crate::error::Result;
use crate::header::SampleCodec;
use crate::options::DecodeOptions;
use crate::source::ByteSource;

fn cursor(bytes: Vec<u8>) -> ByteSource<'static> {
    use std::io::Cursor;
    let n = bytes.len() as u64;
    ByteSource::from_read_seek(Cursor::new(bytes), Some(n))
}

fn pcm_fields(codec: SampleCodec, ch: usize, width: usize) -> crate::header::FmtFields {
    crate::header::FmtFields {
        codec,
        channels: ch,
        sample_rate: 16_000,
        sample_width: width,
        adpcm_ms: None,
        adpcm_ima: None,
        big_endian: false,
    }
}

fn plan(
    codec: SampleCodec,
    ch: usize,
    width: usize,
    frames: usize,
    max: usize,
    mode: ChannelMode,
) -> DecodePlan {
    DecodePlan {
        sample_rate: 16_000,
        channels: ch,
        sample_width: width,
        frame_bytes: width * ch,
        total_frames: frames,
        max_samples: max,
        codec,
        big_endian: false,
        mode,
        data_len: (frames * width * ch) as u64,
        fmt: pcm_fields(codec, ch, width),
    }
}

fn on_ok(_: StreamBlock<'_>) -> Result<()> {
    Ok(())
}

#[test]
fn pull_stream_cursor_short_and_duration() -> Result<()> {
    let s16: Vec<u8> = (0..64i16).flat_map(|i| i.to_le_bytes()).collect();
    let n = stream::pull_mono_s16(&mut cursor(s16.clone()), 64, 10_000, 16_000, &mut on_ok)?;
    assert_eq!(n, 64);
    assert!(stream::pull_mono_s16(&mut cursor(s16.clone()), 64, 10, 16_000, &mut on_ok).is_err());
    assert!(
        stream::pull_mono_s16(
            &mut ByteSource::from_slice(&[0u8; 4]),
            64,
            10_000,
            16_000,
            &mut on_ok
        )
        .is_err()
    );
    assert!(
        stream::pull_mono_s16(&mut cursor(vec![0u8; 4]), 64, 10_000, 16_000, &mut on_ok).is_err()
    );

    let mut stereo = Vec::new();
    for i in 0..16i16 {
        stereo.extend_from_slice(&i.to_le_bytes());
        stereo.extend_from_slice(&(-i).to_le_bytes());
    }
    assert_eq!(
        stream::pull_mix_s16(
            &mut cursor(stereo.clone()),
            16,
            4,
            2,
            10_000,
            16_000,
            &mut on_ok
        )?,
        16
    );
    assert!(
        stream::pull_mix_s16(&mut cursor(stereo.clone()), 16, 4, 2, 4, 16_000, &mut on_ok).is_err()
    );
    assert!(
        stream::pull_mix_s16(
            &mut ByteSource::from_slice(&[0u8; 2]),
            16,
            4,
            2,
            10_000,
            16_000,
            &mut on_ok
        )
        .is_err()
    );

    let f32p: Vec<u8> = [0.0f32, 0.5, -0.25, 1.0, -1.0, 0.125]
        .into_iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    assert_eq!(
        stream::pull_mono_f32(&mut cursor(f32p.clone()), 6, 10_000, 16_000, &mut on_ok)?,
        6
    );
    assert!(stream::pull_mono_f32(&mut cursor(f32p.clone()), 6, 2, 16_000, &mut on_ok).is_err());
    assert!(
        stream::pull_mono_f32(
            &mut ByteSource::from_slice(&[0u8; 4]),
            6,
            10_000,
            16_000,
            &mut on_ok
        )
        .is_err()
    );

    let mut nine = Vec::new();
    for _ in 0..3 {
        for c in 0..9i16 {
            nine.extend_from_slice(&c.to_le_bytes());
        }
    }
    assert_eq!(
        stream::pull_split_s16(
            &mut cursor(nine.clone()),
            3,
            18,
            9,
            10_000,
            16_000,
            &mut on_ok
        )?,
        3
    );
    assert!(
        stream::pull_split_s16(&mut cursor(nine.clone()), 3, 18, 9, 1, 16_000, &mut on_ok).is_err()
    );
    assert!(
        stream::pull_split_s16(
            &mut ByteSource::from_slice(&[0u8; 2]),
            3,
            18,
            9,
            10_000,
            16_000,
            &mut on_ok
        )
        .is_err()
    );

    let u8p: Vec<u8> = (0..27).collect();
    assert_eq!(
        stream::pull_generic(
            &mut cursor(u8p.clone()),
            ChannelMode::Split,
            SampleCodec::U8,
            9,
            1,
            9,
            3,
            10_000,
            8_000,
            false,
            &mut on_ok
        )?,
        3
    );
    assert!(
        stream::pull_generic(
            &mut cursor(u8p),
            ChannelMode::Split,
            SampleCodec::U8,
            9,
            1,
            9,
            3,
            1,
            8_000,
            false,
            &mut on_ok
        )
        .is_err()
    );

    let stereo_s16: Vec<u8> = (0..8i16).flat_map(|i| i.to_le_bytes()).collect();
    assert_eq!(
        stream::pull_split_s16(&mut cursor(stereo_s16), 4, 4, 2, 10_000, 16_000, &mut on_ok)?,
        4
    );
    Ok(())
}

#[test]
fn collect_cursor_short_and_duration() -> Result<()> {
    let s16: Vec<u8> = (0..32i16).flat_map(|i| i.to_le_bytes()).collect();
    let out = decode_collect(
        &mut cursor(s16.clone()),
        &plan(SampleCodec::S16, 1, 2, 32, 10_000, ChannelMode::Mono),
    )?;
    assert_eq!(out[0].len(), 32);
    assert!(
        decode_collect(
            &mut cursor(s16),
            &plan(SampleCodec::S16, 1, 2, 32, 8, ChannelMode::Mono)
        )
        .is_err()
    );
    assert!(
        decode_collect(
            &mut ByteSource::from_slice(&[0u8; 4]),
            &plan(SampleCodec::S16, 1, 2, 32, 10_000, ChannelMode::Mono)
        )
        .is_err()
    );

    let mut stereo = Vec::new();
    for i in 0..8i16 {
        stereo.extend_from_slice(&i.to_le_bytes());
        stereo.extend_from_slice(&(-i).to_le_bytes());
    }
    assert_eq!(
        decode_collect(
            &mut cursor(stereo.clone()),
            &plan(SampleCodec::S16, 2, 2, 8, 10_000, ChannelMode::Mono)
        )?[0]
            .len(),
        8
    );
    assert!(
        decode_collect(
            &mut cursor(stereo.clone()),
            &plan(SampleCodec::S16, 2, 2, 8, 3, ChannelMode::Mono)
        )
        .is_err()
    );
    assert!(
        decode_collect(
            &mut ByteSource::from_slice(&[0u8; 2]),
            &plan(SampleCodec::S16, 2, 2, 8, 10_000, ChannelMode::Mono)
        )
        .is_err()
    );
    assert_eq!(
        decode_collect(
            &mut cursor(stereo.clone()),
            &plan(SampleCodec::S16, 2, 2, 8, 10_000, ChannelMode::Split)
        )?
        .len(),
        2
    );
    assert!(
        decode_collect(
            &mut cursor(stereo),
            &plan(SampleCodec::S16, 2, 2, 8, 3, ChannelMode::Split)
        )
        .is_err()
    );
    assert!(
        decode_collect(
            &mut ByteSource::from_slice(&[0u8; 2]),
            &plan(SampleCodec::S16, 2, 2, 8, 10_000, ChannelMode::Split)
        )
        .is_err()
    );

    let f32p: Vec<u8> = [0.0f32, 1.0, -0.5, 0.25]
        .into_iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    assert_eq!(
        decode_collect(
            &mut cursor(f32p.clone()),
            &plan(SampleCodec::F32, 1, 4, 4, 10_000, ChannelMode::Mono)
        )?[0]
            .len(),
        4
    );
    assert!(
        decode_collect(
            &mut cursor(f32p),
            &plan(SampleCodec::F32, 1, 4, 4, 2, ChannelMode::Mono)
        )
        .is_err()
    );
    assert!(
        decode_collect(
            &mut ByteSource::from_slice(&[0u8; 4]),
            &plan(SampleCodec::F32, 1, 4, 4, 10_000, ChannelMode::Mono)
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn adpcm_helpers_missing_params_and_wrong_codec() {
    let missing_ms = pcm_fields(SampleCodec::MsAdpcm, 1, 1);
    assert!(
        decode_adpcm_interleaved(&mut ByteSource::from_slice(&[]), &missing_ms, 0, 10).is_err()
    );
    let missing_ima = pcm_fields(SampleCodec::ImaAdpcm, 1, 1);
    assert!(
        decode_adpcm_interleaved(&mut ByteSource::from_slice(&[]), &missing_ima, 0, 10).is_err()
    );
    let p = plan(SampleCodec::S16, 1, 2, 0, 10, ChannelMode::Mono);
    assert!(matches!(
        super::super::adpcm::collect_adpcm(&mut ByteSource::from_slice(&[]), &p),
        Err(WavError::UnsupportedCodec)
    ));
}

#[test]
fn cursor_public_decode_nine_ch_and_f32() -> Result<()> {
    let s16: Vec<u8> = (0..32i16).flat_map(|i| i.to_le_bytes()).collect();
    let wav = riff_pcm(1, 16_000, 1, 16, &s16);
    let d = crate::decode_with(&mut cursor(wav.clone()), &DecodeOptions::default())?;
    assert_eq!(d.channels[0].len(), 32);
    let mut streamed = 0usize;
    decode_streaming(&mut cursor(wav), &DecodeOptions::default(), |b| {
        streamed += b.frames;
        Ok(())
    })?;
    assert_eq!(streamed, 32);

    let f32p: Vec<u8> = [0.0f32, 0.5, -0.25]
        .into_iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    let wav = riff_pcm(3, 16_000, 1, 32, &f32p);
    assert_eq!(
        crate::decode_with(&mut cursor(wav.clone()), &DecodeOptions::default())?.channels[0].len(),
        3
    );
    decode_streaming(&mut cursor(wav), &DecodeOptions::default(), |_| Ok(()))?;

    let mut payload = Vec::new();
    for _ in 0..2 {
        for c in 0..9i16 {
            payload.extend_from_slice(&c.to_le_bytes());
        }
    }
    let wav = riff_pcm(1, 16_000, 9, 16, &payload);
    let opts = DecodeOptions::default().with_channel_mode(ChannelMode::Split);
    let d = crate::decode_with(&mut cursor(wav.clone()), &opts)?;
    assert_eq!(d.channels.len(), 9);
    let info = decode_streaming(&mut cursor(wav), &opts, |b| {
        assert_eq!(b.planar.len(), 9);
        Ok(())
    })?;
    assert_eq!(info.channels, 9);
    Ok(())
}
