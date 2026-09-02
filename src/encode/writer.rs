//! Incremental WAVE writer (RIFF, or RF64 via [`WavWriter::new_rf64`]).
//! Sizes are patched on [`WavWriter::finalize`] or on drop.
//! Classic RIFF auto-promotes to RF64 when the payload would overflow `u32`.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use super::header::{
    RF64_DATA_SIZE_POS, RF64_FACT_FRAMES_POS, RF64_RIFF_SIZE_POS, RF64_SAMPLE_COUNT_POS,
    data_len_pos, fact_frames_pos, frame_bytes, needs_rf64, push_header, push_rf64_header,
    rf64_header_len, riff_prefix, u32_len, validate_spec,
};
use super::{WriteFormat, WriteSpec};
use crate::error::{FormatKind, Result, WavError};

/// Streaming WAVE writer. Header sizes are placeholders until [`Self::finalize`]
/// (also attempted on drop, errors swallowed).
pub struct WavWriter<W: Read + Write + Seek> {
    inner: W,
    spec: WriteSpec,
    data_bytes: u64,
    finalized: bool,
    rf64: bool,
}

impl WavWriter<File> {
    /// Create `path` and write a WAVE header with a zero-length `data` chunk.
    pub fn create(path: &Path, spec: WriteSpec) -> Result<Self> {
        Self::new(File::create(path)?, spec)
    }

    /// Create `path` as RF64 (sizes in `ds64`; no 4 GiB cap).
    pub fn create_rf64(path: &Path, spec: WriteSpec) -> Result<Self> {
        Self::new_rf64(File::create(path)?, spec)
    }
}

impl<W: Read + Write + Seek> WavWriter<W> {
    /// Write a WAVE header with a zero-length `data` chunk onto `inner`.
    /// Promotes to RF64 on write if sizes would overflow `u32`.
    pub fn new(mut inner: W, spec: WriteSpec) -> Result<Self> {
        validate_spec(spec)?;
        let mut header = Vec::with_capacity(58);
        push_header(&mut header, spec, 0)?;
        inner.write_all(&header)?;
        Ok(Self {
            inner,
            spec,
            data_bytes: 0,
            finalized: false,
            rf64: false,
        })
    }

    /// Write an RF64 header (`ds64` + `0xFFFFFFFF` sizes) onto `inner`.
    pub fn new_rf64(mut inner: W, spec: WriteSpec) -> Result<Self> {
        validate_spec(spec)?;
        let mut header = Vec::with_capacity(94);
        push_rf64_header(&mut header, spec, 0, 0)?;
        inner.write_all(&header)?;
        Ok(Self {
            inner,
            spec,
            data_bytes: 0,
            finalized: false,
            rf64: true,
        })
    }

    /// Append interleaved PCM bytes (`format` width × channels per frame).
    pub fn write_pcm(&mut self, pcm: &[u8]) -> Result<()> {
        if self.finalized {
            return Err(WavError::format(FormatKind::InvalidOperation));
        }
        let fb = frame_bytes(self.spec)?;
        if !pcm.len().is_multiple_of(fb) {
            return Err(WavError::OddPcm);
        }
        let next = self
            .data_bytes
            .checked_add(pcm.len() as u64)
            .ok_or(WavError::RiffTooLarge)?;
        if !self.rf64 && needs_rf64(self.spec, next) {
            self.promote_to_rf64()?;
        }
        self.inner.write_all(pcm)?;
        self.data_bytes = next;
        Ok(())
    }

    /// Append IEEE f32 samples. Spec must be [`WriteFormat::F32`].
    pub fn write_f32_samples(&mut self, samples: &[f32]) -> Result<()> {
        if self.spec.format != WriteFormat::F32 {
            return Err(WavError::unsupported_codec(3));
        }
        let mut buf = Vec::with_capacity(samples.len().saturating_mul(4));
        for s in samples {
            buf.extend_from_slice(&s.to_le_bytes());
        }
        self.write_pcm(&buf)
    }

    /// Bytes written to the `data` chunk so far.
    #[must_use]
    pub fn data_bytes(&self) -> u64 {
        self.data_bytes
    }

