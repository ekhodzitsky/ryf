//! GSM 06.10 RPE-LTP decoder, Microsoft WAV packing (65-byte / 320 samples).

use super::tables::{DEQUANT, LONG_TERM_GAIN};

pub(crate) const MS_BLOCK: usize = 65;
const FRAME_SAMPLES: usize = 160;
pub(crate) const MS_SAMPLES: usize = 320;

struct Bits<'a> {
    data: &'a [u8],
    cache: u32,
    cached: u32,
    idx: usize,
}

impl<'a> Bits<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            cache: 0,
            cached: 0,
            idx: 0,
        }
    }

    /// Little-endian `get_bits` (ffmpeg `BITSTREAM_READER_LE`).
    fn get(&mut self, n: u32) -> i32 {
        debug_assert!((1..=16).contains(&n));
        while self.cached < n {
            let b = self.data.get(self.idx).copied().unwrap_or(0);
            self.idx += 1;
            self.cache |= u32::from(b) << self.cached;
            self.cached += 8;
        }
        let v = self.cache & ((1u32 << n) - 1);
        self.cache >>= n;
        self.cached -= n;
        v as i32
    }
}

#[inline]
fn gsm_mult(a: i32, b: i32) -> i32 {
    // ffmpeg: `(int)(a * (SUINT)b + (1 << 14)) >> 15`
    (a.wrapping_mul(b).wrapping_add(1 << 14)) >> 15
}

#[inline]
fn clip_i16(v: i32) -> i32 {
    v.clamp(i16::MIN as i32, i16::MAX as i32)
}

#[inline]
fn decode_log_area(coded: i32, factor: i32, offset: i32) -> i32 {
    gsm_mult((coded << 10) - offset, factor) * 2
}

fn get_rrp(filtered: i32) -> i32 {
    let mut abs = filtered.unsigned_abs() as i32;
    if abs < 11059 {
        abs <<= 1;
    } else if abs < 20070 {
        abs += 11059;
    } else {
        abs = (abs >> 2) + 26112;
    }
    if filtered < 0 { -abs } else { abs }
}

fn filter_value(mut input: i32, rrp: &[i32; 8], v: &mut [i32; 9]) -> i32 {
    for i in (0..8).rev() {
        input -= gsm_mult(rrp[i], v[i]);
        v[i + 1] = v[i] + gsm_mult(rrp[i], input);
    }
    v[0] = input;
    input
}

/// Stateful GSM 06.10 decoder (MS-GSM / wav49).
pub(crate) struct GsmDecoder {
    ref_buf: [i16; 280],
    lar: [[i32; 8]; 2],
    lar_idx: usize,
    v: [i32; 9],
    msr: i32,
}

impl GsmDecoder {
    pub(crate) fn new() -> Self {
        Self {
            ref_buf: [0; 280],
            lar: [[0; 8]; 2],
            lar_idx: 0,
            v: [0; 9],
            msr: 0,
        }
    }

    /// Decode one 65-byte MS-GSM block to 320 PCM16 samples.
    pub(crate) fn decode_ms_block(&mut self, buf: &[u8; MS_BLOCK], out: &mut [i16; MS_SAMPLES]) {
        let mut bits = Bits::new(buf);
        let (a, b) = out.split_at_mut(FRAME_SAMPLES);
        self.decode_frame(&mut bits, a);
        self.decode_frame(&mut bits, b);
    }

