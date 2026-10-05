use std::io::{Cursor, Read, Seek, SeekFrom};

use crate::error::{FormatKind, WavError};
use crate::g726::{G726Packing, G726Rate, Tag0064, decode_g726};
use crate::header::{
    W64_GUID_DATA, W64_GUID_FMT, W64_GUID_RIFF, W64_GUID_WAVE, WAVE_FORMAT_G722_ASTERISK,
    WAVE_FORMAT_G726,
};
use crate::source::ByteSource;
use crate::{
    DecodeOptions, ProbeCodec, decode_bytes, decode_reader, decode_streaming, decode_with, probe,
    probe_with, read,
};

fn quantum(bits: u16) -> u16 {
    match bits {
        3 => 3,
        5 => 5,
        _ => 1,
    }
}

fn put_u16(out: &mut Vec<u8>, n: u16, be: bool) {
    out.extend_from_slice(&if be { n.to_be_bytes() } else { n.to_le_bytes() });
}

fn put_u32(out: &mut Vec<u8>, n: u32, be: bool) {
    out.extend_from_slice(&if be { n.to_be_bytes() } else { n.to_le_bytes() });
}

#[derive(Clone, Copy)]
struct WaveSpec {
    tag: u16,
    channels: u16,
    rate: u32,
    byte_rate: u32,
    block: u16,
    bits: u16,
    fact: Option<u32>,
    be: bool,
    fmt_len: u32,
    data_claim: Option<u32>,
}

fn wave(payload: &[u8], spec: &WaveSpec) -> Vec<u8> {
    let mut fmt = Vec::new();
    put_u16(&mut fmt, spec.tag, spec.be);
    put_u16(&mut fmt, spec.channels, spec.be);
    put_u32(&mut fmt, spec.rate, spec.be);
    put_u32(&mut fmt, spec.byte_rate, spec.be);
    put_u16(&mut fmt, spec.block, spec.be);
    put_u16(&mut fmt, spec.bits, spec.be);
    if spec.fmt_len >= 18 {
        put_u16(&mut fmt, 0, spec.be);
    }
    let data_claim = spec.data_claim.unwrap_or(payload.len() as u32);
    let mut body = Vec::new();
    body.extend_from_slice(b"fmt ");
    put_u32(&mut body, spec.fmt_len, spec.be);
    body.extend_from_slice(&fmt[..spec.fmt_len as usize]);
    if let Some(n) = spec.fact {
        body.extend_from_slice(b"fact");
        put_u32(&mut body, 4, spec.be);
        put_u32(&mut body, n, spec.be);
    }
    body.extend_from_slice(b"data");
    put_u32(&mut body, data_claim, spec.be);
    body.extend_from_slice(payload);
    let mut file = Vec::new();
    file.extend_from_slice(if spec.be { b"RIFX" } else { b"RIFF" });
    put_u32(&mut file, 4 + body.len() as u32, spec.be);
    file.extend_from_slice(b"WAVE");
    file.extend_from_slice(&body);
    file
}

fn fields(tag: u16, channels: u16, rate: u32, byte_rate: u32, block: u16, bits: u16) -> WaveSpec {
    WaveSpec {
        tag,
        channels,
        rate,
        byte_rate,
        block,
        bits,
        fact: None,
        be: false,
        fmt_len: 18,
        data_claim: None,
    }
}

fn g726(payload: &[u8], bits: u16) -> Vec<u8> {
    wave(
        payload,
        &fields(
            WAVE_FORMAT_G726,
            1,
            8_000,
            u32::from(bits) * 1_000,
            quantum(bits),
            bits,
        ),
    )
}

fn payload(bits: u8) -> &'static [u8] {
    match bits {
        2 => include_bytes!("../fixtures/g726/aal2-2.bin"),
        3 => include_bytes!("../fixtures/g726/aal2-3.bin"),
        4 => include_bytes!("../fixtures/g726/aal2-4.bin"),
        _ => include_bytes!("../fixtures/g726/aal2-5.bin"),
    }
}

