//! 64 kbit/s G.722 decoder (ITU / FFmpeg `g722dec`, unpacked 8-bit codes).

use super::tables::{
    HIGH_INV_QUANT, HIGH_LOG_FACTOR_STEP, INV_LOG2, LOW_INV_QUANT4, LOW_INV_QUANT6,
    LOW_LOG_FACTOR_STEP, QMF_EVEN, QMF_ODD,
};

#[inline]
fn clip_i16(v: i32) -> i32 {
    v.clamp(i16::MIN as i32, i16::MAX as i32)
}

#[inline]
fn clip14(v: i32) -> i32 {
    v.clamp(-16384, 16383)
}

#[inline]
fn linear_scale_factor(log_factor: i32) -> i32 {
    let wd1 = INV_LOG2[((log_factor >> 6) & 31) as usize];
    let shift = log_factor >> 11;
    if shift < 0 {
        wd1 >> (-shift)
    } else {
        wd1 << shift
    }
}

#[derive(Clone, Copy, Default)]
struct Band {
    s_predictor: i32,
    s_zero: i32,
    scale_factor: i32,
    log_factor: i32,
    pole_mem: [i32; 2],
    zero_mem: [i32; 6],
    diff_mem: [i32; 6],
    part_reconst_mem: [i32; 2],
    prev_qtzd_reconst: i32,
}

impl Band {
    fn low() -> Self {
        Self {
            scale_factor: 8,
            ..Self::default()
        }
    }

    fn high() -> Self {
        Self {
            scale_factor: 2,
            ..Self::default()
        }
    }
}

fn s_zero(band: &mut Band, cur_diff: i32) {
    let d = i32::from(cur_diff != 0);
    let xs = [
        band.diff_mem[4],
        band.diff_mem[3],
        band.diff_mem[2],
        band.diff_mem[1],
        band.diff_mem[0],
        cur_diff.wrapping_mul(2),
    ];
    let mut acc = 0i32;
    for (i, x) in xs.into_iter().enumerate() {
        let k = 5 - i;
        let sign = if (band.diff_mem[k] ^ cur_diff) < 0 {
            -128
        } else {
            128
        };
        band.zero_mem[k] = ((band.zero_mem[k] * 255) >> 8) + d * sign;
        band.diff_mem[k] = x;
        acc += (x * band.zero_mem[k]) >> 15;
    }
    band.s_zero = acc;
}

fn adaptive_prediction(band: &mut Band, cur_diff: i32) {
    let cur_part = i32::from(band.s_zero + cur_diff < 0);
    let sg0 = if cur_part != band.part_reconst_mem[0] {
        1
    } else {
        -1
    };
    let sg1 = if cur_part == band.part_reconst_mem[1] {
        1
    } else {
        -1
    };
    band.part_reconst_mem[1] = band.part_reconst_mem[0];
    band.part_reconst_mem[0] = cur_part;

    let a1 = (sg0 * band.pole_mem[0].clamp(-8191, 8191)) >> 5;
    band.pole_mem[1] = (a1 + sg1 * 128 + ((band.pole_mem[1] * 127) >> 7)).clamp(-12288, 12288);

    let limit = 15360 - band.pole_mem[1];
    band.pole_mem[0] = (-192 * sg0 + ((band.pole_mem[0] * 255) >> 8)).clamp(-limit, limit);

    s_zero(band, cur_diff);

    let cur_qtzd = clip_i16((band.s_predictor + cur_diff) * 2);
    band.s_predictor = clip_i16(
        band.s_zero
            + ((band.pole_mem[0] * cur_qtzd) >> 15)
            + ((band.pole_mem[1] * band.prev_qtzd_reconst) >> 15),
    );
    band.prev_qtzd_reconst = cur_qtzd;
}

fn update_low(band: &mut Band, ilow: i32) {
    let idx = ilow as usize;
    adaptive_prediction(band, (band.scale_factor * LOW_INV_QUANT4[idx]) >> 10);
    band.log_factor = (((band.log_factor * 127) >> 7) + LOW_LOG_FACTOR_STEP[idx]).clamp(0, 18432);
    band.scale_factor = linear_scale_factor(band.log_factor - (8 << 11));
}

fn update_high(band: &mut Band, dhigh: i32, ihigh: i32) {
    adaptive_prediction(band, dhigh);
    band.log_factor = (((band.log_factor * 127) >> 7) + HIGH_LOG_FACTOR_STEP[(ihigh & 1) as usize])
        .clamp(0, 22528);
    band.scale_factor = linear_scale_factor(band.log_factor - (10 << 11));
}

/// One G.722 64 kbit/s channel (2 PCM samples per encoded byte).
#[derive(Clone, Copy)]
pub(crate) struct G722Decoder {
    low: Band,
    high: Band,
    qmf: [i32; 24],
}

impl G722Decoder {
    pub(crate) fn new() -> Self {
        Self {
            low: Band::low(),
            high: Band::high(),
            qmf: [0; 24],
        }
    }

    #[inline]
    pub(crate) fn decode_byte(&mut self, code: u8) -> [i16; 2] {
        let ihigh = i32::from(code >> 6) & 3;
        let ilow = i32::from(code) & 0x3f;

        let rlow = clip14(
            ((self.low.scale_factor * LOW_INV_QUANT6[ilow as usize]) >> 10) + self.low.s_predictor,
        );
        update_low(&mut self.low, ilow >> 2);

        let dhigh = (self.high.scale_factor * HIGH_INV_QUANT[ihigh as usize]) >> 10;
        let rhigh = clip14(dhigh + self.high.s_predictor);
        update_high(&mut self.high, dhigh, ihigh);

        self.qmf.copy_within(2..24, 0);
        self.qmf[22] = rlow + rhigh;
        self.qmf[23] = rlow - rhigh;

        let mut xout0 = 0i32;
        let mut xout1 = 0i32;
        for i in 0..12 {
            xout1 += self.qmf[i * 2] * QMF_EVEN[i];
            xout0 += self.qmf[i * 2 + 1] * QMF_ODD[i];
        }
        [clip_i16(xout0 >> 11) as i16, clip_i16(xout1 >> 11) as i16]
    }
}
