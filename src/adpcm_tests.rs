use super::*;
use crate::error::Result;
use crate::source::ByteSource;
use crate::{DecodeOptions, decode_bytes};

fn default_ms_coefs() -> Vec<(i16, i16)> {
    MS_DEFAULT_COEFS.to_vec()
}

#[test]
fn ms_adpcm_mono_and_errors() -> Result<()> {
    let params = MsAdpcmParams {
        block_align: 32,
        samples_per_block: 50,
        channels: 1,
        coefs: default_ms_coefs(),
    };
    // Mono header: predictor, delta, sample1, sample2 + nibble payload.
    let mut block = vec![0u8; 32];
    block[0] = 0; // predictor
    block[1] = 16;
    block[2] = 0; // delta=16
    block[3] = 0;
    block[4] = 0; // sample1
    block[5] = 0;
    block[6] = 0; // sample2
    for b in &mut block[7..] {
        *b = 0x11; // small nibbles
    }
    let mut src = ByteSource::from_slice(&block);
    let out = decode_ms_adpcm(&mut src, &params, 32, 10_000)?;
    assert!(out.len() >= 2);

    // Over-budget is TooLong (no silent truncate).
    let mut src = ByteSource::from_slice(&block);
    assert!(decode_ms_adpcm(&mut src, &params, 32, 3).is_err());

    // Empty coefs use the default table.
    let params_empty = MsAdpcmParams {
        coefs: vec![],
        ..params.clone()
    };
    let mut src = ByteSource::from_slice(&block);
    assert!(decode_ms_adpcm(&mut src, &params_empty, 32, 100).is_ok());

    // Error paths.
    assert!(
        decode_ms_adpcm(
            &mut ByteSource::from_slice(&[]),
            &MsAdpcmParams {
                channels: 0,
                ..params.clone()
            },
            0,
            10
        )
        .is_err()
    );
    assert!(
        decode_ms_adpcm(
            &mut ByteSource::from_slice(&[]),
            &MsAdpcmParams {
                channels: 3,
                ..params.clone()
            },
            0,
            10
        )
        .is_err()
    );
    assert!(
        decode_ms_adpcm(
            &mut ByteSource::from_slice(&[]),
            &MsAdpcmParams {
                block_align: 4,
                ..params.clone()
            },
            0,
            10
        )
        .is_err()
    );
    assert!(decode_ms_block_mono(&[0u8; 3], &default_ms_coefs(), false).is_err());
    let mut bad = block.clone();
    bad[0] = 99; // predictor OOR
    assert!(decode_ms_block_mono(&bad, &default_ms_coefs(), false).is_err());
    Ok(())
}

#[test]
fn ms_adpcm_stereo_block() -> Result<()> {
    let coefs = default_ms_coefs();
    let mut block = vec![0u8; 32];
    // stereo header 14 bytes
    block[0] = 0;
    block[1] = 0; // predictors
    block[2] = 16;
    block[3] = 0;
    block[4] = 16;
    block[5] = 0; // deltas
    // sample1 L/R, sample2 L/R zeros already
    for b in &mut block[14..] {
        *b = 0x00;
    }
    let out = decode_ms_block_stereo(&block, &coefs, false)?;
    assert!(out.len() >= 4);
    assert!(decode_ms_block_stereo(&[0u8; 10], &coefs, false).is_err());
    let mut bad = block.clone();
    bad[0] = 99;
    assert!(decode_ms_block_stereo(&bad, &coefs, false).is_err());

    let params = MsAdpcmParams {
        block_align: 32,
        samples_per_block: 20,
        channels: 2,
        coefs,
    };
    let mut src = ByteSource::from_slice(&block);
    let out = decode_ms_adpcm(&mut src, &params, 32, 1000)?;
    assert!(out.len() >= 4);
    assert_eq!(out.len() % 2, 0);
    Ok(())
}