fn headerless(bits: u8) -> crate::Result<crate::DecodedWav> {
    let Some(rate) = G726Rate::from_bits(bits) else {
        return Err(WavError::format(FormatKind::InvalidSize));
    };
    decode_g726(
        payload(bits),
        rate,
        G726Packing::Aal2,
        None,
        &DecodeOptions::unbounded(),
    )
}

#[test]
fn riff_matches_headerless_probe_and_stream() -> crate::Result<()> {
    for bits in [2u8, 3, 4, 5] {
        let raw = headerless(bits)?;
        let bytes = g726(payload(bits), u16::from(bits));
        let p = probe(&mut ByteSource::from_slice(&bytes))?;
        assert_eq!(p.codec, ProbeCodec::G726);
        assert_eq!(p.sample_rate, 8_000);
        assert_eq!(p.channels, 1);
        assert_eq!(p.declared_frames, Some(160));
        let decoded = decode_bytes(&bytes, DecodeOptions::unbounded())?;
        assert_eq!(decoded.sample_rate, 8_000);
        assert_eq!(decoded.channels, raw.channels);
        let mut streamed = Vec::new();
        let info = decode_streaming(
            &mut ByteSource::from_slice(&bytes),
            &DecodeOptions::speech(),
            |block| {
                assert_eq!(block.sample_rate, 8_000);
                assert_eq!(block.planar.len(), 1);
                streamed.extend_from_slice(block.planar[0]);
                Ok(())
            },
        )?;
        assert_eq!(info.frames, 160);
        assert_eq!(info.channels, 1);
        assert_eq!(streamed, decoded.channels[0]);
    }
    let bytes = g726(payload(4), 4);
    let from_reader = decode_reader(Cursor::new(bytes.clone()), &DecodeOptions::unbounded())?;
    assert_eq!(from_reader.frames(), 160);
    let dir = tempfile::TempDir::new()?;
    let path = dir.path().join("g726.wav");
    std::fs::write(&path, &bytes)?;
    let from_file = read(&path)?;
    assert_eq!(from_file.frames(), 160);
    assert_eq!(from_file.sample_rate, 8_000);
    let mut short = OneByte {
        data: bytes.clone(),
        pos: 0,
    };
    let from_short = decode_with(
        &mut ByteSource::from_read_seek(&mut short, Some(bytes.len() as u64)),
        &DecodeOptions::unbounded(),
    )?;
    assert_eq!(from_short.channels, from_reader.channels);
    Ok(())
}

struct OneByte {
    data: Vec<u8>,
    pos: usize,
}

impl Read for OneByte {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.pos >= self.data.len() || buf.is_empty() {
            return Ok(0);
        }
        buf[0] = self.data[self.pos];
        self.pos += 1;
        Ok(1)
    }
}

impl Seek for OneByte {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let next = match pos {
            SeekFrom::Start(n) => n as usize,
            SeekFrom::Current(n) => (self.pos as i64 + n) as usize,
            SeekFrom::End(n) => (self.data.len() as i64 + n) as usize,
        };
        self.pos = next;
        Ok(self.pos as u64)
    }
}

