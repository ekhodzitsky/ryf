//! Classic RIFF / RIFX / RF64 / BW64 walk, plus Sony Wave64.

use super::fmt::parse_fmt_chunk;
use super::{
    Container, FmtFields, W64_GUID_DATA, W64_GUID_FACT, W64_GUID_FMT, W64_GUID_RIFF, W64_GUID_WAVE,
    WavHeader,
};
use crate::error::{FormatKind, Result, WavError};
use crate::source::ByteSource;
use std::io::{self, Read, Seek, SeekFrom};

pub(super) fn eof_trunc(err: io::Error) -> WavError {
    if err.kind() == io::ErrorKind::UnexpectedEof {
        WavError::format(FormatKind::Truncated)
    } else {
        WavError::Io(err)
    }
}

fn wav_header(
    mss: &mut ByteSource<'_>,
    fmt: FmtFields,
    data_pos: u64,
    declared_data_len: Option<u64>,
    declared_sample_count: Option<u64>,
) -> Result<WavHeader> {
    if mss.pos() != data_pos {
        mss.seek(SeekFrom::Start(data_pos)).map_err(eof_trunc)?;
    }
    Ok(WavHeader {
        fmt,
        declared_data_len,
        data_pos,
        declared_sample_count,
    })
}

pub(crate) fn parse_header(mss: &mut ByteSource<'_>) -> Result<WavHeader> {
    parse_header_inner(mss)
}

