//! Sample conversion kernels (scalar + optional SIMD).

use crate::header::SampleCodec;

/// `2^-15`. Bit-identical to `/ 32768.0` for every `i16`.
pub(crate) const I16_SCALE: f32 = 1.0 / 32_768.0;
const HALF: f32 = 0.5;

pub(crate) mod g711;
mod scalar;
#[cfg(all(feature = "simd", not(miri)))]
mod simd;

pub(crate) use g711::{
    convert_mono as convert_g711_mono, mix as mix_g711, split as split_g711,
    table_for as g711_table,
};
#[allow(unused_imports)]
use scalar::{
    convert_f32_mono_scalar, convert_s16_mono_scalar, mix_s16_nch_scalar, mix_s16_stereo_scalar,
    split_s16_nch_scalar, split_s16_stereo_scalar,
};
#[cfg(all(feature = "simd", not(miri)))]
#[allow(unused_imports)]
use simd::*;

pub fn convert_s16_le_to_f32(src: &[u8], dst: &mut [f32]) {
    let n = dst.len().min(src.len() / 2);
    if n > 0 {
        convert_s16_mono(&src[..n * 2], &mut dst[..n]);
    }
}

#[inline]
pub(crate) fn convert_s16_mono(src: &[u8], dst: &mut [f32]) {
    debug_assert_eq!(src.len(), dst.len() * 2);
    #[cfg(all(feature = "simd", target_arch = "aarch64", not(miri)))]
    {
        // SAFETY: NEON is baseline on aarch64.
        unsafe {
            convert_s16_mono_neon(src, dst);
        }
    }
    #[cfg(not(all(feature = "simd", target_arch = "aarch64", not(miri))))]
    {
        #[cfg(all(
            feature = "simd",
            any(target_arch = "x86_64", target_arch = "x86"),
            not(miri)
        ))]
        {
            if std::arch::is_x86_feature_detected!("sse4.1") {
                // SAFETY: feature gate above.
                unsafe {
                    convert_s16_mono_sse41(src, dst);
                }
                return;
            }
            if std::arch::is_x86_feature_detected!("sse2") {
                // SAFETY: feature gate above.
                unsafe {
                    convert_s16_mono_sse2(src, dst);
                }
                return;
            }
        }
        convert_s16_mono_scalar(src, dst);
    }
}

/// Interleaved LE s16 to mono f32. Same math as the historical mix:
/// convert each channel, then divide by `channels`.
#[inline]
pub(crate) fn mix_s16_le_to_f32(src: &[u8], dst: &mut [f32], channels: usize) {
    debug_assert!(channels >= 2);
    debug_assert_eq!(src.len(), dst.len() * channels * 2);
    if channels == 2 {
        mix_s16_stereo(src, dst);
    } else {
        mix_s16_nch_scalar(src, dst, channels);
    }
}

/// Interleaved LE s16 to planar f32 (`dst.len()` = channel count, equal lengths).
#[inline]
pub(crate) fn split_s16_le_to_f32(src: &[u8], dst: &mut [&mut [f32]]) {
    let channels = dst.len();
    debug_assert!(channels >= 1);
    let frames = dst.first().map(|c| c.len()).unwrap_or(0);
    debug_assert!(dst.iter().all(|c| c.len() == frames));
    debug_assert_eq!(src.len(), frames * channels * 2);
    match channels {
        1 => convert_s16_mono(src, dst[0]),
        2 => {
            let (left, right) = dst.split_at_mut(1);
            split_s16_stereo(src, left[0], right[0]);
        }
        _ => split_s16_nch_scalar(src, dst),
    }
}

#[inline]
fn mix_s16_stereo(src: &[u8], dst: &mut [f32]) {
    debug_assert_eq!(src.len(), dst.len() * 4);
    #[cfg(all(feature = "simd", target_arch = "aarch64", not(miri)))]
    {
        // SAFETY: NEON is baseline on aarch64.
        unsafe {
            mix_s16_stereo_neon(src, dst);
        }
    }
    #[cfg(not(all(feature = "simd", target_arch = "aarch64", not(miri))))]
    {
        #[cfg(all(
            feature = "simd",
            any(target_arch = "x86_64", target_arch = "x86"),
            not(miri)
        ))]
        {
            if std::arch::is_x86_feature_detected!("sse4.1") {
                // SAFETY: feature gate above.
                unsafe {
                    mix_s16_stereo_sse41(src, dst);
                }
                return;
            }
            if std::arch::is_x86_feature_detected!("sse2") {
                // SAFETY: feature gate above.
                unsafe {
                    mix_s16_stereo_sse2(src, dst);
                }
                return;
            }
        }
        mix_s16_stereo_scalar(src, dst);
    }
}

