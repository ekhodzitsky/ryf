//! Incremental WAVE writer with terminal I/O failures.
use super::{WriteFormat, WriteSpec, header::frame_bytes, layout::Layout};
use crate::{FormatKind, Result, WavError};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Open,
    Finalized,
    Failed,
}

/// Streaming WAVE writer. Sizes are patched on [`Self::finalize`].
/// Drop attempts finalization only while open and cannot report errors.
/// After an I/O failure the writer is terminal: discard the incomplete file.
/// Validation failures before any I/O leave it usable.
pub struct WavWriter<W: Read + Write + Seek> {
    inner: W,
    spec: WriteSpec,
    data_bytes: u64,
    state: State,
    layout: Layout,
}

impl WavWriter<File> {
    /// Validate the specification, then create or truncate a RIFF file.
    /// The file is opened for reading too, allowing later RF64 promotion.
    pub fn create(path: &Path, spec: WriteSpec) -> Result<Self> {
        Self::create_layout(path, spec, Layout::Riff)
    }
    /// Validate, then create or truncate a forced RF64 file.
    pub fn create_rf64(path: &Path, spec: WriteSpec) -> Result<Self> {
        Self::create_layout(path, spec, Layout::Rf64)
    }
    /// Validate, then create or truncate a RIFX file.
    pub fn create_rifx(path: &Path, spec: WriteSpec) -> Result<Self> {
        Self::create_layout(path, spec, Layout::Rifx)
    }
    /// Validate, then create or truncate a WAVEFORMATEXTENSIBLE file.
    pub fn create_extensible(path: &Path, spec: WriteSpec) -> Result<Self> {
        Self::create_layout(path, spec, Layout::Extensible)
    }
    fn create_layout(path: &Path, spec: WriteSpec, layout: Layout) -> Result<Self> {
        let header = layout.header(spec, 0)?;
        let file = OpenOptions::new()
            .read(matches!(layout, Layout::Riff | Layout::Extensible))
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        Self::start(file, spec, layout, &header)
    }
}

impl<W: Read + Write + Seek> WavWriter<W> {
    /// Write a RIFF header at cursor zero; nonzero origins are rejected before
    /// writing. The sink should be empty (generic sinks are not truncated).
    /// Promotes to RF64 when sizes overflow; relocation reads the existing data.
    pub fn new(inner: W, spec: WriteSpec) -> Result<Self> {
        Self::new_layout(inner, spec, Layout::Riff)
    }
    /// Write a forced RF64 header. Same cursor requirements as [`Self::new`].
    pub fn new_rf64(inner: W, spec: WriteSpec) -> Result<Self> {
        Self::new_layout(inner, spec, Layout::Rf64)
    }
    /// Write RIFX. Input PCM remains little-endian. See [`Self::new`].
    pub fn new_rifx(inner: W, spec: WriteSpec) -> Result<Self> {
        Self::new_layout(inner, spec, Layout::Rifx)
    }
    /// Write WAVEFORMATEXTENSIBLE, promoting to RF64 on size overflow.
    /// Same cursor requirements as [`Self::new`].
    pub fn new_extensible(inner: W, spec: WriteSpec) -> Result<Self> {
        Self::new_layout(inner, spec, Layout::Extensible)
    }
    fn new_layout(inner: W, spec: WriteSpec, layout: Layout) -> Result<Self> {
        let header = layout.header(spec, 0)?;
        Self::start(inner, spec, layout, &header)
    }
    fn start(mut inner: W, spec: WriteSpec, layout: Layout, header: &[u8]) -> Result<Self> {
        if inner.stream_position()? != 0 {
            return Err(WavError::format(FormatKind::InvalidOperation));
        }
        inner.write_all(header)?;
        Ok(Self {
            inner,
            spec,
            data_bytes: 0,
            state: State::Open,
            layout,
        })
    }
    fn require_open(&self) -> Result<()> {
        if self.state != State::Open {
            Err(WavError::format(FormatKind::InvalidOperation))
        } else {
            Ok(())
        }
    }