fn parse_header_inner(mss: &mut ByteSource<'_>) -> Result<WavHeader> {
    // Peek up to 16 bytes to distinguish classic RIFF from Sony W64.
    // A 12-byte `RIFF....WAVE` is a complete empty container (MissingChunk),
    // not Truncated: do not require 16 bytes up front.
    let mut head16 = [0u8; 16];
    let mut filled = 0usize;
    while filled < head16.len() {
        match mss.read(&mut head16[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) => return Err(eof_trunc(e)),
        }
    }
    if filled == 16 && head16 == W64_GUID_RIFF {
        // Stream is at offset 16; W64 path continues.
        return parse_header_w64(mss);
    }
    mss.seek(SeekFrom::Start(0)).map_err(eof_trunc)?;

    let marker = mss.read_quad_bytes().map_err(eof_trunc)?;
    let container = match &marker {
        b"RIFF" => Container::Riff,
        b"RIFX" => Container::Rifx,
        b"RF64" | b"BW64" => Container::Rf64,
        _ => return Err(WavError::NotWave),
    };
    let is_rf64 = matches!(container, Container::Rf64);
    let be = container.big_endian();

    let riff_len_u32 = read_u32_endian(mss, be)?;
    // RF64 always uses 0xFFFFFFFF here; real size comes from ds64.
    if !is_rf64 && riff_len_u32 < 4 {
        return Err(WavError::format(FormatKind::MalformedChunk));
    }

    let riff_form = mss.read_quad_bytes().map_err(eof_trunc)?;
    if &riff_form != b"WAVE" {
        return Err(WavError::NotWave);
    }

    // A riff length of u32::MAX marks unknown size (ffmpeg stdout) or RF64.
    let mut riff_data_len: Option<u64> = if is_rf64 || riff_len_u32 == u32::MAX {
        None
    } else {
        Some(u64::from(riff_len_u32 - 4))
    };

    let mut consumed: u64 = 0;
    let mut fmt: Option<FmtFields> = None;
    let mut ds64_data_size: Option<u64> = None;
    let mut ds64_sample_count: Option<u64> = None;
    let mut ds64_table: Vec<([u8; 4], u64)> = Vec::new();
    let mut fact_sample_count: Option<u64> = None;
    let mut saw_ds64 = false;
    // First `data` payload when it appears before `fmt `.
    let mut pending_data: Option<(u64, Option<u64>)> = None;

    loop {
        if let Some(len) = riff_data_len
            && consumed >= len
        {
            break;
        }

        if consumed & 0x1 == 1 {
            let _pad = mss.read_u8().map_err(eof_trunc)?;
            consumed += 1;
        }

        if let Some(len) = riff_data_len
            && consumed + 8 > len
        {
            break;
        }

        let tag = match mss.read_quad_bytes() {
            Ok(t) => t,
            Err(e)
                if e.kind() == io::ErrorKind::UnexpectedEof
                    && fmt.is_some()
                    && pending_data.is_some() =>
            {
                break;
            }
            Err(e) => return Err(eof_trunc(e)),
        };
        let chunk_len_u32 = read_u32_endian(mss, be)?;
        consumed += 8;

        // RF64: data size 0xFFFFFFFF is ds64.dataSize. Other sentinels
        // (JUNK/LIST/...) use the ds64 size table (EBU Tech 3306).
        let mut chunk_len = u64::from(chunk_len_u32);
        if chunk_len_u32 == u32::MAX && tag != *b"ds64" && tag != *b"data" && tag != *b"fmt " {
            chunk_len = ds64_table
                .iter()
                .find(|(id, _)| id == &tag)
                .map(|(_, sz)| *sz)
                .ok_or_else(|| WavError::format(FormatKind::MalformedChunk))?;
        }

        if let Some(len) = riff_data_len
            && len - consumed < chunk_len
            && chunk_len_u32 != u32::MAX
        {
            return Err(WavError::format(FormatKind::MalformedChunk));
        }
        // Sentinel `data`/`ds64` sizes are not added to consumed here.
        if chunk_len_u32 != u32::MAX || (tag != *b"data" && tag != *b"ds64" && tag != *b"fmt ") {
            consumed = consumed.saturating_add(chunk_len);
        }

        match &tag {
            b"ds64" => {
                // EBU Tech 3306: riffSize, dataSize, sampleCount, tableLength, ...
                if chunk_len < 28 {
                    return Err(WavError::format(FormatKind::MalformedChunk));
                }
                if saw_ds64 {
                    return Err(WavError::format(FormatKind::MalformedChunk));
                }
                saw_ds64 = true;
                let riff_size = read_u64_le(mss)?;
                let data_size = read_u64_le(mss)?;
                let sample_count = read_u64_le(mss)?;
                let table_len = mss.read_u32().map_err(eof_trunc)?;
                // tableLength entries of chunkId (4) + chunkSize (8).
                let rest = chunk_len.saturating_sub(28);
                let n_entries = table_len.min(32).min((rest / 12) as u32);
                ds64_table.clear();
                for _ in 0..n_entries {
                    let id = mss.read_quad_bytes().map_err(eof_trunc)?;
                    let sz = read_u64_le(mss)?;
                    ds64_table.push((id, sz));
                }
                let read = u64::from(n_entries).saturating_mul(12);
                if rest > read {
                    mss.ignore_bytes(rest - read).map_err(eof_trunc)?;
                }
                // riffSize is the size of the RF64 chunk body after the first 8 bytes,
                // i.e. same meaning as the RIFF chunk size field: data after the form is riffSize-4.
                if riff_size >= 4 {
                    riff_data_len = Some(riff_size - 4);
                }
                ds64_data_size = Some(data_size);
                if sample_count > 0 {
                    ds64_sample_count = Some(sample_count);
                }
                if chunk_len_u32 == u32::MAX {
                    consumed = consumed.saturating_add(chunk_len);
                }
            }
            b"fmt " => {
                if chunk_len_u32 == u32::MAX {
                    return Err(WavError::format(FormatKind::MalformedChunk));
                }
                let mut f = parse_fmt_chunk(mss, chunk_len_u32, be)?;
                f.big_endian = be;
                fmt = Some(f);
            }
            b"fact" => {
                // Canonical fact is 4 bytes. ffmpeg accepts longer on RIFF
                // (BWF tails); take the first 4 and skip the rest.
                if chunk_len < 4 {
                    return Err(WavError::format(FormatKind::MalformedChunk));
                }
                let n = u64::from(read_u32_endian(mss, be)?);
                if n > 0 {
                    fact_sample_count = Some(n);
                }
                if chunk_len > 4 {
                    mss.ignore_bytes(chunk_len - 4).map_err(eof_trunc)?;
                }
            }
            b"LIST" => {
                if chunk_len < 4 {
                    return Err(WavError::format(FormatKind::MalformedChunk));
                }
                mss.ignore_bytes(chunk_len).map_err(eof_trunc)?;
            }
            b"data" => {
                let declared = if chunk_len_u32 == u32::MAX {
                    ds64_data_size
                } else {
                    Some(chunk_len)
                };
                // Canonical order: return on the first `data` after `fmt `.
                if pending_data.is_none()
                    && let Some(f) = fmt.take()
                {
                    if is_rf64 && !saw_ds64 {
                        return Err(WavError::format(FormatKind::MissingChunk));
                    }
                    return wav_header(
                        mss,
                        f,
                        mss.pos(),
                        declared,
                        ds64_sample_count.or(fact_sample_count),
                    );
                }
                // `data` before `fmt `, or a later `data` after a pending first
                // chunk: skip a known size so the walk can reach `fmt `.
                // Sentinel RF64 `data` (0xFFFFFFFF) cannot be skipped.
                if chunk_len_u32 == u32::MAX {
                    return Err(WavError::format(FormatKind::MissingChunk));
                }
                if pending_data.is_none() {
                    pending_data = Some((mss.pos(), declared));
                }
                mss.ignore_bytes(chunk_len).map_err(eof_trunc)?;
            }
            _ => {
                mss.ignore_bytes(chunk_len).map_err(eof_trunc)?;
            }
        }
    }

    match (fmt, pending_data) {
        (Some(f), Some((pos, declared))) => {
            if is_rf64 && !saw_ds64 {
                return Err(WavError::format(FormatKind::MissingChunk));
            }
            wav_header(
                mss,
                f,
                pos,
                declared,
                ds64_sample_count.or(fact_sample_count),
            )
        }
        _ => Err(WavError::format(FormatKind::MissingChunk)),
    }
}

