//! Optional SIMD s16→f32 kernels (NEON / SSE4.1 / SSE2).

use super::scalar::{mix_s16_stereo_scalar, split_s16_stereo_scalar};

/// x86 SSE4.1: 8× s16 LE → f32 via `_mm_cvtepi16_epi32` + `_mm_div_ps`.
///
/// # Safety
/// Caller must ensure SSE4.1 is available (runtime-checked).
#[cfg(all(
    feature = "simd",
    any(target_arch = "x86_64", target_arch = "x86"),
    not(miri)
))]
#[target_feature(enable = "sse4.1")]
pub(super) unsafe fn convert_s16_mono_sse41(src: &[u8], dst: &mut [f32]) {
    #[cfg(target_arch = "x86")]
    use std::arch::x86::*;
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::*;

    let n = dst.len();
    let mut i = 0;
    // SAFETY: bounds checked by loop condition; unaligned loads/stores.
    unsafe {
        let denom = _mm_set1_ps(32_768.0);
        while i + 8 <= n {
            let p = src.as_ptr().add(i * 2) as *const __m128i;
            let v = _mm_loadu_si128(p); // 8 × i16
            // SSE4.1 sign-extend: low 4 and high 4 lanes.
            let lo = _mm_cvtepi16_epi32(v);
            let hi = _mm_cvtepi16_epi32(_mm_srli_si128(v, 8));
            let flo = _mm_div_ps(_mm_cvtepi32_ps(lo), denom);
            let fhi = _mm_div_ps(_mm_cvtepi32_ps(hi), denom);
            let out = dst.as_mut_ptr().add(i);
            _mm_storeu_ps(out, flo);
            _mm_storeu_ps(out.add(4), fhi);
            i += 8;
        }
    }
    while i < n {
        let b = i * 2;
        let s = i16::from_le_bytes([src[b], src[b + 1]]);
        dst[i] = s as f32 / 32_768.0;
        i += 1;
    }
}

/// x86 SSE2: 8× s16 LE → f32 with true `/ 32768.0` (bit-exact with scalar).
/// Used when SSE4.1 is unavailable (older CPUs).
///
/// # Safety
/// Caller must ensure SSE2 is available (runtime-checked).
#[cfg(all(
    feature = "simd",
    any(target_arch = "x86_64", target_arch = "x86"),
    not(miri)
))]
#[target_feature(enable = "sse2")]
pub(super) unsafe fn convert_s16_mono_sse2(src: &[u8], dst: &mut [f32]) {
    #[cfg(target_arch = "x86")]
    use std::arch::x86::*;
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::*;

    let n = dst.len();
    let mut i = 0;
    // SAFETY: bounds checked by loop condition; unaligned loads/stores.
    unsafe {
        let denom = _mm_set1_ps(32_768.0);
        while i + 8 <= n {
            let p = src.as_ptr().add(i * 2) as *const __m128i;
            let v = _mm_loadu_si128(p); // 8 × i16
            // Sign-extend i16 → i32 (SSE2, no SSE4.1).
            let sign = _mm_srai_epi16(v, 15);
            let lo = _mm_unpacklo_epi16(v, sign);
            let hi = _mm_unpackhi_epi16(v, sign);
            let flo = _mm_div_ps(_mm_cvtepi32_ps(lo), denom);
            let fhi = _mm_div_ps(_mm_cvtepi32_ps(hi), denom);
            let out = dst.as_mut_ptr().add(i);
            _mm_storeu_ps(out, flo);
            _mm_storeu_ps(out.add(4), fhi);
            i += 8;
        }
    }
    while i < n {
        let b = i * 2;
        let s = i16::from_le_bytes([src[b], src[b + 1]]);
        dst[i] = s as f32 / 32_768.0;
        i += 1;
    }
}

/// aarch64 NEON: 8× s16 → f32 with true `/ 32768.0` (not reciprocal mul)
/// so output stays bit-exact with the scalar / ffmpeg path.
///
/// # Safety
/// Caller must ensure NEON is available (baseline on aarch64 product targets).
#[cfg(all(feature = "simd", target_arch = "aarch64", not(miri)))]
#[target_feature(enable = "neon")]
pub(super) unsafe fn convert_s16_mono_neon(src: &[u8], dst: &mut [f32]) {
    use std::arch::aarch64::*;
    let n = dst.len();
    let mut i = 0;
    // SAFETY: all NEON ops below operate on local vectors or on slices
    // already bounds-checked by `i + 8 <= n` / `i < n`.
    unsafe {
        let denom = vdupq_n_f32(32_768.0);
        // 8 samples = 16 bytes per step.
        while i + 8 <= n {
            let p = src.as_ptr().add(i * 2);
            let v = vld1q_s16(p as *const i16);
            let lo = vmovl_s16(vget_low_s16(v)); // i32x4
            let hi = vmovl_s16(vget_high_s16(v));
            let flo = vdivq_f32(vcvtq_f32_s32(lo), denom);
            let fhi = vdivq_f32(vcvtq_f32_s32(hi), denom);
            let out = dst.as_mut_ptr().add(i);
            vst1q_f32(out, flo);
            vst1q_f32(out.add(4), fhi);
            i += 8;
        }
    }
    while i < n {
        let b = i * 2;
        let s = i16::from_le_bytes([src[b], src[b + 1]]);
        dst[i] = s as f32 / 32_768.0;
        i += 1;
    }
}

