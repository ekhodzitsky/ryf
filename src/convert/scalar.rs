//! Scalar PCM convert kernels (s16 / f32 / n-channel mix-split).

#[inline]
#[cfg_attr(
    all(any(
        all(target_arch = "aarch64", not(miri)),
        all(any(target_arch = "x86_64", target_arch = "x86"), not(miri))
    )),
    allow(dead_code)
)]
pub(super) fn convert_s16_mono_scalar(src: &[u8], dst: &mut [f32]) {
    // Process 8 samples per iteration to help LLVM auto-vectorize.
    let n = dst.len();
    let mut i = 0;
    while i + 8 <= n {
        let base = i * 2;
        for j in 0..8 {
            let b = base + j * 2;
            let s = i16::from_le_bytes([src[b], src[b + 1]]);
            dst[i + j] = s as f32 / 32_768.0;
        }
        i += 8;
    }
    while i < n {
        let b = i * 2;
        let s = i16::from_le_bytes([src[b], src[b + 1]]);
        dst[i] = s as f32 / 32_768.0;
        i += 1;
    }
}

pub(super) fn mix_s16_nch_scalar(src: &[u8], dst: &mut [f32], channels: usize) {
    let n_ch = channels as f32;
    let frame_bytes = channels * 2;
    for (i, frame) in src.chunks_exact(frame_bytes).enumerate() {
        let mut sum = 0.0f32;
        for s in frame.as_chunks::<2>().0 {
            sum += i16::from_le_bytes(*s) as f32 / 32_768.0;
        }
        dst[i] = sum / n_ch;
    }
}

pub(super) fn split_s16_nch_scalar(src: &[u8], dst: &mut [&mut [f32]]) {
    let channels = dst.len();
    let frame_bytes = channels * 2;
    for (fi, frame) in src.chunks_exact(frame_bytes).enumerate() {
        for (c, s) in frame.as_chunks::<2>().0.iter().enumerate() {
            dst[c][fi] = i16::from_le_bytes(*s) as f32 / 32_768.0;
        }
    }
}

pub(super) fn mix_s16_stereo_scalar(src: &[u8], dst: &mut [f32]) {
    for (i, frame) in src.as_chunks::<4>().0.iter().enumerate() {
        let l = i16::from_le_bytes([frame[0], frame[1]]) as f32 / 32_768.0;
        let r = i16::from_le_bytes([frame[2], frame[3]]) as f32 / 32_768.0;
        dst[i] = (l + r) / 2.0;
    }
}

pub(super) fn split_s16_stereo_scalar(src: &[u8], left: &mut [f32], right: &mut [f32]) {
    debug_assert_eq!(left.len(), right.len());
    debug_assert_eq!(src.len(), left.len() * 4);
    for (i, frame) in src.as_chunks::<4>().0.iter().enumerate() {
        left[i] = i16::from_le_bytes([frame[0], frame[1]]) as f32 / 32_768.0;
        right[i] = i16::from_le_bytes([frame[2], frame[3]]) as f32 / 32_768.0;
    }
}

#[inline]
#[cfg_attr(
    all(
        feature = "simd",
        target_endian = "little",
        any(
            all(target_arch = "aarch64", not(miri)),
            all(any(target_arch = "x86_64", target_arch = "x86"), not(miri))
        )
    ),
    allow(dead_code)
)]
pub(super) fn convert_f32_mono_scalar(src: &[u8], dst: &mut [f32]) {
    let n = dst.len();
    let mut i = 0;
    while i + 4 <= n {
        let b = i * 4;
        dst[i] = f32::from_le_bytes([src[b], src[b + 1], src[b + 2], src[b + 3]]);
        dst[i + 1] = f32::from_le_bytes([src[b + 4], src[b + 5], src[b + 6], src[b + 7]]);
        dst[i + 2] = f32::from_le_bytes([src[b + 8], src[b + 9], src[b + 10], src[b + 11]]);
        dst[i + 3] = f32::from_le_bytes([src[b + 12], src[b + 13], src[b + 14], src[b + 15]]);
        i += 4;
    }
    while i < n {
        let b = i * 4;
        dst[i] = f32::from_le_bytes([src[b], src[b + 1], src[b + 2], src[b + 3]]);
        i += 1;
    }
}

// G.711 helpers (Sun Microsystems g711.c, unrestricted use).
const XLAW_QUANT_MASK: u8 = 0x0f;
const XLAW_SEG_MASK: u8 = 0x70;
const XLAW_SEG_SHIFT: u32 = 4;

pub(crate) fn alaw_to_linear(mut a_val: u8) -> i16 {
    a_val ^= 0x55;

    let mut t = i16::from((a_val & XLAW_QUANT_MASK) << 4);
    let seg = (a_val & XLAW_SEG_MASK) >> XLAW_SEG_SHIFT;

    match seg {
        0 => t += 0x8,
        1 => t += 0x108,
        _ => t = (t + 0x108) << (seg - 1),
    }

    if a_val & 0x80 == 0x80 { t } else { -t }
}

pub(crate) fn mulaw_to_linear(mut mu_val: u8) -> i16 {
    const BIAS: i16 = 0x84;

    // Complement to obtain normal u-law value.
    mu_val = !mu_val;

    // Extract and bias the quantization bits. Then shift up by the segment
    // number and subtract out the bias.
    let mut t = i16::from((mu_val & XLAW_QUANT_MASK) << 3) + BIAS;
    t <<= (mu_val & XLAW_SEG_MASK) >> XLAW_SEG_SHIFT;

    if mu_val & 0x80 == 0x80 {
        BIAS - t
    } else {
        t - BIAS
    }
}
