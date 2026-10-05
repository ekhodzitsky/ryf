//! G.726 pull / collect. Scratch is one input window, not the recording.

use super::{DecodePlan, check_duration, emit_mono_block, pcm_short};
use crate::error::{FormatKind, Result, WavError};
use crate::g726::{G726Decoder, G726Rate};
use crate::source::ByteSource;

const SCRATCH: usize = 4096;

pub(super) fn collect_g726(mss: &mut ByteSource<'_>, plan: &DecodePlan) -> Result<Vec<Vec<f32>>> {
    let total = plan.total_frames;
    if total > plan.max_samples {
        check_duration(total, plan.max_samples, plan.sample_rate)?;
    }
    if total == 0 {
        return Ok(super::empty_planes(plan.mode, plan.channels));
    }
    let mut pcm = vec![0i16; total];
    decode_window(mss, plan, &mut pcm)?;
    let mut plane = vec![0.0f32; total];
    for (dst, sample) in plane.iter_mut().zip(pcm.iter()) {
        *dst = f32::from(*sample) * crate::convert::I16_SCALE;
    }
    Ok(vec![plane])
}

pub(super) fn pull_g726<F>(
    mss: &mut ByteSource<'_>,
    plan: &DecodePlan,
    on_block: &mut F,
) -> Result<usize>
where
    F: FnMut(super::StreamBlock<'_>) -> Result<()>,
{
    let total = plan.total_frames;
    if total > plan.max_samples {
        check_duration(total, plan.max_samples, plan.sample_rate)?;
    }
    if total == 0 {
        return Ok(0);
    }
    let (bits, mut dec) = decoder(plan)?;
    let mut produced = 0usize;
    while produced < total {
        let left = total - produced;
        let (n, plane) = pull_chunk(mss, &mut dec, bits, left)?;
        emit_mono_block(plan.sample_rate, &plane[..n], on_block)?;
        produced += n;
    }
    Ok(produced)
}

fn decode_window(mss: &mut ByteSource<'_>, plan: &DecodePlan, pcm: &mut [i16]) -> Result<()> {
    let (bits, mut dec) = decoder(plan)?;
    let mut filled = 0usize;
    while filled < pcm.len() {
        let left = pcm.len() - filled;
        let n = fill_chunk(mss, &mut dec, bits, &mut pcm[filled..left + filled])?;
        filled += n;
    }
    Ok(())
}

/// Read just enough new bytes for `left` samples, counting bits already held.
fn fill_chunk(
    mss: &mut ByteSource<'_>,
    dec: &mut G726Decoder,
    bits: u8,
    out: &mut [i16],
) -> Result<usize> {
    let want = bytes_needed(out.len(), bits, dec.pending_bits())?;
    let chunk = want.min(SCRATCH);
    let mut raw = vec![0u8; chunk];
    if chunk > 0 {
        if let Some(rest) = mss.remaining_slice() {
            if rest.len() < chunk {
                return Err(pcm_short("wav: short g726 data"));
            }
            raw.copy_from_slice(&rest[..chunk]);
            mss.advance(chunk as u64).map_err(WavError::packet_io)?;
        } else {
            mss.read_buf_exact(&mut raw).map_err(WavError::packet_io)?;
        }
    }
    let (n, consumed) = dec.push(&raw, out);
    if n == 0 || consumed != chunk {
        return Err(WavError::format(FormatKind::InvalidSize));
    }
    Ok(n)
}

fn pull_chunk(
    mss: &mut ByteSource<'_>,
    dec: &mut G726Decoder,
    bits: u8,
    left: usize,
) -> Result<(usize, Vec<f32>)> {
    let mut pcm = vec![0i16; left.min(samples_cap(bits))];
    let n = fill_chunk(mss, dec, bits, &mut pcm)?;
    let mut plane = vec![0.0f32; n];
    for (dst, sample) in plane.iter_mut().zip(pcm[..n].iter()) {
        *dst = f32::from(*sample) * crate::convert::I16_SCALE;
    }
    Ok((n, plane))
}

fn samples_cap(bits: u8) -> usize {
    (SCRATCH.saturating_mul(8) + 7) / usize::from(bits.max(1))
}

fn bytes_needed(samples: usize, bits: u8, pending: u8) -> Result<usize> {
    let width = usize::from(bits);
    if width == 0 {
        return Err(WavError::format(FormatKind::InvalidSize));
    }
    let need = samples
        .checked_mul(width)
        .ok_or_else(|| WavError::format(FormatKind::InvalidSize))?;
    let missing = need.saturating_sub(usize::from(pending));
    Ok(missing
        .div_ceil(8)
        .min(SCRATCH)
        .max(if missing == 0 { 0 } else { 1 }))
}

fn decoder(plan: &DecodePlan) -> Result<(u8, G726Decoder)> {
    let params = plan
        .fmt
        .g726
        .as_ref()
        .ok_or_else(|| WavError::format(FormatKind::InvalidSize))?;
    let rate = G726Rate::from_bits(params.bits)
        .ok_or_else(|| WavError::format(FormatKind::InvalidSize))?;
    Ok((params.bits, G726Decoder::new(rate, params.packing)))
}
