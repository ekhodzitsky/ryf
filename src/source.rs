//! Seekable byte source for WAVE demux/decode (pure `std`).

use std::io::{self, Read, Seek, SeekFrom};

/// Seekable sink for memory-backed sources (`Read`/`Seek` go through
/// [`Contiguous`], so the boxed inner is never touched).
struct DeadInner;

impl Read for DeadInner {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        Ok(0)
    }
}

impl Seek for DeadInner {
    fn seek(&mut self, _: SeekFrom) -> io::Result<u64> {
        Ok(0)
    }
}

/// Object-safe `Read + Seek`.
pub trait ReadSeek: Read + Seek {}
impl<T: Read + Seek> ReadSeek for T {}

/// Contiguous memory view for zero-copy handoff (WAVE PCM, adapters).
enum Contiguous<'a> {
    None,
    Slice(&'a [u8]),
    Owned(Vec<u8>),
}

impl Contiguous<'_> {
    #[inline]
    fn as_slice(&self) -> Option<&[u8]> {
        match self {
            Contiguous::None => None,
            Contiguous::Slice(s) => Some(s),
            Contiguous::Owned(v) => Some(v.as_slice()),
        }
    }
}

/// Seekable byte stream with exact-read helpers used by demux / decode.
///
/// Memory-backed sources (`from_slice` / `from_vec`) expose a contiguous view
/// so PCM convert can walk the `data` payload without an extra copy.
/// File / `Read + Seek` sources stream.
pub struct ByteSource<'a> {
    inner: Box<dyn ReadSeek + 'a>,
    pos: u64,
    byte_len: Option<u64>,
    memory: Contiguous<'a>,
}

impl<'a> ByteSource<'a> {
    /// Wrap an already-boxed `Read + Seek`. Prefer [`from_slice`],
    /// [`from_file`], or [`from_read_seek`].
    pub fn new(inner: Box<dyn ReadSeek + Send + 'a>, byte_len: Option<u64>) -> Self {
        Self {
            inner,
            pos: 0,
            byte_len,
            memory: Contiguous::None,
        }
    }

    /// Borrowed in-memory buffer (zero-copy; no heap clone of `data`).
    pub fn from_slice(data: &'a [u8]) -> ByteSource<'a> {
        let len = data.len() as u64;
        Self {
            inner: Box::new(DeadInner),
            pos: 0,
            byte_len: Some(len),
            memory: Contiguous::Slice(data),
        }
    }

    /// Owned in-memory buffer (contiguous view — same zero-copy PCM path as
    /// [`from_slice`]).
    pub fn from_vec(data: Vec<u8>) -> ByteSource<'static> {
        let len = data.len() as u64;
        ByteSource {
            inner: Box::new(DeadInner),
            pos: 0,
            byte_len: Some(len),
            memory: Contiguous::Owned(data),
        }
    }

    /// Local file, streamed.
    pub fn from_file(file: std::fs::File) -> ByteSource<'static> {
        let len = file.metadata().ok().map(|m| m.len());
        ByteSource {
            inner: Box::new(file),
            pos: 0,
            byte_len: len,
            memory: Contiguous::None,
        }
    }

    /// Arbitrary `Read + Seek` with optional known length.
    pub fn from_read_seek<'b, R>(inner: R, byte_len: Option<u64>) -> ByteSource<'b>
    where
        R: Read + Seek + 'b,
    {
        ByteSource {
            inner: Box::new(inner),
            pos: 0,
            byte_len,
            memory: Contiguous::None,
        }
    }

    #[inline]
    pub fn pos(&self) -> u64 {
        self.pos
    }

    #[inline]
    pub fn byte_len(&self) -> Option<u64> {
        self.byte_len
    }

    /// Full contiguous buffer when memory-backed (slice / vec).
    #[inline]
    pub fn contiguous_slice(&self) -> Option<&[u8]> {
        self.memory.as_slice()
    }

    /// Remaining bytes from the current position, if memory-backed.
    #[inline]
    pub fn remaining_slice(&self) -> Option<&[u8]> {
        let data = self.memory.as_slice()?;
        let pos = self.pos as usize;
        if pos >= data.len() {
            return Some(&[]);
        }
        Some(&data[pos..])
    }

    pub fn read_buf_exact(&mut self, buf: &mut [u8]) -> io::Result<()> {
        self.read_exact(buf)
    }

    pub fn read_quad_bytes(&mut self) -> io::Result<[u8; 4]> {
        let mut buf = [0u8; 4];
        self.read_exact(&mut buf)?;
        Ok(buf)
    }

    pub fn read_u8(&mut self) -> io::Result<u8> {
        let mut buf = [0u8; 1];
        self.read_exact(&mut buf)?;
        Ok(buf[0])
    }

    pub fn read_u16(&mut self) -> io::Result<u16> {
        let mut buf = [0u8; 2];
        self.read_exact(&mut buf)?;
        Ok(u16::from_le_bytes(buf))
    }

    pub fn read_be_u16(&mut self) -> io::Result<u16> {
        let mut buf = [0u8; 2];
        self.read_exact(&mut buf)?;
        Ok(u16::from_be_bytes(buf))
    }

    pub fn read_u32(&mut self) -> io::Result<u32> {
        let mut buf = [0u8; 4];
        self.read_exact(&mut buf)?;
        Ok(u32::from_le_bytes(buf))
    }

    pub fn read_be_u32(&mut self) -> io::Result<u32> {
        let mut buf = [0u8; 4];
        self.read_exact(&mut buf)?;
        Ok(u32::from_be_bytes(buf))
    }

    /// Skip `count` bytes (seek when large; else read-discard).
    pub fn ignore_bytes(&mut self, count: u64) -> io::Result<()> {
        if count == 0 {
            return Ok(());
        }

        const SEEK_THRESHOLD: u64 = 8192;
        if count >= SEEK_THRESHOLD {
            if let Some(len) = self.byte_len {
                let end = self.pos.saturating_add(count);
                if end > len {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "byte source exhausted during ignore_bytes",
                    ));
                }
            }
            if self.seek(SeekFrom::Current(count as i64)).is_ok() {
                return Ok(());
            }
        }

        let mut remaining = count;
        let mut scratch = [0u8; 8192];
        while remaining > 0 {
            let want = (remaining as usize).min(scratch.len());
            let n = self.read(&mut scratch[..want])?;
            if n == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "byte source exhausted during ignore_bytes",
                ));
            }
            remaining -= n as u64;
        }
        Ok(())
    }
}

impl Read for ByteSource<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if let Some(data) = self.memory.as_slice() {
            let pos = self.pos as usize;
            if pos >= data.len() {
                return Ok(0);
            }
            let n = (data.len() - pos).min(buf.len());
            buf[..n].copy_from_slice(&data[pos..pos + n]);
            self.pos += n as u64;
            return Ok(n);
        }
        let n = self.inner.read(buf)?;
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for ByteSource<'_> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        if let Some(data) = self.memory.as_slice() {
            let len = data.len() as u64;
            let next = match pos {
                SeekFrom::Start(p) => p as i64,
                SeekFrom::End(p) => len as i64 + p,
                SeekFrom::Current(p) => self.pos as i64 + p,
            };
            if next < 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "seek before start",
                ));
            }
            self.pos = next as u64;
            return Ok(self.pos);
        }
        let p = self.inner.seek(pos)?;
        self.pos = p;
        Ok(p)
    }
}

#[cfg(test)]
#[path = "source_tests.rs"]
mod source_tests;