    fn decode_frame(&mut self, bits: &mut Bits<'_>, samples: &mut [i16]) {
        debug_assert_eq!(samples.len(), FRAME_SAMPLES);
        let lar = &mut self.lar[self.lar_idx];
        lar[0] = decode_log_area(bits.get(6), 13107, 1 << 15);
        lar[1] = decode_log_area(bits.get(6), 13107, 1 << 15);
        lar[2] = decode_log_area(bits.get(5), 13107, (1 << 14) + 2048 * 2);
        lar[3] = decode_log_area(bits.get(5), 13107, (1 << 14) - 2560 * 2);
        lar[4] = decode_log_area(bits.get(4), 19223, (1 << 13) + 94 * 2);
        lar[5] = decode_log_area(bits.get(4), 17476, (1 << 13) - 1792 * 2);
        lar[6] = decode_log_area(bits.get(3), 31454, (1 << 12) - 341 * 2);
        lar[7] = decode_log_area(bits.get(3), 29708, (1 << 12) - 1144 * 2);

        let mut ref_dst = 120usize;
        for _ in 0..4 {
            let lag = bits.get(7).clamp(40, 120) as usize;
            let gain = LONG_TERM_GAIN[bits.get(2).clamp(0, 3) as usize];
            let offset = bits.get(2).clamp(0, 3) as usize;
            for i in 0..40 {
                let s = i32::from(self.ref_buf[ref_dst + i - lag]);
                self.ref_buf[ref_dst + i] = gsm_mult(gain, s) as i16;
            }
            // Pulses at offset + 3k stay inside this 40-sample subframe.
            apcm_dequant_add(bits, &mut self.ref_buf[ref_dst + offset..ref_dst + 40]);
            ref_dst += 40;
        }
        self.ref_buf.copy_within(160..280, 0);
        self.short_term_synth(samples);
        self.msr = postprocess(samples, self.msr);
    }

    fn short_term_synth(&mut self, dst: &mut [i16]) {
        let lar = self.lar[self.lar_idx];
        let prev = self.lar[self.lar_idx ^ 1];
        let src = &self.ref_buf[120..280];
        let mut rrp = [0i32; 8];
        for (i, slot) in rrp.iter_mut().enumerate() {
            *slot = get_rrp((prev[i] >> 2) + (prev[i] >> 1) + (lar[i] >> 2));
        }
        synth_range(dst, src, &rrp, &mut self.v, 0, 13);
        for (i, slot) in rrp.iter_mut().enumerate() {
            *slot = get_rrp((prev[i] >> 1) + (lar[i] >> 1));
        }
        synth_range(dst, src, &rrp, &mut self.v, 13, 27);
        for (i, slot) in rrp.iter_mut().enumerate() {
            *slot = get_rrp((prev[i] >> 2) + (lar[i] >> 1) + (lar[i] >> 2));
        }
        synth_range(dst, src, &rrp, &mut self.v, 27, 40);
        for (i, slot) in rrp.iter_mut().enumerate() {
            *slot = get_rrp(lar[i]);
        }
        synth_range(dst, src, &rrp, &mut self.v, 40, 160);
        self.lar_idx ^= 1;
    }
}

fn synth_range(
    dst: &mut [i16],
    src: &[i16],
    rrp: &[i32; 8],
    v: &mut [i32; 9],
    start: usize,
    end: usize,
) {
    for (d, &s) in dst[start..end].iter_mut().zip(src[start..end].iter()) {
        *d = filter_value(i32::from(s), rrp, v) as i16;
    }
}

fn apcm_dequant_add(bits: &mut Bits<'_>, dst: &mut [i16]) {
    let maxidx = bits.get(6).clamp(0, 63) as usize;
    let tab = &DEQUANT[maxidx];
    // 13 kbit/s: every RPE pulse is 3 bits (wav49 / GSM_13000).
    for p in (0..13).map(|i| 3 * i) {
        let val = bits.get(3).clamp(0, 7) as usize;
        debug_assert!(p < dst.len());
        dst[p] = (i32::from(dst[p]) + i32::from(tab[val])) as i16;
    }
}

fn postprocess(data: &mut [i16], mut msr: i32) -> i32 {
    for s in data.iter_mut() {
        msr = clip_i16(i32::from(*s) + gsm_mult(msr, 28180));
        *s = (clip_i16(msr * 2) & !7) as i16;
    }
    msr
}
