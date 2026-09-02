//! Classic RIFF / RIFX / RF64 / BW64 walk, plus Sony Wave64.

use super::fmt::parse_fmt_chunk;
use super::{
    Container, FmtFields, W64_GUID_DATA, W64_GUID_FACT, W64_GUID_FMT, W64_GUID_RIFF, W64_GUID_WAVE,
    WavHeader,
};
use crate::error::{FormatKind, Result, WavError};
use crate::source::ByteSource;

pub(crate) fn parse_header(mss: &mut ByteSource<'_>) -> Result<WavHeader> {
    parse_header_inner(mss)
}

fn parse_header_inner(mss: &mut ByteSource<'_>) -> Result<WavHeader> {
    // Peek first 16 bytes to distinguish classic RIFF family from Sony W64.
    let head16 = {
        let mut b = [0u8; 16];
        mss.read_buf_exact(&mut b)
            .map_err(|_e| WavError::format(FormatKind::Truncated))?;
        b
    };
    if head16 == W64_GUID_RIFF {
        // Rewind already consumed 16 - still at offset 16; W64 path continues.
        return parse_header_w64(mss);
    }
    // Classic fourcc containers: rewind to 0 and re-parse from the start.
    use std::io::{Seek, SeekFrom};
    mss.seek(SeekFrom::Start(0))
        .map_err(|_e| WavError::format(FormatKind::Truncated))?;

    let marker = mss.read_quad_bytes()?;
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

    let riff_form = mss.read_quad_bytes()?;
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
    let mut fact_sample_count: Option<u64> = None;
    let mut saw_ds64 = false;

    loop {
        if let Some(len) = riff_data_len
            && consumed >= len
        {
            break;
        }

        if consumed & 0x1 == 1 {
            let _pad = mss.read_u8()?;
            consumed += 1;
        }

        if let Some(len) = riff_data_len
            && consumed + 8 > len
        {
            break;
        }

        let tag = mss.read_quad_bytes()?;
        let chunk_len_u32 = read_u32_endian(mss, be)?;
        consumed += 8;

        // RF64: data/LIST chunk sizes may be 0xFFFFFFFF -> real size in ds64 table
        // (we only promote `data` via ds64.dataSize for product needs).
        let chunk_len = u64::from(chunk_len_u32);

        if let Some(len) = riff_data_len
            && len - consumed < chunk_len
            && chunk_len_u32 != u32::MAX
        {
            return Err(WavError::format(FormatKind::MalformedChunk));
        }
        // For 0xFFFFFFFF chunk sizes under RF64, do not advance consumed by
        // the sentinel; we still skip using the real size after parse.
        if chunk_len_u32 != u32::MAX {
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
                let table_len = mss.read_u32()?;
                // Skip optional size table (12 bytes per entry).
                let table_bytes = u64::from(table_len).saturating_mul(12);
                let rest = chunk_len.saturating_sub(28);
                let skip = table_bytes.min(rest);
                if skip > 0 {
                    mss.ignore_bytes(skip)?;
                }
                if rest > skip {
                    mss.ignore_bytes(rest - skip)?;
                }
                // riffSize is the size of the RF64 chunk body after the first 8 bytes,
                // i.e. same meaning as RIFF chunk size field -> data after form is riffSize-4.
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
                // Canonical fact is exactly 4 bytes (sample count). Longer
                // facts exist in some RF64 toolchains - accept >= 4 and skip
                // the surplus; shorter is malformed.
                if chunk_len < 4 {
                    return Err(WavError::format(FormatKind::MalformedChunk));
                }
                // Historical bit-exact gate: reject non-canonical 8-byte facts
                // on plain RIFF (ffmpeg also rejects). RF64 may ship longer.
                if !is_rf64 && chunk_len != 4 {
                    return Err(WavError::format(FormatKind::MalformedChunk));
                }
                fact_sample_count = Some(u64::from(read_u32_endian(mss, be)?));
                if chunk_len > 4 {
                    mss.ignore_bytes(chunk_len - 4)?;
                }
            }
            b"LIST" => {
                if chunk_len < 4 {
                    return Err(WavError::format(FormatKind::MalformedChunk));
                }
                if chunk_len_u32 == u32::MAX {
                    return Err(WavError::format(FormatKind::MalformedChunk));
                }
                mss.ignore_bytes(chunk_len)?;
            }
            b"data" => {
                let fmt = match fmt {
                    Some(fmt) => fmt,
                    None => return Err(WavError::format(FormatKind::MissingChunk)),
                };
                if is_rf64 && !saw_ds64 {
                    return Err(WavError::format(FormatKind::MissingChunk));
                }
                let declared = if chunk_len_u32 == u32::MAX {
                    ds64_data_size
                } else {
                    Some(chunk_len)
                };
                let sample_count = ds64_sample_count.or(fact_sample_count);
                return Ok(WavHeader {
                    fmt,
                    declared_data_len: declared,
                    data_pos: mss.pos(),
                    declared_sample_count: sample_count,
                });
            }
            _ => {
                if chunk_len_u32 == u32::MAX {
                    return Err(WavError::format(FormatKind::MalformedChunk));
                }
                mss.ignore_bytes(chunk_len)?;
            }
        }
    }

    Err(WavError::format(FormatKind::MissingChunk))
}