#[test]
fn containers_and_fact_agree() -> crate::Result<()> {
    let raw = headerless(4)?;
    let mut spec = fields(WAVE_FORMAT_G726, 1, 8_000, 4_000, 1, 4);
    spec.be = true;
    let rifx = decode_bytes(&wave(payload(4), &spec), DecodeOptions::unbounded())?;
    assert_eq!(rifx.channels, raw.channels);
    let rf = decode_bytes(
        &rf64(payload(4), b"RF64", Some(160), None),
        DecodeOptions::unbounded(),
    )?;
    assert_eq!(rf.channels, raw.channels);
    let bw64 = decode_bytes(
        &rf64(payload(4), b"BW64", Some(7), None),
        DecodeOptions::unbounded(),
    )?;
    assert_eq!(bw64.frames(), 7);
    assert_eq!(bw64.channels[0], raw.channels[0][..7]);
    let huge = decode_bytes(
        &rf64(payload(4), b"RF64", Some(u64::MAX), Some(u64::MAX)),
        DecodeOptions::unbounded(),
    )?;
    assert_eq!(huge.frames(), 160);
    let w64 = decode_bytes(&wave64(payload(4)), DecodeOptions::unbounded())?;
    assert_eq!(w64.channels, raw.channels);
    let mut capped = spec;
    capped.be = false;
    capped.fact = Some(7);
    let fact = decode_bytes(&wave(payload(4), &capped), DecodeOptions::unbounded())?;
    assert_eq!(fact.frames(), 7);
    capped.fact = Some(u32::MAX);
    let ignored = decode_bytes(&wave(payload(4), &capped), DecodeOptions::unbounded())?;
    assert_eq!(ignored.frames(), 160);
    let mut sixteen = spec;
    sixteen.be = false;
    sixteen.fmt_len = 16;
    let fmt16 = decode_bytes(&wave(payload(4), &sixteen), DecodeOptions::unbounded())?;
    assert_eq!(fmt16.channels, raw.channels);
    Ok(())
}

fn rf64(payload: &[u8], magic: &[u8; 4], samples: Option<u64>, data_size: Option<u64>) -> Vec<u8> {
    let mut fmt = Vec::new();
    fmt.extend_from_slice(&WAVE_FORMAT_G726.to_le_bytes());
    fmt.extend_from_slice(&1u16.to_le_bytes());
    fmt.extend_from_slice(&8_000u32.to_le_bytes());
    fmt.extend_from_slice(&4_000u32.to_le_bytes());
    fmt.extend_from_slice(&1u16.to_le_bytes());
    fmt.extend_from_slice(&4u16.to_le_bytes());
    fmt.extend_from_slice(&0u16.to_le_bytes());
    let mut ds64 = vec![0u8; 28];
    ds64[8..16].copy_from_slice(&data_size.unwrap_or(payload.len() as u64).to_le_bytes());
    ds64[16..24].copy_from_slice(&samples.unwrap_or(0).to_le_bytes());
    let mut body = Vec::new();
    body.extend_from_slice(b"ds64");
    body.extend_from_slice(&28u32.to_le_bytes());
    body.extend_from_slice(&ds64);
    body.extend_from_slice(b"fmt ");
    body.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
    body.extend_from_slice(&fmt);
    body.extend_from_slice(b"data");
    body.extend_from_slice(&u32::MAX.to_le_bytes());
    body.extend_from_slice(payload);
    let riff_size = 4 + body.len() as u64;
    body[8..16].copy_from_slice(&riff_size.to_le_bytes());
    let mut file = Vec::new();
    file.extend_from_slice(magic);
    file.extend_from_slice(&u32::MAX.to_le_bytes());
    file.extend_from_slice(b"WAVE");
    file.extend_from_slice(&body);
    file
}

fn wave64(payload: &[u8]) -> Vec<u8> {
    let mut fmt = Vec::new();
    fmt.extend_from_slice(&WAVE_FORMAT_G726.to_le_bytes());
    fmt.extend_from_slice(&1u16.to_le_bytes());
    fmt.extend_from_slice(&8_000u32.to_le_bytes());
    fmt.extend_from_slice(&4_000u32.to_le_bytes());
    fmt.extend_from_slice(&1u16.to_le_bytes());
    fmt.extend_from_slice(&4u16.to_le_bytes());
    fmt.extend_from_slice(&0u16.to_le_bytes());
    let mut file = Vec::new();
    file.extend_from_slice(&W64_GUID_RIFF);
    let size_at = file.len();
    file.extend_from_slice(&0u64.to_le_bytes());
    file.extend_from_slice(&W64_GUID_WAVE);
    let push = |file: &mut Vec<u8>, guid: &[u8; 16], data: &[u8], pad: bool| {
        file.extend_from_slice(guid);
        let chunk = 24 + data.len() as u64;
        file.extend_from_slice(&chunk.to_le_bytes());
        file.extend_from_slice(data);
        if pad {
            let n = (8 - (data.len() % 8)) % 8;
            file.extend(std::iter::repeat_n(0u8, n));
        }
    };
    push(&mut file, &W64_GUID_FMT, &fmt, true);
    push(&mut file, &W64_GUID_DATA, payload, false);
    let total = file.len() as u64;
    file[size_at..size_at + 8].copy_from_slice(&total.to_le_bytes());
    file
}

