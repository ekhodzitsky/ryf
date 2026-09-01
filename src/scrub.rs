//! PCM scratch buffers. v1 does not scrub on drop (no `zeroize`).

pub(crate) type ScrubVec<T> = Vec<T>;

#[inline]
pub(crate) fn scrub_vec<T>(v: Vec<T>) -> ScrubVec<T> {
    v
}