    /// Append whole interleaved frames. Any I/O error makes the writer terminal.
    pub fn write_pcm(&mut self, pcm: &[u8]) -> Result<()> {
        self.require_open()?;
        let fb = frame_bytes(self.spec)?;
        if !pcm.len().is_multiple_of(fb) {
            return Err(WavError::OddPcm);
        }
        let next = self
            .data_bytes
            .checked_add(pcm.len() as u64)
            .ok_or(WavError::RiffTooLarge)?;
        let promote = self.layout.overflows(self.spec, next);
        let target = if promote {
            self.layout.promoted()
        } else {
            self.layout
        };
        target.file_len(self.spec, next)?;
        if promote && self.layout == Layout::Rifx {
            return Err(WavError::RiffTooLarge);
        }
        // Set the terminal state before the first mutation; only full success
        // restores Open. A short write/seek may already have changed the sink.
        self.state = State::Failed;
        if promote {
            self.promote_to_rf64()?;
        }
        if self.layout == Layout::Rifx {
            let width = self.spec.format.bytes_per_sample();
            let mut scratch = [0u8; 8192];
            let chunk = scratch.len() / fb * fb;
            for input in pcm.chunks(chunk) {
                scratch[..input.len()].copy_from_slice(input);
                for sample in scratch[..input.len()].chunks_exact_mut(width) {
                    sample.reverse();
                }
                self.inner.write_all(&scratch[..input.len()])?;
            }
        } else {
            self.inner.write_all(pcm)?;
        }
        self.data_bytes = next;
        self.state = State::Open;
        Ok(())
    }
    /// Append IEEE f32 using bounded conversion scratch.
    pub fn write_f32_samples(&mut self, samples: &[f32]) -> Result<()> {
        self.require_open()?;
        if self.spec.format != WriteFormat::F32 {
            return Err(WavError::unsupported_codec(3));
        }
        let channels = usize::from(self.spec.channels);
        if !samples.len().is_multiple_of(channels) {
            return Err(WavError::OddPcm);
        }
        let bytes = samples.len().checked_mul(4).ok_or(WavError::RiffTooLarge)?;
        let next = self
            .data_bytes
            .checked_add(bytes as u64)
            .ok_or(WavError::RiffTooLarge)?;
        let promote = self.layout.overflows(self.spec, next);
        if promote && self.layout == Layout::Rifx {
            return Err(WavError::RiffTooLarge);
        }
        let target = if promote {
            self.layout.promoted()
        } else {
            self.layout
        };
        target.file_len(self.spec, next)?;
        let mut scratch = [0u8; 8192];
        let chunk = scratch.len() / 4 / channels * channels;
        for part in samples.chunks(chunk) {
            for (dst, sample) in scratch.as_chunks_mut::<4>().0.iter_mut().zip(part) {
                dst.copy_from_slice(&sample.to_le_bytes());
            }
            self.write_pcm(&scratch[..part.len() * 4])?;
        }
        Ok(())
    }
    /// Bytes from completely successful payload writes; a failed writer may
    /// contain an additional partial write and must not be reused.
    #[must_use]
    pub fn data_bytes(&self) -> u64 {
        self.data_bytes
    }

    /// Patch sizes and flush. Repeated successful finalization is harmless.
    /// Any I/O failure is terminal; neither retry nor Drop repairs the file.
    pub fn finalize(&mut self) -> Result<()> {
        if self.state == State::Finalized {
            return Ok(());
        }
        self.require_open()?;
        let header = self.layout.header(self.spec, self.data_bytes)?;
        let end = self.layout.file_len(self.spec, self.data_bytes)?;
        self.state = State::Failed;
        if self.data_bytes & 1 != 0 {
            self.inner.write_all(&[0])?;
        }
        self.inner.seek(SeekFrom::Start(0))?;
        self.inner.write_all(&header)?;
        self.inner.seek(SeekFrom::Start(end))?;
        self.inner.flush()?;
        self.state = State::Finalized;
        Ok(())
    }
    fn promote_to_rf64(&mut self) -> Result<()> {
        let next = self.layout.promoted();
        if matches!(self.layout, Layout::Rf64 | Layout::Rf64Extensible) {
            return Ok(());
        }
        let old_len = self.layout.header_len(self.spec);
        let new_len = next.header_len(self.spec);
        let header = next.header(self.spec, self.data_bytes)?;
        let mut scratch = [0u8; 65536];
        let mut remaining = self.data_bytes;
        while remaining > 0 {
            let n = remaining.min(scratch.len() as u64) as usize;
            let src = old_len + remaining - n as u64;
            self.inner.seek(SeekFrom::Start(src))?;
            self.inner.read_exact(&mut scratch[..n])?;
            self.inner.seek(SeekFrom::Start(src + new_len - old_len))?;
            self.inner.write_all(&scratch[..n])?;
            remaining -= n as u64;
        }
        self.inner.seek(SeekFrom::Start(0))?;
        self.inner.write_all(&header)?;
        self.inner
            .seek(SeekFrom::Start(new_len + self.data_bytes))?;
        self.layout = next;
        Ok(())
    }
    #[cfg(test)]
    pub(crate) fn force_rf64(&mut self) -> Result<()> {
        self.require_open()?;
        self.state = State::Failed;
        self.promote_to_rf64()?;
        self.state = State::Open;
        Ok(())
    }
}
impl<W: Read + Write + Seek> Drop for WavWriter<W> {
    fn drop(&mut self) {
        if self.state == State::Open {
            let _ = self.finalize();
        }
    }
}
