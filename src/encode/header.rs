//! RIFF, RF64, RIFX, and WAVEFORMATEXTENSIBLE headers.

use super::{WriteFormat, WriteSpec};
use crate::error::{Result, WavError};
use crate::header::{
    KSDATAFORMAT_SUBTYPE_ALAW, KSDATAFORMAT_SUBTYPE_IEEE_FLOAT, KSDATAFORMAT_SUBTYPE_MULAW,
    KSDATAFORMAT_SUBTYPE_PCM,
};

pub(super) const MAX_CHANNELS: u16 = 26;
pub(super) const PCM_RIFF_PREFIX: u32 = 36;
/// `fmt ` 18 + `fact` 4 (IEEE f32 and G.711).
pub(super) const FACT_RIFF_PREFIX: u32 = 50;
pub(super) const PCM_DATA_LEN_POS: u64 = 40;
pub(super) const FACT_DATA_LEN_POS: u64 = 54;
pub(super) const FACT_FRAMES_POS: u64 = 46;

pub(super) fn u32_len(n: usize) -> Result<u32> {
    u32::try_from(n).map_err(|_| WavError::RiffTooLarge)
}

pub(super) fn validate_spec(spec: WriteSpec) -> Result<()> {
    if spec.sample_rate == 0 {
        return Err(WavError::sample_rate(0, 1));
    }
    if spec.channels == 0 || spec.channels > MAX_CHANNELS {
        return Err(WavError::unsupported_codec(0));
    }
    Ok(())
}

pub(super) fn frame_bytes(spec: WriteSpec) -> Result<usize> {
    usize::from(spec.channels)
        .checked_mul(spec.format.bytes_per_sample())
        .filter(|&n| n > 0)
        .ok_or(WavError::RiffTooLarge)
}

pub(super) fn riff_prefix(spec: WriteSpec) -> u32 {
    if spec.format.has_fact() {
        FACT_RIFF_PREFIX
    } else {
        PCM_RIFF_PREFIX
    }
}

pub(super) fn data_len_pos(spec: WriteSpec) -> u64 {
    if spec.format.has_fact() {
        FACT_DATA_LEN_POS
    } else {
        PCM_DATA_LEN_POS
    }
}

pub(super) fn fact_frames_pos(spec: WriteSpec) -> Option<u64> {
    spec.format.has_fact().then_some(FACT_FRAMES_POS)
}

fn u16b(v: u16, be: bool) -> [u8; 2] {
    if be { v.to_be_bytes() } else { v.to_le_bytes() }
}

fn u32b(v: u32, be: bool) -> [u8; 4] {
    if be { v.to_be_bytes() } else { v.to_le_bytes() }
}

fn pcm_layout(spec: WriteSpec) -> Result<(u16, u32)> {
    let width = spec.format.bytes_per_sample() as u16;
    let block = spec.channels.saturating_mul(width);
    let byte_rate = spec
        .sample_rate
        .checked_mul(u32::from(block))
        .ok_or(WavError::RiffTooLarge)?;
    Ok((block, byte_rate))
}

/// Classic RIFF header (PCM / IEEE / G.711).
pub(super) fn push_header(out: &mut Vec<u8>, spec: WriteSpec, data_len: u32) -> Result<()> {
    push_classic(out, spec, data_len, false)
}

/// RIFX header. Same layout as [`push_header`], big-endian sizes.
pub(super) fn push_rifx_header(out: &mut Vec<u8>, spec: WriteSpec, data_len: u32) -> Result<()> {
    push_classic(out, spec, data_len, true)
}

/// RIFF size: prefix + payload + 0/1 pad byte (chunk size itself stays odd).
pub(super) fn riff_body_len(prefix: u32, data_len: u32) -> Result<u32> {
    let pad = data_len & 1;
    prefix
        .checked_add(data_len)
        .and_then(|n| n.checked_add(pad))
        .ok_or(WavError::RiffTooLarge)
}

pub(super) fn padded_data_len(data_len: u64) -> u64 {
    data_len.saturating_add(data_len & 1)
}

