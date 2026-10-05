use crate::error::{FormatKind, WavError};
use crate::g726::{G726Packing, Tag0064};
use crate::header::WAVE_FORMAT_G722_ASTERISK;
use crate::{DecodeOptions, decode_bytes};

fn put_u16(out: &mut Vec<u8>, n: u16) {
    out.extend_from_slice(&n.to_le_bytes());
}

fn put_u32(out: &mut Vec<u8>, n: u32) {
    out.extend_from_slice(&n.to_le_bytes());
}

/// Mono or stereo RIFF, tag `0x0064`, 8000 Hz, 18-byte `fmt `.
fn wave_0064(payload: &[u8], channels: u16, byte_rate: u32, block: u16, bits: u16) -> Vec<u8> {
    let mut fmt = Vec::new();
    put_u16(&mut fmt, WAVE_FORMAT_G722_ASTERISK);
    put_u16(&mut fmt, channels);
    put_u32(&mut fmt, 8_000);
    put_u32(&mut fmt, byte_rate);
    put_u16(&mut fmt, block);
    put_u16(&mut fmt, bits);
    put_u16(&mut fmt, 0);
    let mut body = Vec::new();
    body.extend_from_slice(b"fmt ");
    put_u32(&mut body, 18);
    body.extend_from_slice(&fmt);
    body.extend_from_slice(b"data");
    put_u32(&mut body, payload.len() as u32);
    body.extend_from_slice(payload);
    let mut file = Vec::new();
    file.extend_from_slice(b"RIFF");
    put_u32(&mut file, 4 + body.len() as u32);
    file.extend_from_slice(b"WAVE");
    file.extend_from_slice(&body);
    file
}

fn forced_g726() -> DecodeOptions {
    DecodeOptions::unbounded().with_tag_0064(Tag0064::G726 {
        packing: G726Packing::Aal2,
    })
}

fn assert_format(result: crate::Result<crate::DecodedWav>, kind: FormatKind) -> crate::Result<()> {
    match result {
        Err(WavError::Format(got)) if got == kind => Ok(()),
        other => panic!("expected Format({kind:?}), observed {other:?}"),
    }
}

/// `wBitsPerSample` is outside 2..=5. Byte rate 3000 is 3 bits at 8000 Hz.
fn kbps24(block: u16, bits: u16) -> (Vec<u8>, usize) {
    let payload = [0x11u8, 0x22, 0x33];
    let frames = payload.len() * 8 / 3;
    (wave_0064(&payload, 1, 3_000, block, bits), frames)
}

#[test]
fn tag_0064_byte_rate_decodes_as_g726() -> crate::Result<()> {
    let (file, frames) = kbps24(3, 0);
    let decoded = decode_bytes(&file, forced_g726())?;
    assert_eq!(decoded.sample_rate, 8_000);
    assert_eq!(decoded.frames(), frames);
    assert!(decoded.frames() > 0);
    Ok(())
}

#[test]
fn tag_0064_auto_is_ambiguous() -> crate::Result<()> {
    let (file, _) = kbps24(3, 0);
    assert_format(
        decode_bytes(&file, DecodeOptions::unbounded()),
        FormatKind::AmbiguousCodec,
    )?;
    Ok(())
}

#[test]
fn tag_0064_forced_g726_rejects_stereo() -> crate::Result<()> {
    let file = wave_0064(&[0x11, 0x22], 2, 8_000, 1, 4);
    assert_format(
        decode_bytes(&file, forced_g726()),
        FormatKind::ChannelLayout,
    )?;
    Ok(())
}

#[test]
fn tag_0064_forced_g726_rejects_short_block() -> crate::Result<()> {
    let (from_rate, _) = kbps24(1, 0);
    assert_format(
        decode_bytes(&from_rate, forced_g726()),
        FormatKind::InvalidSize,
    )?;
    let (from_bits, _) = kbps24(1, 3);
    assert_format(
        decode_bytes(&from_bits, forced_g726()),
        FormatKind::InvalidSize,
    )?;
    Ok(())
}
