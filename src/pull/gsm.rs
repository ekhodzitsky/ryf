//! Microsoft GSM 06.10 pull / collect (65-byte blocks to 320 PCM frames).

use super::{DecodePlan, check_duration, emit_mono_block};
use crate::error::Result;
use crate::gsm::{self, GsmDecoder, MS_BLOCK, MS_SAMPLES};
use crate::source::ByteSource;

pub(super) fn collect_gsm(mss: &mut ByteSource<'_>, plan: &DecodePlan) -> Result<Vec<Vec<f32>>> {
    check_duration(plan.total_frames, plan.max_samples, plan.sample_rate)?;
    let mut out = vec![0.0; plan.total_frames];
    let mut decoder = GsmDecoder::new();
    let mut pcm = [0i16; MS_SAMPLES];
    let mut offset = 0;
    super::input::for_each_input(
        mss,
        plan.total_frames.div_ceil(MS_SAMPLES),
        MS_BLOCK,
        usize::MAX,
        plan.sample_rate,
        |bytes, _| {
            for block in bytes.as_chunks::<MS_BLOCK>().0 {
                decoder.decode_ms_block(block, &mut pcm);
                let n = MS_SAMPLES.min(plan.total_frames - offset);
                gsm::scale_i16(&mut out[offset..offset + n], &pcm[..n]);
                offset += n;
            }
            Ok(())
        },
    )?;
    Ok(vec![out])
}
pub(super) fn pull_gsm<F>(
    mss: &mut ByteSource<'_>,
    plan: &DecodePlan,
    on_block: &mut F,
) -> Result<usize>
where
    F: FnMut(super::StreamBlock<'_>) -> Result<()>,
{
    check_duration(plan.total_frames, plan.max_samples, plan.sample_rate)?;
    let mut pull = GsmPull {
        dec: GsmDecoder::new(),
        pcm: [0; MS_SAMPLES],
        scratch: [0.0; MS_SAMPLES],
    };
    let mut decoded = 0;
    super::input::for_each_input(
        mss,
        plan.total_frames.div_ceil(MS_SAMPLES),
        MS_BLOCK,
        usize::MAX,
        plan.sample_rate,
        |raw, _| {
            pull.emit(
                raw,
                plan.total_frames,
                &mut decoded,
                plan.sample_rate,
                on_block,
            )?;
            Ok(())
        },
    )?;
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