fn push_classic(out: &mut Vec<u8>, spec: WriteSpec, data_len: u32, be: bool) -> Result<()> {
    if spec.format.has_fact() {
        let (block, _) = pcm_layout(spec)?;
        let frames = data_len.checked_div(u32::from(block).max(1)).unwrap_or(0);
        let riff_len = riff_body_len(FACT_RIFF_PREFIX, data_len)?;
        out.extend_from_slice(if be { b"RIFX" } else { b"RIFF" });
        out.extend_from_slice(&u32b(riff_len, be));
        out.extend_from_slice(b"WAVE");
        push_fact_fmt_data(out, spec, data_len, frames, be)
    } else {
        let riff_len = riff_body_len(PCM_RIFF_PREFIX, data_len)?;
        out.extend_from_slice(if be { b"RIFX" } else { b"RIFF" });
        out.extend_from_slice(&u32b(riff_len, be));
        out.extend_from_slice(b"WAVE");
        push_pcm_fmt_data(out, spec, data_len, be)
    }
}

pub(super) fn needs_rf64(spec: WriteSpec, data_len: u64) -> bool {
    u64::from(riff_prefix(spec)).saturating_add(padded_data_len(data_len)) > u64::from(u32::MAX)
}

/// RF64 header bytes including the `data` size field (payload follows).
pub(super) fn rf64_header_len(spec: WriteSpec) -> u64 {
    // RF64+WAVE (12) + ds64 (36) + fmt + optional fact + data hdr (8).
    if spec.format.has_fact() { 94 } else { 80 }
}

pub(super) const RF64_RIFF_SIZE_POS: u64 = 20;
pub(super) const RF64_DATA_SIZE_POS: u64 = 28;
pub(super) const RF64_SAMPLE_COUNT_POS: u64 = 36;
/// `fact` sampleCount in an RF64 IEEE / G.711 header (after ds64 + WAVEFORMATEX).
pub(super) const RF64_FACT_FRAMES_POS: u64 = 82;
/// `fact` sampleCount after ds64 + 40-byte extensible `fmt `.
pub(super) const RF64_EXTENSIBLE_FACT_FRAMES_POS: u64 = 104;

pub(super) fn push_rf64_header(
    out: &mut Vec<u8>,
    spec: WriteSpec,
    data_len: u64,
    frames: u64,
) -> Result<()> {
    let header_len = rf64_header_len(spec);
    let file_len = header_len.saturating_add(padded_data_len(data_len));
    let riff_size = file_len.saturating_sub(8);
    out.extend_from_slice(b"RF64");
    out.extend_from_slice(&u32::MAX.to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"ds64");
    out.extend_from_slice(&28u32.to_le_bytes());
    out.extend_from_slice(&riff_size.to_le_bytes());
    out.extend_from_slice(&data_len.to_le_bytes());
    out.extend_from_slice(&frames.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    if spec.format.has_fact() {
        push_fact_fmt_data(
            out,
            spec,
            u32::MAX,
            frames.min(u64::from(u32::MAX)) as u32,
            false,
        )
    } else {
        push_pcm_fmt_data(out, spec, u32::MAX, false)
    }
}

fn push_pcm_fmt_data(out: &mut Vec<u8>, spec: WriteSpec, data_len: u32, be: bool) -> Result<()> {
    let (block, byte_rate) = pcm_layout(spec)?;
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&u32b(16, be));
    out.extend_from_slice(&u16b(spec.format.tag(), be));
    out.extend_from_slice(&u16b(spec.channels, be));
    out.extend_from_slice(&u32b(spec.sample_rate, be));
    out.extend_from_slice(&u32b(byte_rate, be));
    out.extend_from_slice(&u16b(block, be));
    out.extend_from_slice(&u16b(spec.format.bits(), be));
    out.extend_from_slice(b"data");
    out.extend_from_slice(&u32b(data_len, be));
    Ok(())
}

fn push_fact_fmt_data(
    out: &mut Vec<u8>,
    spec: WriteSpec,
    data_len: u32,
    frames: u32,
    be: bool,
) -> Result<()> {
    let (block, byte_rate) = pcm_layout(spec)?;
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&u32b(18, be));
    out.extend_from_slice(&u16b(spec.format.tag(), be));
    out.extend_from_slice(&u16b(spec.channels, be));
    out.extend_from_slice(&u32b(spec.sample_rate, be));
    out.extend_from_slice(&u32b(byte_rate, be));
    out.extend_from_slice(&u16b(block, be));
    out.extend_from_slice(&u16b(spec.format.bits(), be));
    out.extend_from_slice(&u16b(0, be));
    out.extend_from_slice(b"fact");
    out.extend_from_slice(&u32b(4, be));
    out.extend_from_slice(&u32b(frames, be));
    out.extend_from_slice(b"data");
    out.extend_from_slice(&u32b(data_len, be));
    Ok(())
}

