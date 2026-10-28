//! G.726 pull / collect. Scratch is one input window, not the recording.

use super::{DecodePlan, check_duration, emit_mono_block};
use crate::error::{FormatKind, Result, WavError};
use crate::g726::{G726Decoder, G726Rate};
use crate::source::ByteSource;

const SCRATCH: usize = 4096;

pub(super) fn collect_g726(mss: &mut ByteSource<'_>, plan: &DecodePlan) -> Result<Vec<Vec<f32>>> {
    let mut out = vec![0.0; plan.total_frames];
    decode_windows(mss, plan, |pcm, offset| {
        for (dst, sample) in out[offset..offset + pcm.len()].iter_mut().zip(pcm) {
            *dst = f32::from(*sample) * crate::convert::I16_SCALE;
        }
        Ok(())
    })?;
    Ok(vec![out])
}
pub(super) fn pull_g726<F>(
    mss: &mut ByteSource<'_>,
    plan: &DecodePlan,
    on_block: &mut F,
) -> Result<usize>
where
    F: FnMut(super::StreamBlock<'_>) -> Result<()>,
{
    let mut plane = Vec::new();
    decode_windows(mss, plan, |pcm, _| {
        plane.resize(pcm.len(), 0.0);
        for (dst, sample) in plane.iter_mut().zip(pcm) {
            *dst = f32::from(*sample) * crate::convert::I16_SCALE;
        }
        emit_mono_block(plan.sample_rate, &plane, on_block)
    })
}
fn decode_windows(
    mss: &mut ByteSource<'_>,
    plan: &DecodePlan,
    mut visit: impl FnMut(&[i16], usize) -> Result<()>,
) -> Result<usize> {
    let total = plan.total_frames;
    check_duration(total, plan.max_samples, plan.sample_rate)?;
    if total == 0 {
        return Ok(0);
    }
    let (bits, mut dec) = decoder(plan)?;
    let mut raw = Vec::new();
    let mut pcm = vec![0i16; total.min(samples_cap(bits))];
    let mut produced = 0;
    while produced < total {
        let count = (total - produced).min(pcm.len());
        let want = bytes_needed(count, bits, dec.pending_bits())?;
        let bytes = mss
            .read_window(want, &mut raw)
            .map_err(WavError::packet_io)?;
        let (n, consumed) = dec.push(bytes, &mut pcm[..count]);
        if n == 0 || consumed != want {
            return Err(WavError::format(FormatKind::InvalidSize));
        }
        visit(&pcm[..n], produced)?;
        produced += n;
    }
    Ok(produced)
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
        .g726_params()
        .ok_or_else(|| WavError::format(FormatKind::InvalidSize))?;
    let rate = G726Rate::from_bits(params.bits)
        .ok_or_else(|| WavError::format(FormatKind::InvalidSize))?;
    Ok((params.bits, G726Decoder::new(rate, params.packing)))
}