pub(super) fn read_u64_le(mss: &mut ByteSource<'_>) -> Result<u64> {
    let mut buf = [0u8; 8];
    mss.read_buf_exact(&mut buf)
        .map_err(|_e| WavError::format(FormatKind::Truncated))?;
    Ok(u64::from_le_bytes(buf))
}

pub(super) fn read_u32_endian(mss: &mut ByteSource<'_>, big_endian: bool) -> Result<u32> {
    let mut buf = [0u8; 4];
    mss.read_buf_exact(&mut buf)
        .map_err(|_e| WavError::format(FormatKind::Truncated))?;
    Ok(if big_endian {
        u32::from_be_bytes(buf)
    } else {
        u32::from_le_bytes(buf)
    })
}

pub(super) fn read_u16_endian(mss: &mut ByteSource<'_>, big_endian: bool) -> Result<u16> {
    let mut buf = [0u8; 2];
    mss.read_buf_exact(&mut buf)
        .map_err(|_e| WavError::format(FormatKind::Truncated))?;
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
    mss.read_buf_exact(&mut wave_guid)
        .map_err(|_e| WavError::format(FormatKind::Truncated))?;
    if wave_guid != W64_GUID_WAVE {
        return Err(WavError::NotWave);
    }

    let mut fmt: Option<FmtFields> = None;
    let mut fact_sample_count: Option<u64> = None;

    loop {
        let mut guid = [0u8; 16];
        match mss.read_buf_exact(&mut guid) {
            Ok(()) => {}
            Err(_) => break,
        }
        let chunk_size = match read_u64_le(mss) {
            Ok(s) => s,
            Err(_) => break,
        };
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
            fact_sample_count = Some(u64::from(mss.read_u32()?));
            if data_len > 4 {
                mss.ignore_bytes(data_len - 4)?;
            }
        } else if guid == W64_GUID_DATA {
            let fmt = fmt.ok_or_else(|| WavError::format(FormatKind::MissingChunk))?;
            // Align: after data there may be padding to 8 bytes; we stop here.
            return Ok(WavHeader {
                fmt,
                declared_data_len: Some(data_len),
                data_pos: mss.pos(),
                declared_sample_count: fact_sample_count,
            });
        } else {
            mss.ignore_bytes(data_len)?;
        }

        // Pad to 8-byte boundary after chunk body.
        let pad = (8 - (data_len % 8)) % 8;
        if pad > 0 {
            mss.ignore_bytes(pad)?;
        }
    }
    Err(WavError::format(FormatKind::MissingChunk))
}