/// aarch64: 4 stereo frames / step. Convert each channel, then `/ 2.0`.
///
/// # Safety
/// NEON is baseline on aarch64 product targets.
#[cfg(all(feature = "simd", target_arch = "aarch64", not(miri)))]
#[target_feature(enable = "neon")]
pub(super) unsafe fn mix_s16_stereo_neon(src: &[u8], dst: &mut [f32]) {
    use std::arch::aarch64::*;
    let n = dst.len();
    let mut i = 0;
    // SAFETY: loop condition bounds the unaligned loads/stores.
    unsafe {
        let denom = vdupq_n_f32(32_768.0);
        let two = vdupq_n_f32(2.0);
        while i + 4 <= n {
            let v = vld1q_s16(src.as_ptr().add(i * 4) as *const i16);
            let even = vuzp1q_s16(v, v);
            let odd = vuzp2q_s16(v, v);
            let l = vcvtq_f32_s32(vmovl_s16(vget_low_s16(even)));
            let r = vcvtq_f32_s32(vmovl_s16(vget_low_s16(odd)));
            let lf = vdivq_f32(l, denom);
            let rf = vdivq_f32(r, denom);
            vst1q_f32(dst.as_mut_ptr().add(i), vdivq_f32(vaddq_f32(lf, rf), two));
            i += 4;
        }
    }
    if i < n {
        mix_s16_stereo_scalar(&src[i * 4..], &mut dst[i..]);
    }
}

/// # Safety
/// NEON is baseline on aarch64 product targets.
#[cfg(all(feature = "simd", target_arch = "aarch64", not(miri)))]
#[target_feature(enable = "neon")]
pub(super) unsafe fn split_s16_stereo_neon(src: &[u8], left: &mut [f32], right: &mut [f32]) {
    use std::arch::aarch64::*;
    let n = left.len();
    let mut i = 0;
    // SAFETY: loop condition bounds the unaligned loads/stores.
    unsafe {
        let denom = vdupq_n_f32(32_768.0);
        while i + 4 <= n {
            let v = vld1q_s16(src.as_ptr().add(i * 4) as *const i16);
            let even = vuzp1q_s16(v, v);
            let odd = vuzp2q_s16(v, v);
            let l = vdivq_f32(vcvtq_f32_s32(vmovl_s16(vget_low_s16(even))), denom);
            let r = vdivq_f32(vcvtq_f32_s32(vmovl_s16(vget_low_s16(odd))), denom);
            vst1q_f32(left.as_mut_ptr().add(i), l);
            vst1q_f32(right.as_mut_ptr().add(i), r);
            i += 4;
        }
    }
    if i < n {
        split_s16_stereo_scalar(&src[i * 4..], &mut left[i..], &mut right[i..]);
    }
}

/// # Safety
/// Caller must ensure SSE4.1 is available.
#[cfg(all(
    feature = "simd",
    any(target_arch = "x86_64", target_arch = "x86"),
    not(miri)
))]
#[target_feature(enable = "sse4.1")]
pub(super) unsafe fn mix_s16_stereo_sse41(src: &[u8], dst: &mut [f32]) {
    #[cfg(target_arch = "x86")]
    use std::arch::x86::*;
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::*;

    let n = dst.len();
    let mut i = 0;
    // SAFETY: loop condition bounds the unaligned loads/stores.
    unsafe {
        let denom = _mm_set1_ps(32_768.0);
        let two = _mm_set1_ps(2.0);
        while i + 4 <= n {
            let v = _mm_loadu_si128(src.as_ptr().add(i * 4) as *const __m128i);
            let lo = _mm_cvtepi16_epi32(v);
            let hi = _mm_cvtepi16_epi32(_mm_srli_si128(v, 8));
            let flo = _mm_div_ps(_mm_cvtepi32_ps(lo), denom);
            let fhi = _mm_div_ps(_mm_cvtepi32_ps(hi), denom);
            // flo=[L0,R0,L1,R1] fhi=[L2,R2,L3,R3] → left/right then average.
            let left = _mm_shuffle_ps(flo, fhi, 0b10_00_10_00);
            let right = _mm_shuffle_ps(flo, fhi, 0b11_01_11_01);
            _mm_storeu_ps(
                dst.as_mut_ptr().add(i),
                _mm_div_ps(_mm_add_ps(left, right), two),
            );
            i += 4;
        }
    }
    if i < n {
        mix_s16_stereo_scalar(&src[i * 4..], &mut dst[i..]);
    }
}