fn subtype_guid(fmt: WriteFormat) -> [u8; 16] {
    match fmt {
        WriteFormat::F32 => KSDATAFORMAT_SUBTYPE_IEEE_FLOAT,
        WriteFormat::ALaw => KSDATAFORMAT_SUBTYPE_ALAW,
        WriteFormat::MuLaw => KSDATAFORMAT_SUBTYPE_MULAW,
        _ => KSDATAFORMAT_SUBTYPE_PCM,
    }
}

fn speaker_mask(channels: u16) -> u32 {
    if channels == 0 || channels > 18 {
        0
    } else {
        (1u32 << channels) - 1
    }
}

/// RIFF size minus 8 minus `data` payload (`60` PCM, `72` with `fact`).
pub(super) fn extensible_riff_prefix(spec: WriteSpec) -> u32 {
    if spec.format.has_fact() { 72 } else { 60 }
}

pub(super) fn extensible_data_len_pos(spec: WriteSpec) -> u64 {
    if spec.format.has_fact() { 76 } else { 64 }
}

pub(super) fn extensible_fact_frames_pos(spec: WriteSpec) -> Option<u64> {
    spec.format.has_fact().then_some(68)
}

fn push_extensible_body(
    out: &mut Vec<u8>,
    spec: WriteSpec,
    data_len: u32,
    fact_frames: u32,
) -> Result<()> {
    let (block, byte_rate) = pcm_layout(spec)?;
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&0xFFFEu16.to_le_bytes());
    out.extend_from_slice(&spec.channels.to_le_bytes());
    out.extend_from_slice(&spec.sample_rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block.to_le_bytes());
    out.extend_from_slice(&spec.format.bits().to_le_bytes());
    out.extend_from_slice(&22u16.to_le_bytes());
    out.extend_from_slice(&spec.format.bits().to_le_bytes());
    out.extend_from_slice(&speaker_mask(spec.channels).to_le_bytes());
    out.extend_from_slice(&subtype_guid(spec.format));
    if spec.format.has_fact() {
        out.extend_from_slice(b"fact");
        out.extend_from_slice(&4u32.to_le_bytes());
        out.extend_from_slice(&fact_frames.to_le_bytes());
    }
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    Ok(())
}

/// RIFF `WAVEFORMATEXTENSIBLE` (40-byte `fmt `). PCM / IEEE / G.711.
pub(super) fn push_extensible_header(
    out: &mut Vec<u8>,
    spec: WriteSpec,
    data_len: u32,
) -> Result<()> {
    let (block, _) = pcm_layout(spec)?;
    let frames = data_len.checked_div(u32::from(block).max(1)).unwrap_or(0);
    let riff_len = riff_body_len(extensible_riff_prefix(spec), data_len)?;
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&riff_len.to_le_bytes());
    out.extend_from_slice(b"WAVE");
    push_extensible_body(out, spec, data_len, frames)
}

pub(super) fn rf64_extensible_header_len(spec: WriteSpec) -> u64 {
    if spec.format.has_fact() { 116 } else { 104 }
}

pub(super) fn needs_rf64_extensible(spec: WriteSpec, data_len: u64) -> bool {
    u64::from(extensible_riff_prefix(spec)).saturating_add(padded_data_len(data_len))
        > u64::from(u32::MAX)
}

/// RF64 + `WAVEFORMATEXTENSIBLE`.
pub(super) fn push_rf64_extensible_header(
    out: &mut Vec<u8>,
    spec: WriteSpec,
    data_len: u64,
    frames: u64,
) -> Result<()> {
    let header_len = rf64_extensible_header_len(spec);
    let file_len = header_len.saturating_add(padded_data_len(data_len));
    let riff_size = file_len.saturating_sub(8);
    out.extend_from_slice(b"RF64");
    out.extend_from_slice(&u32::MAX.to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"ds64");
    out.extend_from_slice(&28u32.to_le_bytes());
    out.extend_from_slice(&riff_size.to_le_bytes());
    out.extend_from_slice(&data_len.to_le_bytes());
    out.extend_from_slice(&frames.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    push_extensible_body(out, spec, u32::MAX, frames.min(u64::from(u32::MAX)) as u32)
}

/// Reverse each sample container (LE payload to RIFX data bytes). Width 1 is a copy.
pub(super) fn swap_sample_bytes(pcm: &[u8], width: usize) -> Vec<u8> {
    if width <= 1 {
        return pcm.to_vec();
    }
    let mut out = pcm.to_vec();
    for chunk in out.chunks_exact_mut(width) {
        chunk.reverse();
    }
    out
}
