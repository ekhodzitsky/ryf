//! ITU-T G.726 ADPCM decoder (16/24/32/40 kbit/s).
//!
//! Arithmetic follows section 4 of the recommendation: 11-bit float
//! multiply, adaptive quantizer scale, and the second-order predictor.
//! Output is 16-bit linear PCM (`reconstructed * 4`, low 16 bits).

use super::tables::{self, RateTables};
use super::unpack::Unpacker;
use super::{G726Packing, G726Rate};

#[derive(Clone, Copy, Debug)]
struct Float11 {
    sign: u8,
    exp: u8,
    mant: u8,
}

impl Float11 {
    const fn idle() -> Self {
        Self {
            sign: 0,
            exp: 0,
            mant: 1 << 5,
        }
    }
}

fn i2f(mag_signed: i32) -> Float11 {
    let sign = u8::from(mag_signed < 0);
    let mag = if mag_signed < 0 {
        mag_signed.wrapping_neg()
    } else {
        mag_signed
    };
    if mag <= 0 {
        return Float11 {
            sign,
            exp: 0,
            mant: 1 << 5,
        };
    }
    let mag_u = mag as u32;
    let exp = (31 - mag_u.leading_zeros()) as u8 + 1;
    let mant = ((u64::from(mag_u) << 6) >> u32::from(exp)) as u8;
    Float11 { sign, exp, mant }
}

fn mult(a: Float11, b: Float11) -> i32 {
    let exp = i32::from(a.exp) + i32::from(b.exp);
    let mut res = (i32::from(a.mant) * i32::from(b.mant) + 0x30) >> 4;
    if exp > 19 {
        let sh = exp - 19;
        res = if sh >= 31 {
            0
        } else {
            res.wrapping_shl(sh as u32)
        };
    } else {
        let sh = 19 - exp;
        res = if sh >= 31 { 0 } else { res >> sh };
    }
    let signed = if (a.sign ^ b.sign) != 0 {
        res.wrapping_neg()
    } else {
        res
    };
    // The multiply is a 16-bit result. Wider products keep only the low half.
    i32::from(signed as i16)
}

fn sgn(v: i32) -> i32 {
    if v < 0 { -1 } else { 1 }
}

fn clip(v: i32, lo: i32, hi: i32) -> i32 {
    v.clamp(lo, hi)
}

/// `av_clip_intp2(v, 8)`: -256..=255.
fn clip_p8(v: i32) -> i32 {
    v.clamp(-256, 255)
}

fn inverse_quant(y: i32, iquant: i16) -> i32 {
    let dql = i32::from(iquant) + (y >> 2);
    if dql < 0 {
        return 0;
    }
    let dex = (dql >> 7) & 0xf;
    let dqt = (1 << 7) + (dql & 0x7f);
    (dqt << dex) >> 7
}

#[derive(Clone, Debug)]
struct Core {
    bits: u8,
    sr: [Float11; 2],
    dq: [Float11; 6],
    a: [i32; 2],
    b: [i32; 6],
    pk: [i32; 2],
    ap: i32,
    yu: i32,
    yl: i32,
    dms: i32,
    dml: i32,
    td: i32,
    se: i32,
    sez: i32,
    y: i32,
}

impl Core {
    fn new(bits: u8) -> Self {
        Self {
            bits,
            sr: [Float11::idle(); 2],
            dq: [Float11::idle(); 6],
            a: [0; 2],
            b: [0; 6],
            pk: [1; 2],
            ap: 0,
            yu: 544,
            yl: 34816,
            dms: 0,
            dml: 0,
            td: 0,
            se: 0,
            sez: 0,
            y: 544,
        }
    }

