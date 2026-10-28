//! Shared bounded source windows for collect and callback conversion.
use super::{check_duration, scratch_frames};
use crate::{ByteSource, FormatKind, Result, WavError};
use std::ops::Range;

pub(super) fn for_each_input(
    source: &mut ByteSource<'_>,
    total: usize,
    frame_bytes: usize,
    max: usize,
    rate: u32,
    mut visit: impl FnMut(&[u8], Range<usize>) -> Result<()>,
) -> Result<usize> {
    check_duration(total, max, rate)?;
    if frame_bytes == 0 {
        return Err(WavError::format(FormatKind::InvalidSize));
    }
    let block = scratch_frames(frame_bytes, total);
    let mut scratch = Vec::new();
    let mut offset = 0;
    while offset < total {
        let end = offset + (total - offset).min(block);
        let n = (end - offset)
            .checked_mul(frame_bytes)
            .ok_or_else(|| WavError::format(FormatKind::InvalidSize))?;
        let bytes = source
            .read_window(n, &mut scratch)
            .map_err(WavError::packet_io)?;
        visit(bytes, offset..end)?;
        offset = end;
    }
    Ok(total)
}