pub(super) fn read_u64_le(mss: &mut ByteSource<'_>) -> Result<u64> {
    let mut buf = [0u8; 8];
    mss.read_buf_exact(&mut buf).map_err(eof_trunc)?;
    Ok(u64::from_le_bytes(buf))
}

pub(super) fn read_u32_endian(mss: &mut ByteSource<'_>, big_endian: bool) -> Result<u32> {
    let mut buf = [0u8; 4];
    mss.read_buf_exact(&mut buf).map_err(eof_trunc)?;
    Ok(if big_endian {
        u32::from_be_bytes(buf)
    } else {
        u32::from_le_bytes(buf)
    })
}

pub(super) fn read_u16_endian(mss: &mut ByteSource<'_>, big_endian: bool) -> Result<u16> {
    let mut buf = [0u8; 2];
    mss.read_buf_exact(&mut buf).map_err(eof_trunc)?;
    Ok(if big_endian {
        u16::from_be_bytes(buf)
    } else {
        u16::from_le_bytes(buf)
    })
}

/// Sony Wave64: stream is positioned after the 16-byte riff GUID.
fn parse_header_w64(mss: &mut ByteSource<'_>) -> Result<WavHeader> {
    // size of outer riff chunk (includes GUID+size fields).
    let _riff_size = read_u64_le(mss)?;
    let mut wave_guid = [0u8; 16];
    mss.read_buf_exact(&mut wave_guid).map_err(eof_trunc)?;
    if wave_guid != W64_GUID_WAVE {
        return Err(WavError::NotWave);
    }

    let mut fmt: Option<FmtFields> = None;
    let mut fact_sample_count: Option<u64> = None;
    let mut pending_data: Option<(u64, u64)> = None;

    loop {
        let mut guid = [0u8; 16];
        match mss.read_buf_exact(&mut guid) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(eof_trunc(e)),
        }
        // GUID was read; a short size field is Truncated, not MissingChunk.
        let chunk_size = read_u64_le(mss)?;
        // chunk_size includes the 24-byte header (16 GUID + 8 size).
        if chunk_size < 24 {
            return Err(WavError::format(FormatKind::MalformedChunk));
        }
        let data_len = chunk_size - 24;

        if guid == W64_GUID_FMT {
            if data_len > u64::from(u32::MAX) {
                return Err(WavError::format(FormatKind::MalformedChunk));
            }
            let mut f = parse_fmt_chunk(mss, data_len as u32, false)?;
            f.big_endian = false;
            fmt = Some(f);
        } else if guid == W64_GUID_FACT {
            if data_len < 4 {
                return Err(WavError::format(FormatKind::MalformedChunk));
            }
            let n = u64::from(mss.read_u32().map_err(eof_trunc)?);
            if n > 0 {
                fact_sample_count = Some(n);
            }
            if data_len > 4 {
                mss.ignore_bytes(data_len - 4).map_err(eof_trunc)?;
            }
        } else if guid == W64_GUID_DATA {
            // Sony/ffmpeg: size includes the 24-byte GUID+size header, not
            // the 8-byte alignment pad after the payload. A writer that
            // stuffed the pad into size makes those bytes PCM (ffmpeg too).
            if pending_data.is_none()
                && let Some(f) = fmt.take()
            {
                return wav_header(mss, f, mss.pos(), Some(data_len), fact_sample_count);
            }
            if pending_data.is_none() {
                pending_data = Some((mss.pos(), data_len));
            }
            mss.ignore_bytes(data_len).map_err(eof_trunc)?;
        } else {
            mss.ignore_bytes(data_len).map_err(eof_trunc)?;
        }

        // Pad to 8-byte boundary after chunk body.
        let pad = (8 - (data_len % 8)) % 8;
        if pad > 0 {
            mss.ignore_bytes(pad).map_err(eof_trunc)?;
        }
    }
    match (fmt, pending_data) {
        (Some(f), Some((pos, data_len))) => {
            wav_header(mss, f, pos, Some(data_len), fact_sample_count)
        }
        _ => Err(WavError::format(FormatKind::MissingChunk)),
    }
}
