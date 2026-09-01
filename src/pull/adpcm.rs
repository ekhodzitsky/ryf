//! ADPCM pull / collect (MS + IMA), gated on the `adpcm` feature.

use super::{DecodePlan, StreamBlock, emit_mono_block, emit_split_block};
use crate::ChannelMode;
use crate::error::{Result, WavError};
#[cfg(test)]
use crate::header::FmtFields;
use crate::header::SampleCodec;
use crate::source::ByteSource;

#[cfg(all(test, feature = "adpcm"))]
use crate::adpcm::{decode_ima_adpcm, decode_ms_adpcm};
#[cfg(feature = "adpcm")]
use crate::adpcm::{for_each_ima_adpcm_block, for_each_ms_adpcm_block};

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
                .ok_or_else(|| WavError::format("wav: missing MS-ADPCM params"))?;
            decode_ms_adpcm(mss, params, data_len, max_samples)
        }
        SampleCodec::ImaAdpcm => {
            let params = fmt
                .adpcm_ima
                .as_ref()
                .ok_or_else(|| WavError::format("wav: missing IMA-ADPCM params"))?;
            decode_ima_adpcm(mss, params, data_len, max_samples)
        }
        _ => Err(WavError::UnsupportedCodec),
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
                .ok_or_else(|| WavError::format("wav: missing MS-ADPCM params"))?;
            for_each_ms_adpcm_block(
                mss,
                params,
                plan.data_len,
                plan.max_samples,
                plan.sample_rate,
                on_block,
            )
        }
        SampleCodec::ImaAdpcm => {
            let params = plan
                .fmt
                .adpcm_ima
                .as_ref()
                .ok_or_else(|| WavError::format("wav: missing IMA-ADPCM params"))?;
            for_each_ima_adpcm_block(
                mss,
                params,
                plan.data_len,
                plan.max_samples,
                plan.sample_rate,
                on_block,
            )
        }
        _ => Err(WavError::UnsupportedCodec),
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
            append_i16_interleaved(interleaved, ch, plan.mode, &mut out);
            Ok(())
        })?;
        Ok(out)
    }
}

#[cfg(feature = "adpcm")]
fn append_i16_interleaved(
    interleaved: &[i16],
    channels: usize,
    mode: ChannelMode,
    out: &mut [Vec<f32>],
) {
    let ch = channels.max(1);
    match mode {
        ChannelMode::Mono => {
            let n_ch = ch as f32;
            if ch == 1 {
                for &s in interleaved {
                    out[0].push(s as f32 / 32_768.0);
                }
            } else {
                for frame in interleaved.chunks_exact(ch) {
                    let sum: f32 = frame.iter().map(|&s| s as f32 / 32_768.0).sum();
                    out[0].push(sum / n_ch);
                }
            }
        }
        ChannelMode::Split => {
            for frame in interleaved.chunks_exact(ch) {
                for (c, &s) in frame.iter().enumerate() {
                    out[c].push(s as f32 / 32_768.0);
                }
            }
        }
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
        let mut mono = Vec::new();
        let mut planar: Vec<Vec<f32>> = Vec::new();
        visit_adpcm(mss, plan, |interleaved| {
            let frames_this = interleaved.len() / ch;
            match mode {
                ChannelMode::Mono => {
                    mono.resize(frames_this, 0.0);
                    let n_ch = ch as f32;
                    if ch == 1 {
                        for (i, &s) in interleaved.iter().enumerate() {
                            mono[i] = s as f32 / 32_768.0;
                        }
                    } else {
                        for (i, frame) in interleaved.chunks_exact(ch).enumerate() {
                            let sum: f32 = frame.iter().map(|&s| s as f32 / 32_768.0).sum();
                            mono[i] = sum / n_ch;
                        }
                    }
                    emit_mono_block(sample_rate, &mono, on_block)?;
                }
                ChannelMode::Split => {
                    if planar.len() != ch {
                        planar = (0..ch).map(|_| Vec::new()).collect();
                    }
                    for p in &mut planar {
                        p.resize(frames_this, 0.0);
                    }
                    for (i, frame) in interleaved.chunks_exact(ch).enumerate() {
                        for (c, &s) in frame.iter().enumerate() {
                            planar[c][i] = s as f32 / 32_768.0;
                        }
                    }
                    emit_split_block(sample_rate, frames_this, &planar, on_block)?;
                }
            }
            Ok(())
        })
    }
}