#[test]
fn tag_0064_policy() -> crate::Result<()> {
    let shaped = fields(WAVE_FORMAT_G722_ASTERISK, 1, 8_000, 4_000, 1, 4);
    let bytes = wave(payload(4), &shaped);
    let err = probe(&mut ByteSource::from_slice(&bytes)).unwrap_err();
    assert!(matches!(err, WavError::Format(FormatKind::AmbiguousCodec)));
    assert!(err.to_string().contains("0x0064"));
    let aal2 = DecodeOptions::unbounded().with_tag_0064(Tag0064::G726 {
        packing: G726Packing::Aal2,
    });
    let decoded = decode_bytes(&bytes, aal2.clone())?;
    assert_eq!(decoded.channels, headerless(4)?.channels);
    let p = probe_with(&mut ByteSource::from_slice(&bytes), &aal2)?;
    assert_eq!(p.codec, ProbeCodec::G726);
    assert_eq!(p.sample_rate, 8_000);
    let rfc = DecodeOptions::unbounded().with_tag_0064(Tag0064::G726 {
        packing: G726Packing::Rfc3551,
    });
    assert_ne!(decode_bytes(&bytes, rfc)?.channels[0], decoded.channels[0]);
    let forced = DecodeOptions::unbounded().with_tag_0064(Tag0064::G722);
    let as_g722 = decode_bytes(&bytes, forced.clone())?;
    assert_eq!(as_g722.sample_rate, 16_000);
    assert_eq!(
        probe_with(&mut ByteSource::from_slice(&bytes), &forced)?.codec,
        ProbeCodec::G722
    );
    let mut asterisk = shaped;
    asterisk.byte_rate = 8_000;
    let classic = wave(&[0x11, 0x22, 0x33, 0x44], &asterisk);
    let auto = decode_bytes(&classic, DecodeOptions::unbounded())?;
    assert_eq!(auto.sample_rate, 16_000);
    assert_eq!(
        probe(&mut ByteSource::from_slice(&classic))?.codec,
        ProbeCodec::G722
    );
    let fixed = g726(payload(4), 4);
    let still = decode_bytes(
        &fixed,
        DecodeOptions::unbounded().with_tag_0064(Tag0064::G722),
    )?;
    assert_eq!(still.sample_rate, 8_000);
    assert_eq!(still.channels, decoded.channels);
    Ok(())
}