    fn step(&mut self, code: u8) -> i16 {
        let tbl = tables::tables(self.bits);
        let code_i = i32::from(code);
        let Some(&iquant) = tbl.iquant.get(code as usize) else {
            return 0;
        };
        let i_sig = code_i >> (self.bits - 1);
        let mut dq = inverse_quant(self.y, iquant);

        let ylint = self.yl >> 15;
        let ylfrac = (self.yl >> 10) & 0x1f;
        let thr2 = if ylint > 9 {
            0x1f << 10
        } else if ylint < 0 {
            0
        } else {
            (0x20 + ylfrac) << ylint
        };
        let tr = self.td == 1 && dq > ((3 * thr2) >> 2);
        #[cfg(test)]
        tone_note(tr, ylint, self.yl);

        if i_sig != 0 {
            dq = dq.wrapping_neg();
        }
        let re_signal = self.se.wrapping_add(dq) as i16 as i32;

        let sum = self.sez.wrapping_add(dq);
        let pk0 = if sum == 0 { 0 } else { sgn(sum) };
        let dq0 = if dq == 0 { 0 } else { sgn(dq) };

        if tr {
            self.a = [0; 2];
            self.b = [0; 6];
        } else {
            self.adapt_predictor(pk0, dq0);
        }

        self.pk[1] = self.pk[0];
        self.pk[0] = if pk0 == 0 { 1 } else { pk0 };
        self.sr[1] = self.sr[0];
        self.sr[0] = i2f(re_signal);
        for i in (1..6).rev() {
            self.dq[i] = self.dq[i - 1];
        }
        self.dq[0] = i2f(dq);
        self.dq[0].sign = u8::from(i_sig != 0);

        self.td = i32::from(self.a[1] < -11776);
        self.adapt_scale(code_i, &tbl, tr);

        let scaled = re_signal.wrapping_mul(4).clamp(-0xffff, 0xffff);
        scaled as i16
    }

    fn adapt_predictor(&mut self, pk0: i32, dq0: i32) {
        // Pole update uses -a1 * sgn(pk0) * sgn(pk1), then a 9-bit clip.
        let fa1 = clip_p8(
            self.a[0]
                .wrapping_neg()
                .wrapping_mul(self.pk[0])
                .wrapping_mul(pk0)
                >> 5,
        );
        self.a[1] = clip(
            self.a[1]
                .wrapping_add(pk0.wrapping_mul(self.pk[1]).wrapping_mul(128))
                .wrapping_add(fa1)
                .wrapping_sub(self.a[1] >> 7),
            -12288,
            12288,
        );
        let limit = 15360 - self.a[1];
        self.a[0] = clip(
            self.a[0]
                .wrapping_add(pk0.wrapping_mul(self.pk[0]).wrapping_mul(192))
                .wrapping_sub(self.a[0] >> 8),
            -limit,
            limit,
        );
        for i in 0..6 {
            let sign = sgn(-i32::from(self.dq[i].sign));
            self.b[i] = self.b[i]
                .wrapping_add(dq0.wrapping_mul(sign).wrapping_mul(128))
                .wrapping_sub(self.b[i] >> 8);
        }
    }

    fn adapt_scale(&mut self, code: i32, tbl: &RateTables, tr: bool) {
        let f = i32::from(tbl.f.get(code as usize).copied().unwrap_or(0));
        let w = i32::from(tbl.w.get(code as usize).copied().unwrap_or(0));
        self.dms = self
            .dms
            .wrapping_add(f << 4)
            .wrapping_add(self.dms.wrapping_neg() >> 5);
        self.dml = self
            .dml
            .wrapping_add(f << 4)
            .wrapping_add(self.dml.wrapping_neg() >> 7);
        if tr {
            self.ap = 256;
        } else {
            self.ap = self.ap.wrapping_add(self.ap.wrapping_neg() >> 4);
            let delta = i64::from(self.dms.wrapping_shl(2)) - i64::from(self.dml);
            let thr = i64::from(self.dml >> 3);
            if self.y <= 1535 || self.td != 0 || delta.abs() >= thr {
                self.ap = self.ap.wrapping_add(0x20);
            }
        }
        self.yu = clip(
            self.y
                .wrapping_add(w)
                .wrapping_add(self.y.wrapping_neg() >> 5),
            544,
            5120,
        );
        self.yl = self
            .yl
            .wrapping_add(self.yu)
            .wrapping_add(self.yl.wrapping_neg() >> 6);
        let al = if self.ap >= 256 { 1 << 6 } else { self.ap >> 2 };
        let mixed = self.yu.wrapping_sub(self.yl >> 6).wrapping_mul(al);
        self.y = self.yl.wrapping_add(mixed) >> 6;

        self.se = 0;
        for i in 0..6 {
            self.se = self.se.wrapping_add(mult(i2f(self.b[i] >> 2), self.dq[i]));
        }
        self.sez = self.se >> 1;
        for i in 0..2 {
            self.se = self.se.wrapping_add(mult(i2f(self.a[i] >> 2), self.sr[i]));
        }
        self.se >>= 1;
    }
}