/// # Safety
/// Caller must ensure SSE4.1 is available.
#[cfg(all(
    feature = "simd",
    any(target_arch = "x86_64", target_arch = "x86"),
    not(miri)
))]
#[target_feature(enable = "sse4.1")]
pub(super) unsafe fn split_s16_stereo_sse41(src: &[u8], left: &mut [f32], right: &mut [f32]) {
    #[cfg(target_arch = "x86")]
    use std::arch::x86::*;
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::*;

    let n = left.len();
    let mut i = 0;
    // SAFETY: loop condition bounds the unaligned loads/stores.
    unsafe {
        let denom = _mm_set1_ps(32_768.0);
        while i + 4 <= n {
            let v = _mm_loadu_si128(src.as_ptr().add(i * 4) as *const __m128i);
            let lo = _mm_cvtepi16_epi32(v);
            let hi = _mm_cvtepi16_epi32(_mm_srli_si128(v, 8));
            let flo = _mm_div_ps(_mm_cvtepi32_ps(lo), denom);
            let fhi = _mm_div_ps(_mm_cvtepi32_ps(hi), denom);
            _mm_storeu_ps(
                left.as_mut_ptr().add(i),
                _mm_shuffle_ps(flo, fhi, 0b10_00_10_00),
            );
            _mm_storeu_ps(
                right.as_mut_ptr().add(i),
                _mm_shuffle_ps(flo, fhi, 0b11_01_11_01),
            );
            i += 4;
        }
    }
    if i < n {
        split_s16_stereo_scalar(&src[i * 4..], &mut left[i..], &mut right[i..]);
    }
}

/// # Safety
/// Caller must ensure SSE2 is available.
#[cfg(all(
    feature = "simd",
    any(target_arch = "x86_64", target_arch = "x86"),
    not(miri)
))]
#[target_feature(enable = "sse2")]
pub(super) unsafe fn mix_s16_stereo_sse2(src: &[u8], dst: &mut [f32]) {
    #[cfg(target_arch = "x86")]
    use std::arch::x86::*;
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::*;

    let n = dst.len();
    let mut i = 0;
    // SAFETY: loop condition bounds the unaligned loads/stores.
    unsafe {
        let denom = _mm_set1_ps(32_768.0);
        let two = _mm_set1_ps(2.0);
        while i + 4 <= n {
            let v = _mm_loadu_si128(src.as_ptr().add(i * 4) as *const __m128i);
            let sign = _mm_srai_epi16(v, 15);
            let lo = _mm_unpacklo_epi16(v, sign);
            let hi = _mm_unpackhi_epi16(v, sign);
            let flo = _mm_div_ps(_mm_cvtepi32_ps(lo), denom);
            let fhi = _mm_div_ps(_mm_cvtepi32_ps(hi), denom);
            let left = _mm_shuffle_ps(flo, fhi, 0b10_00_10_00);
            let right = _mm_shuffle_ps(flo, fhi, 0b11_01_11_01);
            _mm_storeu_ps(
                dst.as_mut_ptr().add(i),
                _mm_div_ps(_mm_add_ps(left, right), two),
            );
            i += 4;
        }
    }
    if i < n {
        mix_s16_stereo_scalar(&src[i * 4..], &mut dst[i..]);
    }
}

/// # Safety
/// Caller must ensure SSE2 is available.
#[cfg(all(
    feature = "simd",
    any(target_arch = "x86_64", target_arch = "x86"),
    not(miri)
))]
#[target_feature(enable = "sse2")]
pub(super) unsafe fn split_s16_stereo_sse2(src: &[u8], left: &mut [f32], right: &mut [f32]) {
    #[cfg(target_arch = "x86")]
    use std::arch::x86::*;
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::*;

    let n = left.len();
    let mut i = 0;
    // SAFETY: loop condition bounds the unaligned loads/stores.
    unsafe {
        let denom = _mm_set1_ps(32_768.0);
        while i + 4 <= n {
            let v = _mm_loadu_si128(src.as_ptr().add(i * 4) as *const __m128i);
            let sign = _mm_srai_epi16(v, 15);
            let lo = _mm_unpacklo_epi16(v, sign);
            let hi = _mm_unpackhi_epi16(v, sign);
            let flo = _mm_div_ps(_mm_cvtepi32_ps(lo), denom);
            let fhi = _mm_div_ps(_mm_cvtepi32_ps(hi), denom);
            _mm_storeu_ps(
                left.as_mut_ptr().add(i),
                _mm_shuffle_ps(flo, fhi, 0b10_00_10_00),
            );
            _mm_storeu_ps(
                right.as_mut_ptr().add(i),
                _mm_shuffle_ps(flo, fhi, 0b11_01_11_01),
            );
            i += 4;
        }
    }
    if i < n {
        split_s16_stereo_scalar(&src[i * 4..], &mut left[i..], &mut right[i..]);
    }
}