    /// Patch RIFF / `fact` / `data` sizes and flush. Safe to call once.
    pub fn finalize(&mut self) -> Result<()> {
        if self.finalized {
            return Ok(());
        }
        self.patch()?;
        self.inner.flush()?;
        self.finalized = true;
        Ok(())
    }

    /// Rewrite a classic RIFF header as RF64 and slide the payload forward.
    fn promote_to_rf64(&mut self) -> Result<()> {
        if self.rf64 {
            return Ok(());
        }
        let old_len = u64::from(riff_prefix(self.spec)).saturating_add(8);
        let new_len = rf64_header_len(self.spec);
        let delta = new_len.saturating_sub(old_len);
        let payload = self.data_bytes;
        const BLOCK: usize = 64 * 1024;
        let mut buf = [0u8; BLOCK];
        let mut remaining = payload;
        while remaining > 0 {
            let n = remaining.min(BLOCK as u64) as usize;
            let src = old_len.saturating_add(remaining).saturating_sub(n as u64);
            let dst = src.saturating_add(delta);
            self.inner.seek(SeekFrom::Start(src))?;
            self.inner.read_exact(&mut buf[..n])?;
            self.inner.seek(SeekFrom::Start(dst))?;
            self.inner.write_all(&buf[..n])?;
            remaining -= n as u64;
        }
        let fb = frame_bytes(self.spec)? as u64;
        let frames = payload.checked_div(fb).unwrap_or(0);
        let mut header = Vec::with_capacity(new_len as usize);
        push_rf64_header(&mut header, self.spec, payload, frames)?;
        self.inner.seek(SeekFrom::Start(0))?;
        self.inner.write_all(&header)?;
        self.inner
            .seek(SeekFrom::Start(new_len.saturating_add(payload)))?;
        self.rf64 = true;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn force_rf64(&mut self) -> Result<()> {
        self.promote_to_rf64()
    }

    fn patch(&mut self) -> Result<()> {
        if self.rf64 {
            return self.patch_rf64();
        }
        let data_len =
            u32_len(usize::try_from(self.data_bytes).map_err(|_| WavError::RiffTooLarge)?)?;
        let riff_len = riff_prefix(self.spec)
            .checked_add(data_len)
            .ok_or(WavError::RiffTooLarge)?;
        self.inner.seek(SeekFrom::Start(4))?;
        self.inner.write_all(&riff_len.to_le_bytes())?;
        if let Some(pos) = fact_frames_pos(self.spec) {
            let fb = frame_bytes(self.spec)? as u32;
            let frames = data_len.checked_div(fb).unwrap_or(0);
            self.inner.seek(SeekFrom::Start(pos))?;
            self.inner.write_all(&frames.to_le_bytes())?;
        }
        self.inner.seek(SeekFrom::Start(data_len_pos(self.spec)))?;
        self.inner.write_all(&data_len.to_le_bytes())?;
        self.inner.seek(SeekFrom::End(0))?;
        Ok(())
    }

    fn patch_rf64(&mut self) -> Result<()> {
        let data_len = self.data_bytes;
        let fb = frame_bytes(self.spec)? as u64;
        let frames = data_len.checked_div(fb).unwrap_or(0);
        let riff_size = rf64_header_len(self.spec)
            .saturating_add(data_len)
            .saturating_sub(8);
        self.inner.seek(SeekFrom::Start(RF64_RIFF_SIZE_POS))?;
        self.inner.write_all(&riff_size.to_le_bytes())?;
        self.inner.seek(SeekFrom::Start(RF64_DATA_SIZE_POS))?;
        self.inner.write_all(&data_len.to_le_bytes())?;
        self.inner.seek(SeekFrom::Start(RF64_SAMPLE_COUNT_POS))?;
        self.inner.write_all(&frames.to_le_bytes())?;
        if self.spec.format.is_float() {
            let fact = frames.min(u64::from(u32::MAX)) as u32;
            self.inner.seek(SeekFrom::Start(RF64_FACT_FRAMES_POS))?;
            self.inner.write_all(&fact.to_le_bytes())?;
        }
        self.inner.seek(SeekFrom::End(0))?;
        Ok(())
    }
}

impl<W: Read + Write + Seek> Drop for WavWriter<W> {
    fn drop(&mut self) {
        if !self.finalized {
            let _ = self.patch();
        }
    }
}
