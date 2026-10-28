//! Bounded RF64 chunk-size records, consumed once per sentinel occurrence.
use super::riff::{eof_trunc, read_u64_le};
use crate::{ByteSource, FormatKind, Result, WavError};

/// At most 64 KiB of on-disk records (plus bounded Vec element padding).
const MAX_ENTRIES: u32 = 4096;
#[derive(Default)]
pub(super) struct ChunkSizes(Vec<([u8; 4], u64)>);
impl ChunkSizes {
    pub(super) fn read(source: &mut ByteSource<'_>, count: u32, remaining: u64) -> Result<Self> {
        let bytes = u64::from(count) * 12;
        if count > MAX_ENTRIES || bytes > remaining {
            return Err(WavError::format(FormatKind::MalformedChunk));
        }
        let mut entries = Vec::with_capacity(count as usize);
        for _ in 0..count {
            entries.push((
                source.read_quad_bytes().map_err(eof_trunc)?,
                read_u64_le(source)?,
            ));
        }
        source.ignore_bytes(remaining - bytes).map_err(eof_trunc)?;
        Ok(Self(entries))
    }
    pub(super) fn take(&mut self, tag: [u8; 4]) -> Result<u64> {
        let index = self
            .0
            .iter()
            .position(|(id, _)| *id == tag)
            .ok_or_else(|| WavError::format(FormatKind::MalformedChunk))?;
        Ok(self.0.remove(index).1)
    }
}