/// Incremental G.726 decoder.
///
/// Holds the predictor and at most 7 unused input bits. `push` writes into
/// the caller buffer and does not allocate. `finish` drops a partial
/// codeword. `reset` restores the initial predictor and an empty bit buffer.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct G726Decoder {
    core: Core,
    unpack: Unpacker,
    rate: G726Rate,
    packing: G726Packing,
}

impl G726Decoder {
    pub fn new(rate: G726Rate, packing: G726Packing) -> Self {
        Self {
            core: Core::new(rate.bits()),
            unpack: Unpacker::new(rate.bits(), packing),
            rate,
            packing,
        }
    }

    pub fn rate(&self) -> G726Rate {
        self.rate
    }

    pub fn packing(&self) -> G726Packing {
        self.packing
    }

    /// Unused input bits (0..=7). When the output buffer fills, these may
    /// include complete codewords; drain them with `push(&[], out)`.
    pub fn pending_bits(&self) -> u8 {
        self.unpack.pending_bits()
    }

    /// Decode complete codewords from `input` into `out`.
    ///
    /// Returns `(samples_written, bytes_consumed)`. An empty `out` consumes
    /// nothing. Bytes past `bytes_consumed` stay with the caller. A codeword
    /// that spans the chunk boundary stays in [`Self::pending_bits`].
    pub fn push(&mut self, input: &[u8], out: &mut [i16]) -> (usize, usize) {
        let mut produced = 0usize;
        let mut consumed = 0usize;
        while produced < out.len() {
            if let Some(code) = self.unpack.try_take() {
                out[produced] = self.core.step(code);
                produced += 1;
                continue;
            }
            if consumed >= input.len() {
                break;
            }
            self.unpack.push_byte(input[consumed]);
            consumed += 1;
        }
        (produced, consumed)
    }

    /// Discard all pending bits. Drain complete codewords with `push(&[], out)`
    /// first, or they are discarded too. The predictor is unchanged.
    /// Returns how many bits were discarded.
    pub fn finish(&mut self) -> u8 {
        let n = self.unpack.pending_bits();
        self.unpack.clear();
        n
    }

    /// Initial predictor and an empty bit buffer.
    pub fn reset(&mut self) {
        *self = Self::new(self.rate, self.packing);
    }
}

// Test counters, in order: tr hits, ylint>9 hits, max yl, first tr
// sample, first ylint>9 sample, samples seen. Indexes are -1 if unset.
#[cfg(test)]
thread_local! {
    static TONE: std::cell::Cell<(u32, u32, i32, i32, i32, i32)> =
        const { std::cell::Cell::new((0, 0, i32::MIN, -1, -1, 0)) };
}

#[cfg(test)]
pub(super) fn tone_counters_reset() {
    TONE.with(|c| c.set((0, 0, i32::MIN, -1, -1, 0)));
}

#[cfg(test)]
pub(super) fn tone_counters() -> (u32, u32, i32, i32, i32, i32) {
    TONE.with(|c| c.get())
}

#[cfg(test)]
fn tone_note(tr: bool, ylint: i32, yl: i32) {
    TONE.with(|c| {
        let (mut ntr, mut nhi, mut ymax, mut first_tr, mut first_hi, n) = c.get();
        if tr {
            if ntr == 0 {
                first_tr = n;
            }
            ntr += 1;
        }
        if ylint > 9 {
            if nhi == 0 {
                first_hi = n;
            }
            nhi += 1;
        }
        if yl > ymax {
            ymax = yl;
        }
        c.set((ntr, nhi, ymax, first_tr, first_hi, n + 1));
    });
}
