use super::header::{push_extensible_header, push_rf64_extensible_header};
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
    assert_eq!(&alaw[16..20], &18u32.to_le_bytes());
    assert!(alaw.windows(4).any(|w| w == b"fact"));
    let packed = crate::convert::g711::s16le_to_g711(&f32_to_s16le(&[0.25, -0.5]), true)?;
    let rwav = encode_rifx(WriteSpec::alaw(8_000, 1), &packed)?;
    assert_eq!(&rwav[..4], b"RIFX");
    let d = decode_bytes(&rwav, DecodeOptions::unbounded())?;
    let dle = decode_bytes(&alaw, DecodeOptions::unbounded())?;
    assert_eq!(d.channels[0], dle.channels[0]);
    Ok(())
}

#[test]
fn encode_extensible_g711_roundtrips_and_overflow() -> Result<()> {
    let wav = encode_extensible(WriteSpec::alaw(8_000, 1), &[0xd5, 0x55])?;
    assert_eq!(&wav[20..22], &0xFFFEu16.to_le_bytes());
    assert!(wav.windows(4).any(|w| w == b"fact"));
    let d = decode_bytes(&wav, DecodeOptions::unbounded())?;
    assert_eq!(d.frames(), 2);
    let mu = encode_extensible(WriteSpec::mulaw(8_000, 1), &[0xff, 0x7f])?;
    assert_eq!(decode_bytes(&mu, DecodeOptions::unbounded())?.frames(), 2);
    let mut hdr = Vec::new();
    assert!(matches!(
        push_extensible_header(&mut hdr, WriteSpec::s16(16_000, 1), u32::MAX - 50),
        Err(WavError::RiffTooLarge)
    ));
    let mut rf = Vec::new();
    push_rf64_extensible_header(&mut rf, WriteSpec::s16(16_000, 1), 1u64 << 40, 1u64 << 39)?;
    assert_eq!(&rf[..4], b"RF64");
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

#[test]
fn wav_writer_alaw_matches_encode() -> Result<()> {
    use std::io::Cursor;

    let packed = crate::convert::g711::s16le_to_g711(&f32_to_s16le(&[0.25, -0.5]), true)?;
    let spec = WriteSpec::alaw(8_000, 1);
    let mut cur = Cursor::new(Vec::new());
    {
        let mut w = WavWriter::new(&mut cur, spec)?;
        w.write_pcm(&packed)?;
        w.finalize()?;
    }
    assert_eq!(cur.into_inner(), crate::encode(spec, &packed)?);
    Ok(())
}

#[test]
fn write_f32_samples_rejects_odd_channels() -> Result<()> {
    use std::io::Cursor;
    let mut cur = Cursor::new(Vec::new());
    let mut w = WavWriter::new(&mut cur, WriteSpec::f32(16_000, 2))?;
    assert!(matches!(w.write_f32_samples(&[0.1]), Err(WavError::OddPcm)));
    w.write_f32_samples(&[0.1, -0.2])?;
    w.finalize()?;
    Ok(())
}

#[test]
fn encode_odd_u8_writes_riff_pad() -> Result<()> {
    let pcm = [0u8, 128, 255];
    let wav = crate::encode(WriteSpec::u8(8_000, 1), &pcm)?;
    assert_eq!(wav.len() % 2, 0);
    assert_eq!(wav[wav.len() - 1], 0);
    assert_eq!(&wav[40..44], &3u32.to_le_bytes());
    let (_, mono) = crate::decode_f32(&wav)?;
    assert_eq!(mono.len(), 3);

    let mut cur = std::io::Cursor::new(Vec::new());
    {
        let mut w = WavWriter::new(&mut cur, WriteSpec::u8(8_000, 1))?;
        w.write_pcm(&pcm)?;
        w.finalize()?;
    }
    let streamed = cur.into_inner();
    assert_eq!(streamed, wav);
    Ok(())
}

#[test]
fn sniff_rewinds_after_io_error() -> Result<()> {
    use std::io::{Read, Seek, SeekFrom};

    struct PartialBoom {
        remain: u8,
        pos: u64,
    }
    impl Read for PartialBoom {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.remain == 0 {
                return Err(std::io::Error::other("boom"));
            }
            self.remain -= 1;
            if buf.is_empty() {
                return Ok(0);
            }
            buf[0] = b'R';
            self.pos += 1;
            Ok(1)
        }
    }
    impl Seek for PartialBoom {
        fn seek(&mut self, from: SeekFrom) -> std::io::Result<u64> {
            self.pos = match from {
                SeekFrom::Start(p) => p,
                SeekFrom::Current(d) => {
                    let s = i64::try_from(self.pos)
                        .unwrap_or(i64::MAX)
                        .saturating_add(d);
                    if s < 0 {
                        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "neg"));
                    }
                    s as u64
                }
                SeekFrom::End(_) => 0,
            };
            Ok(self.pos)
        }
    }
    let mut src = crate::ByteSource::from_read_seek(PartialBoom { remain: 1, pos: 0 }, Some(40));
    assert!(matches!(
        crate::sniff_is_riff_wave(&mut src),
        Err(WavError::Io(_))
    ));
    assert_eq!(src.pos(), 0);
    Ok(())
}
