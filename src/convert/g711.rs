//! G.711 A-law / mu-law: 256-entry tables + bulk convert.
//!
//! Tables are filled from the historical Sun `g711.c` expanders so every
//! code stays bit-exact with [`super::convert_sample`].

const XLAW_QUANT_MASK: u8 = 0x0f;
const XLAW_SEG_MASK: u8 = 0x70;
const XLAW_SEG_SHIFT: u32 = 4;

/// Sun Microsystems `alaw2linear` (unrestricted use).
#[must_use]
pub(crate) const fn alaw_to_linear(mut a_val: u8) -> i16 {
    a_val ^= 0x55;

    let mut t = ((a_val & XLAW_QUANT_MASK) << 4) as i16;
    let seg = (a_val & XLAW_SEG_MASK) >> XLAW_SEG_SHIFT;

    match seg {
        0 => t += 0x8,
        1 => t += 0x108,
        _ => t = (t + 0x108) << (seg - 1),
    }

    if a_val & 0x80 == 0x80 { t } else { -t }
}

/// Sun Microsystems `ulaw2linear` (unrestricted use).
#[must_use]
pub(crate) const fn mulaw_to_linear(mut mu_val: u8) -> i16 {
    const BIAS: i16 = 0x84;
    mu_val = !mu_val;
    let mut t = ((mu_val & XLAW_QUANT_MASK) << 3) as i16 + BIAS;
    t <<= (mu_val & XLAW_SEG_MASK) >> XLAW_SEG_SHIFT;
    if mu_val & 0x80 == 0x80 {
        BIAS - t
    } else {
        t - BIAS
    }
}

const ALAW_I16: [i16; 256] = {
    let mut t = [0i16; 256];
    let mut i = 0;
    while i < 256 {
        t[i] = alaw_to_linear(i as u8);
        i += 1;
    }
    t
};

const MULAW_I16: [i16; 256] = {
    let mut t = [0i16; 256];
    let mut i = 0;
    while i < 256 {
        t[i] = mulaw_to_linear(i as u8);
        i += 1;
    }
    t
};

pub(crate) static ALAW_F32: [f32; 256] = {
    let mut t = [0.0f32; 256];
    let mut i = 0;
    while i < 256 {
        t[i] = ALAW_I16[i] as f32 * super::I16_SCALE;
        i += 1;
    }
    t
};

pub(crate) static MULAW_F32: [f32; 256] = {
    let mut t = [0.0f32; 256];
    let mut i = 0;
    while i < 256 {
        t[i] = MULAW_I16[i] as f32 * super::I16_SCALE;
        i += 1;
    }
    t
};

#[inline]
pub(crate) fn table_for(alaw: bool) -> &'static [f32; 256] {
    if alaw { &ALAW_F32 } else { &MULAW_F32 }
}

#[inline]
pub(crate) fn lut(table: &[f32; 256], b: u8) -> f32 {
    // SAFETY: `b` is u8, table length is 256.
    unsafe { *table.get_unchecked(b as usize) }
}

/// One byte per sample to planar mono f32.
#[inline]
pub(crate) fn convert_mono(src: &[u8], dst: &mut [f32], table: &[f32; 256]) {
    debug_assert_eq!(src.len(), dst.len());
    let n = dst.len();
    let mut i = 0;
    while i + 8 <= n {
        dst[i] = lut(table, src[i]);
        dst[i + 1] = lut(table, src[i + 1]);
        dst[i + 2] = lut(table, src[i + 2]);
        dst[i + 3] = lut(table, src[i + 3]);
        dst[i + 4] = lut(table, src[i + 4]);
        dst[i + 5] = lut(table, src[i + 5]);
        dst[i + 6] = lut(table, src[i + 6]);
        dst[i + 7] = lut(table, src[i + 7]);
        i += 8;
    }
    while i < n {
        dst[i] = lut(table, src[i]);
        i += 1;
    }
}

/// Interleaved G.711 to mixed mono (sum / n).
#[inline]
pub(crate) fn mix(src: &[u8], dst: &mut [f32], channels: usize, table: &[f32; 256]) {
    debug_assert!(channels >= 2);
    debug_assert_eq!(src.len(), dst.len() * channels);
    let n = channels as f32;
    for (frame, out) in src.chunks_exact(channels).zip(dst.iter_mut()) {
        let mut sum = 0.0f32;
        for &b in frame {
            sum += lut(table, b);
        }
        *out = sum / n;
    }
}

/// Interleaved G.711 to one plane per channel.
#[inline]
pub(crate) fn split(src: &[u8], dst: &mut [&mut [f32]], table: &[f32; 256]) {
    let channels = dst.len();
    debug_assert!(channels >= 1);
    let frames = dst.first().map(|c| c.len()).unwrap_or(0);
    debug_assert!(dst.iter().all(|c| c.len() == frames));
    debug_assert_eq!(src.len(), frames * channels);
    for (fi, frame) in src.chunks_exact(channels).enumerate() {
        for (c, &b) in frame.iter().enumerate() {
            dst[c][fi] = lut(table, b);
        }
    }
}
