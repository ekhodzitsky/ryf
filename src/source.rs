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

/// Object-safe `Read + Seek + Send` (0.3 library contract).
pub trait ReadSeek: Read + Seek + Send {}
impl<T: Read + Seek + Send> ReadSeek for T {}

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
/// The inner reader is [`Send`]. Memory-backed sources (`from_slice` /
/// `from_vec`) expose a contiguous view so PCM convert can walk the `data`
/// payload without an extra copy. File / `Read + Seek` sources stream.
pub struct ByteSource<'a> {
    inner: Box<dyn ReadSeek + 'a>,
    pos: u64,
    byte_len: Option<u64>,
    memory: Contiguous<'a>,
}

impl<'a> ByteSource<'a> {
    /// Wrap an already-boxed `Read + Seek + Send`. Prefer [`from_slice`],
    /// [`from_file`], or [`from_read_seek`].
    pub fn new(inner: Box<dyn ReadSeek + 'a>, byte_len: Option<u64>) -> Self {
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

    /// Owned in-memory buffer (contiguous view - same zero-copy PCM path as
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

    /// Local file, streamed. The cursor is rewound to 0.
    pub fn from_file(file: std::fs::File) -> ByteSource<'static> {
        let mut file = file;
        let len = file
            .metadata()
            .ok()
            .map(|m| m.len())
            .or_else(|| file.seek(SeekFrom::End(0)).ok());
        let pos = match file.seek(SeekFrom::Start(0)) {
            Ok(p) => p,
            Err(_) => file.stream_position().unwrap_or(0),
        };
        ByteSource {
            inner: Box::new(file),
            pos,
            byte_len: len,
            memory: Contiguous::None,
        }
    }

    /// Arbitrary `Read + Seek + Send` with optional known length.
    pub fn from_read_seek<'b, R>(inner: R, byte_len: Option<u64>) -> ByteSource<'b>
    where
        R: Read + Seek + Send + 'b,
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
        let pos = usize::try_from(self.pos).unwrap_or(usize::MAX);
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

    /// Move the cursor forward by `n` bytes without going through `i64`.
    pub fn advance(&mut self, n: u64) -> io::Result<()> {
        let pos = self.pos.saturating_add(n);
        self.seek(SeekFrom::Start(pos)).map(|_| ())
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
            if self.advance(count).is_ok() {
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

fn add_signed(base: u64, delta: i64) -> io::Result<u64> {
    if delta >= 0 {
        Ok(base.saturating_add(delta as u64))
    } else {
        base.checked_sub(delta.unsigned_abs())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "seek before start"))
    }
}

impl Read for ByteSource<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if let Some(data) = self.memory.as_slice() {
            let pos = usize::try_from(self.pos).unwrap_or(usize::MAX);
            if pos >= data.len() {
                return Ok(0);
            }
            let n = (data.len() - pos).min(buf.len());
            buf[..n].copy_from_slice(&data[pos..pos + n]);
            self.pos += n as u64;
            return Ok(n);
        }
        loop {
            match self.inner.read(buf) {
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
                Ok(n) => {
                    self.pos += n as u64;
                    return Ok(n);
                }
            }
        }
    }
}

impl Seek for ByteSource<'_> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        if let Some(data) = self.memory.as_slice() {
            let len = data.len() as u64;
            let next = match pos {
                SeekFrom::Start(p) => p,
                SeekFrom::End(p) => add_signed(len, p)?,
                SeekFrom::Current(p) => add_signed(self.pos, p)?,
            };
            self.pos = next;
            return Ok(self.pos);
        }
        loop {
            match self.inner.seek(pos) {
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
                Ok(p) => {
                    self.pos = p;
                    return Ok(p);
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "source_tests.rs"]
mod source_tests;
