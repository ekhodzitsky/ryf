//! Microsoft GSM 06.10 pull / collect (65-byte blocks to 320 PCM frames).

use std::io::{Seek, SeekFrom};

use super::{DecodePlan, check_duration, emit_mono_block, pcm_short, scratch_frames};
use crate::error::{Result, WavError};
use crate::gsm::{self, GsmDecoder, MS_BLOCK, MS_SAMPLES};
use crate::scrub::scrub_vec;
use crate::source::ByteSource;

pub(super) fn collect_gsm(mss: &mut ByteSource<'_>, plan: &DecodePlan) -> Result<Vec<Vec<f32>>> {
    let total = plan.total_frames;
    if total > plan.max_samples {
        check_duration(total, plan.max_samples, plan.sample_rate)?;
    }
    if total == 0 {
        return Ok(Vec::new());
    }
    let n_blocks = total.div_ceil(MS_SAMPLES);
    let need = n_blocks.saturating_mul(MS_BLOCK);
    let mut plane = if let Some(rest) = mss.remaining_slice() {
        if rest.len() < need {
            return Err(pcm_short("wav: short gsm data"));
        }
        gsm::decode_mono_f32(&rest[..need])
    } else {
        let mut owned = scrub_vec(vec![0u8; need]);
        mss.read_buf_exact(&mut owned)
            .map_err(WavError::packet_io)?;
        gsm::decode_mono_f32(&owned)
    };
    plane.truncate(total);
    if mss.remaining_slice().is_some() {
        mss.seek(SeekFrom::Current(need as i64))
            .map_err(WavError::packet_io)?;
    }
    Ok(vec![plane])
}

pub(super) fn pull_gsm<F>(
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
    let n_blocks = total.div_ceil(MS_SAMPLES);
    let need = n_blocks.saturating_mul(MS_BLOCK);
    let rate = plan.sample_rate;
    let mut pull = GsmPull {
        dec: GsmDecoder::new(),
        pcm: [0i16; MS_SAMPLES],
        scratch: [0.0f32; MS_SAMPLES],
    };

    if let Some(rest) = mss.remaining_slice() {
        if rest.len() < need {
            return Err(pcm_short("wav: short gsm data"));
        }
        let mut decoded = 0usize;
        let n = pull.emit(&rest[..need], total, &mut decoded, rate, on_block)?;
        mss.seek(SeekFrom::Current(need as i64))
            .map_err(WavError::packet_io)?;
        return Ok(n);
    }

    let block_n = scratch_frames(MS_BLOCK, n_blocks).max(1);
    let mut raw = scrub_vec(vec![0u8; block_n * MS_BLOCK]);
    let mut decoded = 0usize;
    let mut left = n_blocks;
    while left > 0 {
        let this = left.min(block_n);
        let want = this * MS_BLOCK;
        mss.read_buf_exact(&mut raw[..want])
            .map_err(WavError::packet_io)?;
        pull.emit(&raw[..want], total, &mut decoded, rate, on_block)?;
        left -= this;
    }
    Ok(decoded)
}

struct GsmPull {
    dec: GsmDecoder,
    pcm: [i16; MS_SAMPLES],
    scratch: [f32; MS_SAMPLES],
}

impl GsmPull {
    fn emit<F>(
        &mut self,
        raw: &[u8],
        total: usize,
        decoded: &mut usize,
        rate: u32,
        on_block: &mut F,
    ) -> Result<usize>
    where
        F: FnMut(super::StreamBlock<'_>) -> Result<()>,
    {
        let mut got = 0usize;
        for chunk in raw.as_chunks::<MS_BLOCK>().0 {
            if *decoded >= total {
                break;
            }
            self.dec.decode_ms_block(chunk, &mut self.pcm);
            let n = MS_SAMPLES.min(total - *decoded);
            gsm::scale_i16(&mut self.scratch[..n], &self.pcm[..n]);
            emit_mono_block(rate, &self.scratch[..n], on_block)?;
            *decoded += n;
            got += n;
        }
        Ok(got)
    }
}
