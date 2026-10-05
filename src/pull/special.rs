//! Frame budget and dispatch for G.722, G.726, and GSM.

use super::{DecodePlan, g722, gsm};
use crate::error::{FormatKind, Result, WavError};
use crate::header::{FmtFields, SampleCodec};
use crate::source::ByteSource;

/// Frame size and PCM frame count for G.722, GSM, and G.726.
///
/// `Ok(None)` means some other codec. The inner count is `None` only when
/// `data_len` is `None` and no fact or ds64 count was declared. A known
/// byte length always yields a count. G.726 bit width outside 2..=5 is
/// [`FormatKind::InvalidSize`] even when the length is unknown.
pub(crate) fn framed_count(
    fmt: &FmtFields,
    data_len: Option<u64>,
    declared: Option<u64>,
) -> Result<Option<(usize, Option<u64>)>> {
    let frame_bytes = match fmt.codec {
        SampleCodec::G722 => fmt.channels,
        SampleCodec::Gsm => crate::gsm::MS_BLOCK,
        SampleCodec::G726 => {
            g726_bits(fmt)?;
            0
        }
        _ => return Ok(None),
    };
    let Some(len) = data_len else {
        return Ok(Some((frame_bytes, declared)));
    };
    let frames = match fmt.codec {
        SampleCodec::G722 => crate::g722::pcm_frames_capped(len, fmt.channels, declared),
        SampleCodec::Gsm => crate::gsm::pcm_frames_capped(len, declared),
        SampleCodec::G726 => crate::g726::pcm_frames_capped(len, g726_bits(fmt)?, declared),
        _ => return Ok(None),
    };
    Ok(Some((frame_bytes, Some(frames))))
}

pub(super) fn pull_framed<F>(
    mss: &mut ByteSource<'_>,
    plan: &DecodePlan,
    on_block: &mut F,
) -> Result<Option<usize>>
where
    F: FnMut(super::StreamBlock<'_>) -> Result<()>,
{
    let n = match plan.codec {
        SampleCodec::G722 => g722::pull_g722(mss, plan, on_block)?,
        SampleCodec::Gsm => gsm::pull_gsm(mss, plan, on_block)?,
        SampleCodec::G726 => super::g726::pull_g726(mss, plan, on_block)?,
        _ => return Ok(None),
    };
    Ok(Some(n))
}

pub(super) fn collect_framed(
    mss: &mut ByteSource<'_>,
    plan: &DecodePlan,
) -> Result<Option<Vec<Vec<f32>>>> {
    let planes = match plan.codec {
        SampleCodec::G722 => g722::collect_g722(mss, plan)?,
        SampleCodec::Gsm => gsm::collect_gsm(mss, plan)?,
        SampleCodec::G726 => super::g726::collect_g726(mss, plan)?,
        _ => return Ok(None),
    };
    Ok(Some(planes))
}

fn g726_bits(fmt: &FmtFields) -> Result<u8> {
    fmt.g726
        .as_ref()
        .map(|p| p.bits)
        .filter(|b| (2..=5).contains(b))
        .ok_or_else(|| WavError::format(FormatKind::InvalidSize))
}