#[test]
fn ima_adpcm_mono_stereo_and_errors() -> Result<()> {
    let mono = ImaAdpcmParams {
        block_align: 36,
        samples_per_block: 65,
        channels: 1,
    };
    let mut block = vec![0u8; 36];
    block[0] = 0;
    block[1] = 0; // predictor
    block[2] = 0; // step index
    for b in &mut block[4..] {
        *b = 0x77;
    }
    let mut src = ByteSource::from_slice(&block);
    let out = decode_ima_adpcm(&mut src, &mono, 36, 10_000)?;
    assert!(!out.is_empty());

    let mut src = ByteSource::from_slice(&block);
    assert!(decode_ima_adpcm(&mut src, &mono, 36, 5).is_err());

    assert!(decode_ima_block_mono(&[0u8; 2], false).is_err());
    let mut bad = block.clone();
    bad[2] = 100; // step OOR
    assert!(decode_ima_block_mono(&bad, false).is_err());

    let stereo = ImaAdpcmParams {
        block_align: 40,
        samples_per_block: 33,
        channels: 2,
    };
    // stereo header 8 bytes + groups of 8
    let mut sblock = vec![0u8; 40];
    sblock[2] = 0;
    sblock[6] = 0;
    for b in &mut sblock[8..] {
        *b = 0x12;
    }
    let out = decode_ima_block_stereo(&sblock, false)?;
    assert_eq!(out.len(), 66); // 1 header frame + 32 bytes = 32 frames, interleaved
    let leftover = decode_ima_block_stereo(&[0u8; 12], false)?;
    assert_eq!(leftover.len(), 2); // header only; 4-byte tail dropped
    assert!(decode_ima_block_stereo(&[0u8; 4], false).is_err());
    let mut bad = sblock.clone();
    bad[2] = 100;
    assert!(decode_ima_block_stereo(&bad, false).is_err());

    let mut src = ByteSource::from_slice(&sblock);
    assert!(decode_ima_adpcm(&mut src, &stereo, 40, 1000).is_ok());

    assert!(
        decode_ima_adpcm(
            &mut ByteSource::from_slice(&[]),
            &ImaAdpcmParams {
                channels: 0,
                ..mono
            },
            0,
            1
        )
        .is_err()
    );
    assert!(
        decode_ima_adpcm(
            &mut ByteSource::from_slice(&[]),
            &ImaAdpcmParams {
                block_align: 2,
                ..mono
            },
            0,
            1
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn i16_frames_to_f32_modes() {
    assert_eq!(i16_frames_to_f32(&[], 0, ChannelMode::Mono).len(), 1);
    let mono = i16_frames_to_f32(&[0, 16_384], 1, ChannelMode::Mono);
    assert_eq!(mono.len(), 1);
    assert!((mono[0][1] - 0.5).abs() < 1e-6);

    let mixed = i16_frames_to_f32(&[16_384, -16_384], 2, ChannelMode::Mono);
    assert_eq!(mixed[0].len(), 1);
    assert!(mixed[0][0].abs() < 1e-6);

    let split = i16_frames_to_f32(&[100, 200, 300, 400], 2, ChannelMode::Split);
    assert_eq!(split.len(), 2);
    assert_eq!(split[0].len(), 2);
    assert_eq!(split[1].len(), 2);
}

#[test]
fn clamp_i16_edges() {
    assert_eq!(clamp_i16(100_000), i16::MAX);
    assert_eq!(clamp_i16(-100_000), i16::MIN);
    assert_eq!(clamp_i16(12), 12);
}

#[test]
fn ms_adpcm_delta_clamps_instead_of_wrapping() {
    // nibble 0 adapt 230: 16*230>>8 = 14 -> min 16.
    assert_eq!(adapt_ms_delta(16, 0), 16);
    // nibble 6 adapt 512: 20000*512>>8 = 40000, must not become -25536.
    assert_eq!(adapt_ms_delta(20_000, 6), i16::MAX);
    assert_eq!(adapt_ms_delta(16_384, 6), i16::MAX);
}

#[test]
fn i16_at_rejects_short_block() -> Result<()> {
    assert!(i16_at(&[0u8], 0, false).is_err());
    assert_eq!(i16_at(&[0x34, 0x12], 0, false)?, 0x1234);
    assert_eq!(i16_at(&[0x12, 0x34], 0, true)?, 0x1234);
    Ok(())
}

fn put_i16(buf: &mut [u8], off: usize, v: i16, be: bool) {
    let b = if be { v.to_be_bytes() } else { v.to_le_bytes() };
    buf[off] = b[0];
    buf[off + 1] = b[1];
}

fn wrap_wave(be: bool, fmt: &[u8], payload: &[u8]) -> Vec<u8> {
    wrap_with_fact(be, fmt, payload, None)
}

fn wrap_with_fact(be: bool, fmt: &[u8], payload: &[u8], fact: Option<u32>) -> Vec<u8> {
    let u32b = |v: u32| if be { v.to_be_bytes() } else { v.to_le_bytes() };
    let fmt_len = fmt.len() as u32;
    let data_len = payload.len() as u32;
    let pad = data_len % 2;
    let fact_bytes = if fact.is_some() { 12 } else { 0 };
    let riff_len = 4 + 8 + fmt_len + fact_bytes + 8 + data_len + pad;
    let mut w = Vec::new();
    w.extend_from_slice(if be { b"RIFX" } else { b"RIFF" });
    w.extend_from_slice(&u32b(riff_len));
    w.extend_from_slice(b"WAVE");
    w.extend_from_slice(b"fmt ");
    w.extend_from_slice(&u32b(fmt_len));
    w.extend_from_slice(fmt);
    if let Some(n) = fact {
        w.extend_from_slice(b"fact");
        w.extend_from_slice(&u32b(4));
        w.extend_from_slice(&u32b(n));
    }
    w.extend_from_slice(b"data");
    w.extend_from_slice(&u32b(data_len));
    w.extend_from_slice(payload);
    if pad == 1 {
        w.push(0);
    }
    w
}

fn ima_fmt(be: bool) -> Vec<u8> {
    let u16b = |v: u16| if be { v.to_be_bytes() } else { v.to_le_bytes() };
    let u32b = |v: u32| if be { v.to_be_bytes() } else { v.to_le_bytes() };
    let mut f = Vec::new();
    f.extend_from_slice(&u16b(0x0011));
    f.extend_from_slice(&u16b(1));
    f.extend_from_slice(&u32b(16_000));
    f.extend_from_slice(&u32b(16_000 * 36 / 65));
    f.extend_from_slice(&u16b(36));
    f.extend_from_slice(&u16b(4));
    f.extend_from_slice(&u16b(2));
    f.extend_from_slice(&u16b(65));
    f
}

fn ms_fmt(be: bool) -> Vec<u8> {
    let u16b = |v: u16| if be { v.to_be_bytes() } else { v.to_le_bytes() };
    let u32b = |v: u32| if be { v.to_be_bytes() } else { v.to_le_bytes() };
    let mut f = Vec::new();
    f.extend_from_slice(&u16b(0x0002));
    f.extend_from_slice(&u16b(1));
    f.extend_from_slice(&u32b(16_000));
    f.extend_from_slice(&u32b(8_000));
    f.extend_from_slice(&u16b(32));
    f.extend_from_slice(&u16b(4));
    f.extend_from_slice(&u16b(32));
    f.extend_from_slice(&u16b(52));
    f.extend_from_slice(&u16b(7));
    for (a, b) in MS_DEFAULT_COEFS {
        f.extend_from_slice(&if be { a.to_be_bytes() } else { a.to_le_bytes() });
        f.extend_from_slice(&if be { b.to_be_bytes() } else { b.to_le_bytes() });
    }
    f
}

#[test]
fn rifx_adpcm_headers_match_le() -> Result<()> {
    let mut ima_le = vec![0u8; 36];
    put_i16(&mut ima_le, 0, 0x1234, false);
    ima_le[2] = 4;
    for b in &mut ima_le[4..] {
        *b = 0x77;
    }
    let mut ima_be = ima_le.clone();
    put_i16(&mut ima_be, 0, 0x1234, true);
    ima_be[2] = 4;
    assert_eq!(
        decode_ima_block_mono(&ima_le, false)?,
        decode_ima_block_mono(&ima_be, true)?
    );

    let coefs = default_ms_coefs();
    let mut ms_le = vec![0u8; 32];
    ms_le[0] = 0;
    put_i16(&mut ms_le, 1, 0x1234, false);
    put_i16(&mut ms_le, 3, 0x0100, false);
    put_i16(&mut ms_le, 5, -32, false);
    for b in &mut ms_le[7..] {
        *b = 0x11;
    }
    let mut ms_be = ms_le.clone();
    put_i16(&mut ms_be, 1, 0x1234, true);
    put_i16(&mut ms_be, 3, 0x0100, true);
    put_i16(&mut ms_be, 5, -32, true);
    assert_eq!(
        decode_ms_block_mono(&ms_le, &coefs, false)?,
        decode_ms_block_mono(&ms_be, &coefs, true)?
    );

    let le_wav = wrap_wave(false, &ima_fmt(false), &ima_le);
    let be_wav = wrap_wave(true, &ima_fmt(true), &ima_be);
    let dle = decode_bytes(&le_wav, DecodeOptions::unbounded())?;
    let dbe = decode_bytes(&be_wav, DecodeOptions::unbounded())?;
    assert_eq!(dle.channels[0], dbe.channels[0]);

    let le_wav = wrap_wave(false, &ms_fmt(false), &ms_le);
    let be_wav = wrap_wave(true, &ms_fmt(true), &ms_be);
    let dle = decode_bytes(&le_wav, DecodeOptions::unbounded())?;
    let dbe = decode_bytes(&be_wav, DecodeOptions::unbounded())?;
    assert_eq!(dle.channels[0], dbe.channels[0]);
    Ok(())
}

#[test]
fn ima_zero_samples_per_block_still_decodes() -> Result<()> {
    let mut block = vec![0u8; 36];
    put_i16(&mut block, 0, 0x0100, false);
    block[2] = 4;
    for b in &mut block[4..] {
        *b = 0x11;
    }
    let mut fmt = ima_fmt(false);
    fmt[18..20].copy_from_slice(&0u16.to_le_bytes());
    let wav = wrap_wave(false, &fmt, &block);
    let d = decode_bytes(&wav, DecodeOptions::unbounded())?;
    assert!(d.frames() > 1);
    Ok(())
}

#[test]
fn fact_smaller_than_block_truncates() -> Result<()> {
    let mut block = vec![0u8; 36];
    put_i16(&mut block, 0, 0x0100, false);
    block[2] = 4;
    for b in &mut block[4..] {
        *b = 0x11;
    }
    let wav = wrap_with_fact(false, &ima_fmt(false), &block, Some(4));
    let d = decode_bytes(&wav, DecodeOptions::unbounded())?;
    assert_eq!(d.frames(), 4);
    let p = crate::probe(&mut ByteSource::from_slice(&wav))?;
    assert_eq!(p.declared_frames, Some(4));
    Ok(())
}
