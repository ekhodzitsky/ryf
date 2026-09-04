use super::header::push_extensible_header;
use super::{WavWriter, WriteSpec, encode_extensible, encode_rifx};
use crate::convert::f32_to_s16le;
use crate::error::{Result, WavError};
use crate::{DecodeOptions, decode_bytes};

#[test]
fn encode_rifx_f32_s24_alaw_roundtrips() -> Result<()> {
    let mut fpcm = Vec::new();
    for s in [0.25f32, -0.5, 0.0] {
        fpcm.extend_from_slice(&s.to_le_bytes());
    }
    let fwav = encode_rifx(WriteSpec::f32(16_000, 1), &fpcm)?;
    assert_eq!(&fwav[..4], b"RIFX");
    let d = decode_bytes(&fwav, DecodeOptions::unbounded())?;
    assert_eq!(d.frames(), 3);
    for (g, e) in d.channels[0].iter().zip([0.25f32, -0.5, 0.0]) {
        assert_eq!(g.to_bits(), e.to_bits());
    }

    let s24 = [0u8, 0, 0, 0x00, 0x00, 0x80];
    let swav = encode_rifx(WriteSpec::s24(16_000, 1), &s24)?;
    let (_, mono) = crate::decode_f32(&swav)?;
    let le = crate::encode(WriteSpec::s24(16_000, 1), &s24)?;
    let (_, le_mono) = crate::decode_f32(&le)?;
    assert_eq!(mono, le_mono);

    let alaw = crate::encode_alaw(&[0.25, -0.5], 8_000, 1)?;
    let rwav = encode_rifx(WriteSpec::alaw(8_000, 1), &alaw[44..])?;
    assert_eq!(&rwav[..4], b"RIFX");
    let d = decode_bytes(&rwav, DecodeOptions::unbounded())?;
    let dle = decode_bytes(&alaw, DecodeOptions::unbounded())?;
    assert_eq!(d.channels[0], dle.channels[0]);
    Ok(())
}

#[test]
fn encode_extensible_rejects_g711_and_overflow() -> Result<()> {
    assert!(matches!(
        encode_extensible(WriteSpec::alaw(8_000, 1), &[0u8; 4]),
        Err(WavError::UnsupportedCodec { tag: 6 })
    ));
    assert!(matches!(
        encode_extensible(WriteSpec::mulaw(8_000, 1), &[0u8; 4]),
        Err(WavError::UnsupportedCodec { tag: 7 })
    ));
    let mut hdr = Vec::new();
    assert!(matches!(
        push_extensible_header(&mut hdr, WriteSpec::s16(16_000, 1), u32::MAX - 50),
        Err(WavError::RiffTooLarge)
    ));
    Ok(())
}

#[test]
fn encode_extensible_f32_roundtrips() -> Result<()> {
    let mut pcm = Vec::new();
    for s in [0.5f32, -0.25] {
        pcm.extend_from_slice(&s.to_le_bytes());
    }
    let wav = encode_extensible(WriteSpec::f32(16_000, 1), &pcm)?;
    let d = decode_bytes(&wav, DecodeOptions::unbounded())?;
    assert_eq!(d.frames(), 2);
    assert_eq!(d.channels[0][0].to_bits(), 0.5f32.to_bits());
    Ok(())
}

#[test]
fn wav_writer_rifx_and_extensible_match_encode() -> Result<()> {
    use std::io::Cursor;

    let pcm = f32_to_s16le(&[0.25, -0.5]);
    let spec = WriteSpec::s16(16_000, 1);
    let mut cur = Cursor::new(Vec::new());
    {
        let mut w = WavWriter::new_rifx(&mut cur, spec)?;
        w.write_pcm(&pcm)?;
        w.finalize()?;
    }
    assert_eq!(cur.into_inner(), encode_rifx(spec, &pcm)?);

    let mut cur = Cursor::new(Vec::new());
    {
        let mut w = WavWriter::new_extensible(&mut cur, spec)?;
        w.write_pcm(&pcm)?;
        w.finalize()?;
    }
    assert_eq!(cur.into_inner(), encode_extensible(spec, &pcm)?);
    Ok(())
}
