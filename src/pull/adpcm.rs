//! ADPCM pull / collect (MS + IMA), gated on the `adpcm` feature.

use super::{DecodePlan, StreamBlock, emit_mono_block, emit_split_block};
use crate::ChannelMode;
use crate::error::{FormatKind, Result, WavError};
#[cfg(test)]
use crate::header::FmtFields;
use crate::header::SampleCodec;
use crate::source::ByteSource;

#[cfg(all(test, feature = "adpcm"))]
use crate::adpcm::{decode_ima_adpcm, decode_ms_adpcm};
#[cfg(feature = "adpcm")]
use crate::adpcm::{for_each_ima_adpcm_block, for_each_ms_adpcm_block};

/// Frames implied by `block_align` (MS: 2 + 2*(ba/ch - 7);
/// IMA mono: 1 + 2*(ba - 4); IMA stereo: 1 + 8*floor((ba - 8)/8)).
/// Matches the nibble walk; header `samplesPerBlock` is ignored (0 / lie).
/// Stereo IMA leftover shorter than an 8-byte L/R group is dropped.
pub(crate) fn adpcm_est_frames(data_len: u64, ba: u64, ch: u64, ima: bool) -> u64 {
    let ch = ch.max(1);
    let spb = if ima {
        if ch >= 2 {
            ba.saturating_sub(8) / 8 * 8 + 1
        } else {
            ba.saturating_sub(4).saturating_mul(2) + 1
        }
    } else {
        ba.saturating_sub(7 * ch).saturating_mul(2) / ch + 2
    }
    .max(1);
    data_len
        .checked_div(ba)
        .map(|n| n.saturating_mul(spb))
        .unwrap_or(0)
}

/// Nibble estimate, then a smaller `fact` / ds64 count wins (same as PCM).
pub(crate) fn adpcm_frames_capped(
    data_len: u64,
    ba: u64,
    ch: u64,
    ima: bool,
    declared: Option<u64>,
) -> u64 {
    let est = adpcm_est_frames(data_len, ba, ch, ima);
    match declared {
        Some(sc) if sc > 0 => est.min(sc),
        _ => est,
    }
}

#[inline]
pub(crate) fn ensure_adpcm_enabled() -> Result<()> {
    #[cfg(feature = "adpcm")]
    {
        Ok(())
    }
    #[cfg(not(feature = "adpcm"))]
    {
        Err(WavError::FeatureDisabled { feature: "adpcm" })
    }
}

#[cfg(all(test, feature = "adpcm"))]
pub(crate) fn decode_adpcm_interleaved(
    mss: &mut ByteSource<'_>,
    fmt: &FmtFields,
    data_len: u64,
    max_samples: usize,
) -> Result<Vec<i16>> {
    match fmt.codec {
        SampleCodec::MsAdpcm => {
            let params = fmt
                .adpcm_ms
                .as_ref()
                .ok_or_else(|| WavError::format(FormatKind::Adpcm))?;
            decode_ms_adpcm(mss, params, data_len, max_samples)
        }
        SampleCodec::ImaAdpcm => {
            let params = fmt
                .adpcm_ima
                .as_ref()
                .ok_or_else(|| WavError::format(FormatKind::Adpcm))?;
            decode_ima_adpcm(mss, params, data_len, max_samples)
        }
        _ => Err(WavError::unsupported_codec(0)),
    }
}

#[cfg(all(test, not(feature = "adpcm")))]
pub(crate) fn decode_adpcm_interleaved(
    _mss: &mut ByteSource<'_>,
    _fmt: &FmtFields,
    _data_len: u64,
    _max_samples: usize,
) -> Result<Vec<i16>> {
    Err(WavError::FeatureDisabled { feature: "adpcm" })
}

#[cfg(feature = "adpcm")]
fn visit_adpcm(
    mss: &mut ByteSource<'_>,
    plan: &DecodePlan,
    on_block: impl FnMut(&[i16]) -> Result<()>,
) -> Result<usize> {
    match plan.codec {
        SampleCodec::MsAdpcm => {
            let params = plan
                .fmt
                .adpcm_ms
                .as_ref()
                .ok_or_else(|| WavError::format(FormatKind::Adpcm))?;
            for_each_ms_adpcm_block(
                mss,
                params,
                plan.data_len,
                plan.max_samples,
                plan.total_frames,
                plan.sample_rate,
                plan.big_endian,
                on_block,
            )
        }
        SampleCodec::ImaAdpcm => {
            let params = plan
                .fmt
                .adpcm_ima
                .as_ref()
                .ok_or_else(|| WavError::format(FormatKind::Adpcm))?;
            for_each_ima_adpcm_block(
                mss,
                params,
                plan.data_len,
                plan.max_samples,
                plan.total_frames,
                plan.sample_rate,
                plan.big_endian,
                on_block,
            )
        }
        _ => Err(WavError::unsupported_codec(0)),
    }
}

pub(super) fn collect_adpcm(mss: &mut ByteSource<'_>, plan: &DecodePlan) -> Result<Vec<Vec<f32>>> {
    ensure_adpcm_enabled()?;
    #[cfg(not(feature = "adpcm"))]
    {
        let _ = (mss, plan);
        Err(WavError::FeatureDisabled { feature: "adpcm" })
    }
    #[cfg(feature = "adpcm")]
    {
        let ch = plan.channels.max(1);
        let n_out = match plan.mode {
            ChannelMode::Mono => 1,
            ChannelMode::Split => ch,
        };
        let mut out: Vec<Vec<f32>> = (0..n_out)
            .map(|_| Vec::with_capacity(plan.total_frames))
            .collect();
        visit_adpcm(mss, plan, |interleaved| {
            let planes = crate::adpcm::i16_frames_to_f32(interleaved, ch, plan.mode);
            for (dst, src) in out.iter_mut().zip(planes) {
                dst.extend_from_slice(&src);
            }
            Ok(())
        })?;
        Ok(out)
    }
}

pub(super) fn pull_adpcm<F>(
    mss: &mut ByteSource<'_>,
    plan: &DecodePlan,
    on_block: &mut F,
) -> Result<usize>
where
    F: FnMut(StreamBlock<'_>) -> Result<()>,
{
    ensure_adpcm_enabled()?;
    #[cfg(not(feature = "adpcm"))]
    {
        let _ = (mss, plan, on_block);
        Err(WavError::FeatureDisabled { feature: "adpcm" })
    }
    #[cfg(feature = "adpcm")]
    {
        let ch = plan.channels.max(1);
        let sample_rate = plan.sample_rate;
        let mode = plan.mode;
        visit_adpcm(mss, plan, |interleaved| {
            let planes = crate::adpcm::i16_frames_to_f32(interleaved, ch, mode);
            match mode {
                ChannelMode::Mono => emit_mono_block(sample_rate, &planes[0], on_block)?,
                ChannelMode::Split => {
                    emit_split_block(sample_rate, planes[0].len(), &planes, on_block)?;
                }
            }
            Ok(())
        })
    }
}
