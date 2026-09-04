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

const ALAW_SEG_END: [i32; 8] = [0xFF, 0x1FF, 0x3FF, 0x7FF, 0xFFF, 0x1FFF, 0x3FFF, 0x7FFF];
/// 16-bit biased endpoints (Sun 14-bit `seg_uend` does not match `ulaw2linear`).
const ULAW_SEG_END: [i32; 8] = [0xFC, 0x1F8, 0x3F0, 0x7E0, 0xFC0, 0x1F80, 0x3F00, 0x7E00];

fn search_seg(val: i32, ends: &[i32; 8]) -> usize {
    ends.iter().position(|&e| val <= e).unwrap_or(8)
}

/// Sun `linear2alaw`.
#[must_use]
pub(crate) fn linear_to_alaw(pcm_val: i32) -> u8 {
    let (mask, mag) = if pcm_val >= 0 {
        (0xD5u8, pcm_val)
    } else {
        (0x55u8, -pcm_val - 8)
    };
    let seg = search_seg(mag, &ALAW_SEG_END);
    if seg >= 8 {
        0x7F ^ mask
    } else {
        let quant = if seg < 2 {
            (mag >> 4) as u8
        } else {
            (mag >> (seg + 3)) as u8
        };
        (((seg as u8) << 4) | (quant & 0x0F)) ^ mask
    }
}

/// Sun `linear2ulaw`.
#[must_use]
pub(crate) fn linear_to_ulaw(pcm_val: i32) -> u8 {
    const BIAS: i32 = 0x84;
    const CLIP: i32 = 32635;
    let (mask, mut mag) = if pcm_val < 0 {
        (0x7Fu8, BIAS - pcm_val)
    } else {
        (0xFFu8, pcm_val + BIAS)
    };
    if mag > CLIP {
        mag = CLIP;
    }
    let seg = search_seg(mag, &ULAW_SEG_END);
    if seg >= 8 {
        0x7F ^ mask
    } else {
        let uval = ((seg as u8) << 4) | (((mag >> (seg + 3)) as u8) & 0x0F);
        uval ^ mask
    }
}

/// Interleaved little-endian i16 to G.711 bytes.
pub(crate) fn s16le_to_g711(pcm: &[u8], alaw: bool) -> crate::Result<Vec<u8>> {
    if !pcm.len().is_multiple_of(2) {
        return Err(crate::WavError::OddPcm);
    }
    let mut out = Vec::with_capacity(pcm.len() / 2);
    for s in pcm.as_chunks::<2>().0 {
        let v = i32::from(i16::from_le_bytes(*s));
        out.push(if alaw {
            linear_to_alaw(v)
        } else {
            linear_to_ulaw(v)
        });
    }
    Ok(out)
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