#[test]
fn malformed_limits_and_long_windows() -> crate::Result<()> {
    let g = |bits, ch, rate, byte_rate, block| {
        fields(WAVE_FORMAT_G726, ch, rate, byte_rate, block, bits)
    };
    let reject = |bytes: Vec<u8>, pred: fn(&WavError) -> bool| {
        pred(&decode_bytes(&bytes, DecodeOptions::unbounded()).unwrap_err())
    };
    assert!(reject(
        wave(payload(4), &g(4, 2, 8_000, 8_000, 1)),
        |e| matches!(e, WavError::Format(FormatKind::ChannelLayout))
    ));
    assert!(reject(
        wave(payload(4), &fields(0x0014, 1, 8_000, 4_000, 1, 4)),
        |e| matches!(e, WavError::UnsupportedCodec { tag: 0x0014 })
    ));
    assert!(reject(
        wave(payload(4), &fields(0x0040, 1, 8_000, 4_000, 1, 4)),
        |e| matches!(e, WavError::UnsupportedCodec { tag: 0x0040 })
    ));
    assert!(reject(
        wave(payload(4), &g(4, 1, 16_000, 8_000, 1)),
        |e| matches!(e, WavError::UnsupportedSampleRate { rate: 16_000, .. })
    ));
    assert!(reject(
        wave(payload(4), &g(4, 1, 0, 4_000, 1)),
        |e| matches!(e, WavError::UnsupportedSampleRate { rate: 0, .. })
    ));
    assert!(reject(
        wave(payload(3), &g(3, 1, 8_000, 3_000, 1)),
        |e| matches!(e, WavError::Format(FormatKind::InvalidSize))
    ));
    assert!(reject(
        wave(payload(4), &g(5, 1, 8_000, 4_000, 5)),
        |e| matches!(e, WavError::Format(FormatKind::MalformedFmt))
    ));
    assert!(reject(
        wave(payload(4), &g(0, 1, 8_000, 0, 1)),
        |e| matches!(e, WavError::Format(FormatKind::InvalidSize))
    ));
    let mut claimed = g(4, 1, 8_000, 4_000, 1);
    claimed.data_claim = Some(10_000);
    let mut short = wave(&payload(4)[..4], &claimed);
    // RIFF size 0 is an unknown length. The data claim is clamped to the file.
    short[4..8].copy_from_slice(&0u32.to_le_bytes());
    assert_eq!(
        decode_bytes(&short, DecodeOptions::unbounded())?.frames(),
        8
    );
    let bytes = g726(&[0u8; 12_000], 5);
    let opts = DecodeOptions::speech().with_max_duration_secs(0.01);
    assert!(matches!(
        decode_bytes(&bytes, opts.clone()),
        Err(WavError::TooLong { .. })
    ));
    assert!(matches!(
        decode_streaming(&mut ByteSource::from_slice(&bytes), &opts, |_| Ok(())),
        Err(WavError::TooLong { .. })
    ));
    let ram = DecodeOptions::unbounded().with_max_output_bytes(32);
    assert!(matches!(
        decode_bytes(&g726(payload(4), 4), ram),
        Err(WavError::OutputTooLarge { .. })
    ));
    let full = decode_bytes(&bytes, DecodeOptions::unbounded())?;
    let mut streamed = Vec::new();
    decode_streaming(
        &mut ByteSource::from_slice(&bytes),
        &DecodeOptions::unbounded(),
        |block| {
            streamed.extend_from_slice(block.planar[0]);
            Ok(())
        },
    )?;
    assert_eq!(streamed, full.channels[0]);
    assert!(full.frames() > 8_000);
    Ok(())
}

#[test]
fn tag_0064_explicit_errors() -> crate::Result<()> {
    let g722 = fields(WAVE_FORMAT_G722_ASTERISK, 1, 16_000, 8_000, 1, 4);
    let bytes = wave(&[0u8; 8], &g722);
    let force = DecodeOptions::unbounded().with_tag_0064(Tag0064::G726 {
        packing: G726Packing::Aal2,
    });
    assert!(matches!(
        decode_bytes(&bytes, force.clone()),
        Err(WavError::UnsupportedSampleRate { rate: 16_000, .. })
    ));
    let mut shaped = g722;
    shaped.rate = 8_000;
    shaped.byte_rate = 4_000;
    shaped.bits = 5;
    let mismatch = wave(payload(4), &shaped);
    assert!(matches!(
        decode_bytes(&mismatch, force),
        Err(WavError::Format(FormatKind::MalformedFmt))
    ));
    let empty = g726(&[], 4);
    let decoded = decode_bytes(&empty, DecodeOptions::unbounded())?;
    assert_eq!(decoded.frames(), 0);
    let mut ds = rf64(payload(4), b"RF64", None, None);
    ds[16..20].copy_from_slice(&8u32.to_le_bytes());
    assert!(matches!(
        decode_bytes(&ds, DecodeOptions::unbounded()),
        Err(WavError::Format(FormatKind::MalformedChunk))
    ));
    Ok(())
}
