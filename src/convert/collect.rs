//! Safe iterator filling, compiled for the available x86 conversion instructions.
#[inline(always)]
fn append_scalar(source: &[u8], out: &mut Vec<f32>) {
    out.extend(
        source
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| f32::from(i16::from_le_bytes(*b)) * super::I16_SCALE),
    );
}

#[allow(unsafe_code)]
#[inline]
pub(crate) fn append_s16(source: &[u8], out: &mut Vec<f32>) {
    #[cfg(all(
        feature = "simd",
        any(target_arch = "x86", target_arch = "x86_64"),
        not(miri)
    ))]
    if std::arch::is_x86_feature_detected!("sse4.1") {
        // SAFETY: the runtime check establishes the sole target-feature requirement.
        unsafe {
            append_sse41(source, out);
        }
        return;
    }
    append_scalar(source, out);
}

/// # Safety
/// SSE4.1 must be available. All memory initialization uses safe Vec operations.
#[cfg(all(
    feature = "simd",
    any(target_arch = "x86", target_arch = "x86_64"),
    not(miri)
))]
#[allow(unsafe_code)]
#[target_feature(enable = "sse4.1")]
unsafe fn append_sse41(source: &[u8], out: &mut Vec<f32>) {
    append_scalar(source, out);
}