#[inline]
fn split_s16_stereo(src: &[u8], left: &mut [f32], right: &mut [f32]) {
    debug_assert_eq!(left.len(), right.len());
    debug_assert_eq!(src.len(), left.len() * 4);
    #[cfg(all(feature = "simd", target_arch = "aarch64", not(miri)))]
    {
        // SAFETY: NEON is baseline on aarch64.
        unsafe {
            split_s16_stereo_neon(src, left, right);
        }
    }
    #[cfg(not(all(feature = "simd", target_arch = "aarch64", not(miri))))]
    {
        #[cfg(all(
            feature = "simd",
            any(target_arch = "x86_64", target_arch = "x86"),
            not(miri)
        ))]
        {
            if std::arch::is_x86_feature_detected!("sse4.1") {
                // SAFETY: feature gate above.
                unsafe {
                    split_s16_stereo_sse41(src, left, right);
                }
                return;
            }
            if std::arch::is_x86_feature_detected!("sse2") {
                // SAFETY: feature gate above.
                unsafe {
                    split_s16_stereo_sse2(src, left, right);
                }
                return;
            }
        }
        split_s16_stereo_scalar(src, left, right);
    }
}

#[inline]
pub(crate) fn convert_f32_mono(src: &[u8], dst: &mut [f32]) {
    debug_assert_eq!(src.len(), dst.len() * 4);
    // Unaligned-safe: data chunk is not guaranteed 4-byte aligned after the
    // RIFF header walk, so never cast the byte slice to `&[f32]`.
    #[cfg(all(
        feature = "simd",
        target_endian = "little",
        target_arch = "aarch64",
        not(miri)
    ))]
    {
        // SAFETY: NEON is baseline on aarch64; loads are unaligned.
        unsafe {
            convert_f32_mono_neon(src, dst);
        }
    }
    #[cfg(not(all(
        feature = "simd",
        target_endian = "little",
        target_arch = "aarch64",
        not(miri)
    )))]
    {
        #[cfg(all(
            feature = "simd",
            target_endian = "little",
            any(target_arch = "x86_64", target_arch = "x86"),
            not(miri)
        ))]
        {
            if std::arch::is_x86_feature_detected!("sse2") {
                // SAFETY: feature gate above; loads are unaligned.
                unsafe {
                    convert_f32_mono_sse2(src, dst);
                }
                return;
            }
        }
        convert_f32_mono_scalar(src, dst);
    }
}

/// Little-endian host: copy 4x f32 bits via unaligned NEON load/store.
///
/// # Safety
/// NEON is baseline on aarch64. `src`/`dst` need not be 16-byte aligned.
#[cfg(all(
    feature = "simd",
    target_endian = "little",
    target_arch = "aarch64",
    not(miri)
))]
#[target_feature(enable = "neon")]
unsafe fn convert_f32_mono_neon(src: &[u8], dst: &mut [f32]) {
    use std::arch::aarch64::*;
    let n = dst.len();
    let mut i = 0;
    // SAFETY: `vld1q_u8` / `vst1q_u8` are unaligned; bounds via `i + 4 <= n`.
    unsafe {
        while i + 4 <= n {
            let bits = vld1q_u8(src.as_ptr().add(i * 4));
            vst1q_u8(dst.as_mut_ptr().add(i) as *mut u8, bits);
            i += 4;
        }
    }
    if i < n {
        convert_f32_mono_scalar(&src[i * 4..], &mut dst[i..]);
    }
}

/// # Safety
/// Caller must ensure SSE2 is available. Loads/stores are unaligned.
#[cfg(all(
    feature = "simd",
    target_endian = "little",
    any(target_arch = "x86_64", target_arch = "x86"),
    not(miri)
))]
#[target_feature(enable = "sse2")]
unsafe fn convert_f32_mono_sse2(src: &[u8], dst: &mut [f32]) {
    #[cfg(target_arch = "x86")]
    use std::arch::x86::*;
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::*;

    let n = dst.len();
    let mut i = 0;
    // SAFETY: `_mm_loadu_ps` / `_mm_storeu_ps` accept any alignment.
    unsafe {
        while i + 4 <= n {
            let v = _mm_loadu_ps(src.as_ptr().add(i * 4) as *const f32);
            _mm_storeu_ps(dst.as_mut_ptr().add(i), v);
            i += 4;
        }
    }
    if i < n {
        convert_f32_mono_scalar(&src[i * 4..], &mut dst[i..]);
    }
}

