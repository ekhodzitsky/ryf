use super::*;
use crate::error::Result;
use crate::source::ByteSource;

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

    // Over-budget -> TooLong (no silent truncate).
    let mut src = ByteSource::from_slice(&block);
    assert!(decode_ms_adpcm(&mut src, &params, 32, 3).is_err());

    // Empty coefs -> default table.
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
    assert!(decode_ms_block_mono(&[0u8; 3], &default_ms_coefs()).is_err());
    let mut bad = block.clone();
    bad[0] = 99; // predictor OOR
    assert!(decode_ms_block_mono(&bad, &default_ms_coefs()).is_err());
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
    let out = decode_ms_block_stereo(&block, &coefs)?;
    assert!(out.len() >= 4);
    assert!(decode_ms_block_stereo(&[0u8; 10], &coefs).is_err());
    let mut bad = block.clone();
    bad[0] = 99;
    assert!(decode_ms_block_stereo(&bad, &coefs).is_err());

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

    assert!(decode_ima_block_mono(&[0u8; 2]).is_err());
    let mut bad = block.clone();
    bad[2] = 100; // step OOR
    assert!(decode_ima_block_mono(&bad).is_err());

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
    let out = decode_ima_block_stereo(&sblock)?;
    assert!(out.len() >= 2);
    assert!(decode_ima_block_stereo(&[0u8; 4]).is_err());
    let mut bad = sblock.clone();
    bad[2] = 100;
    assert!(decode_ima_block_stereo(&bad).is_err());

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