#[inline]
pub(crate) fn convert_sample(codec: SampleCodec, b: &[u8], big_endian: bool) -> f32 {
    debug_assert!(
        b.len()
            >= match codec {
                SampleCodec::U8 | SampleCodec::ALaw | SampleCodec::MuLaw => 1,
                SampleCodec::S16 => 2,
                SampleCodec::S24 => 3,
                SampleCodec::S24_4 | SampleCodec::S32 | SampleCodec::F32 => 4,
                SampleCodec::F64 => 8,
                _ => 0,
            }
    );
    match codec {
        SampleCodec::U8 => (b[0] as f32) * (1.0 / 128.0) - 1.0,
        SampleCodec::S16 => {
            let s = if big_endian {
                i16::from_be_bytes([b[0], b[1]])
            } else {
                i16::from_le_bytes([b[0], b[1]])
            };
            s as f32 * I16_SCALE
        }
        SampleCodec::S24 => {
            let (b0, b1, b2) = if big_endian {
                (b[2], b[1], b[0]) // bring to LE order for shared sign-extend
            } else {
                (b[0], b[1], b[2])
            };
            let sign = if b2 & 0x80 != 0 { 0xFF } else { 0x00 };
            i32::from_le_bytes([b0, b1, b2, sign]) as f32 * (1.0 / 8_388_608.0)
        }
        SampleCodec::S24_4 => {
            let x = if big_endian {
                u32::from_be_bytes([b[0], b[1], b[2], b[3]])
            } else {
                u32::from_le_bytes([b[0], b[1], b[2], b[3]])
            };
            let i = if x & (1 << 23) == 0 {
                (x & 0x00ff_ffff) as i32
            } else {
                (x | 0xff00_0000) as i32
            };
            i as f32 / 8_388_608.0
        }
        // Historical pipeline converts s32 through f64 intermediate.
        SampleCodec::S32 => {
            let v = if big_endian {
                i32::from_be_bytes([b[0], b[1], b[2], b[3]])
            } else {
                i32::from_le_bytes([b[0], b[1], b[2], b[3]])
            };
            (v as f64 / 2_147_483_648.0) as f32
        }
        SampleCodec::F32 => {
            if big_endian {
                f32::from_be_bytes([b[0], b[1], b[2], b[3]])
            } else {
                f32::from_le_bytes([b[0], b[1], b[2], b[3]])
            }
        }
        SampleCodec::F64 => {
            if big_endian {
                f64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]) as f32
            } else {
                f64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]) as f32
            }
        }
        SampleCodec::ALaw => g711::lut(&g711::ALAW_F32, b[0]),
        SampleCodec::MuLaw => g711::lut(&g711::MULAW_F32, b[0]),
        SampleCodec::MsAdpcm
        | SampleCodec::ImaAdpcm
        | SampleCodec::G722
        | SampleCodec::Gsm
        | SampleCodec::Unsupported => 0.0,
    }
}

/// Pack little-endian i16 with the decode scale (`* 32768`, then clamp to
/// `i16`). `-1.0` maps to `-32768`, `1.0` maps to `32767`.
#[must_use]
pub fn f32_to_s16le(samples: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len().saturating_mul(2));
    for s in samples {
        let i = (s * 32_768.0).round();
        let i = i.clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16;
        out.extend_from_slice(&i.to_le_bytes());
    }
    out
}

/// Unpack little-endian i16 with the decode scale (`/ 32768`). Odd length is
/// [`crate::WavError::OddPcm`].
pub fn s16le_to_f32(pcm: &[u8]) -> crate::Result<Vec<f32>> {
    if !pcm.len().is_multiple_of(2) {
        return Err(crate::WavError::OddPcm);
    }
    let mut out = Vec::with_capacity(pcm.len() / 2);
    for s in pcm.as_chunks::<2>().0 {
        let v = i16::from_le_bytes(*s);
        out.push(f32::from(v) * I16_SCALE);
    }
    Ok(out)
}

#[cfg(test)]
#[path = "../convert_tests.rs"]
mod convert_tests;
