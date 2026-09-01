use super::*;
use crate::error::Result;
use std::io::{Read, Seek, SeekFrom, Write};

#[test]
fn slice_remaining_and_endian() -> Result<()> {
    let data = [1u8, 2, 3, 4, 5, 6, 7, 8];
    let mut s = ByteSource::from_slice(&data);
    assert_eq!(s.contiguous_slice(), Some(&data[..]));
    assert_eq!(s.read_u8()?, 1);
    assert_eq!(s.remaining_slice(), Some(&data[1..]));
    assert_eq!(s.read_be_u16()?, u16::from_be_bytes([2, 3]));
    assert_eq!(s.read_u16()?, u16::from_le_bytes([4, 5]));
    Ok(())
}

#[test]
fn ignore_and_file() -> Result<()> {
    let mut s = ByteSource::from_vec(vec![0u8; 100]);
    assert_eq!(s.contiguous_slice().map(|c| c.len()), Some(100));
    s.ignore_bytes(10)?;
    assert_eq!(s.pos(), 10);
    assert_eq!(s.remaining_slice().map(|c| c.len()), Some(90));

    let mut tmp = tempfile::NamedTempFile::new()?;
    tmp.write_all(b"abc")?;
    tmp.flush()?;
    let mut s = ByteSource::from_file(std::fs::File::open(tmp.path())?);
    assert_eq!(s.read_u8()?, b'a');
    Ok(())
}

#[test]
fn memory_seek_and_integer_helpers() -> Result<()> {
    let data: Vec<u8> = (0u8..32).collect();
    let mut s = ByteSource::from_vec(data);
    assert_eq!(s.byte_len(), Some(32));
    assert_eq!(s.read_quad_bytes()?, [0, 1, 2, 3]);
    assert_eq!(s.read_u32()?, u32::from_le_bytes([4, 5, 6, 7]));
    assert_eq!(s.read_be_u32()?, u32::from_be_bytes([8, 9, 10, 11]));
    s.seek(SeekFrom::Start(0))?;
    s.seek(SeekFrom::Current(4))?;
    assert_eq!(s.pos(), 4);
    s.seek(SeekFrom::End(-2))?;
    assert_eq!(s.read_u8()?, 30);
    assert!(s.seek(SeekFrom::Current(-100)).is_err());
    s.seek(SeekFrom::Start(32))?;
    assert_eq!(s.remaining_slice(), Some(&[][..]));
    assert_eq!(s.read(&mut [0u8; 4])?, 0);
    s.ignore_bytes(0)?;
    Ok(())
}

#[test]
fn ignore_bytes_seek_threshold_and_eof() -> Result<()> {
    let mut s = ByteSource::from_vec(vec![7u8; 20_000]);
    s.ignore_bytes(10_000)?;
    assert_eq!(s.pos(), 10_000);
    assert!(s.ignore_bytes(20_000).is_err());

    let mut tmp = tempfile::NamedTempFile::new()?;
    tmp.write_all(&vec![1u8; 16_000])?;
    tmp.flush()?;
    let mut s = ByteSource::from_file(std::fs::File::open(tmp.path())?);
    assert!(s.contiguous_slice().is_none());
    s.ignore_bytes(9_000)?;
    assert_eq!(s.read_u8()?, 1);
    assert!(s.ignore_bytes(100_000).is_err());
    Ok(())
}

#[test]
fn from_read_seek_and_new() -> Result<()> {
    use std::io::Cursor;
    let data = [9u8, 8, 7, 6];
    let mut s = ByteSource::from_read_seek(Cursor::new(data), Some(4));
    assert!(s.contiguous_slice().is_none());
    assert_eq!(s.read_u16()?, u16::from_le_bytes([9, 8]));
    s.seek(SeekFrom::Start(0))?;
    let mut boxed: ByteSource = ByteSource::new(Box::new(Cursor::new(data.to_vec())), Some(4));
    assert_eq!(boxed.read_u8()?, 9);
    Ok(())
}

struct SeekFail {
    data: Vec<u8>,
    pos: usize,
}

impl Read for SeekFail {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.pos >= self.data.len() {
            return Ok(0);
        }
        let n = (self.data.len() - self.pos).min(buf.len());
        buf[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

impl Seek for SeekFail {
    fn seek(&mut self, _: SeekFrom) -> std::io::Result<u64> {
        Err(std::io::Error::other("seek disabled"))
    }
}

struct EofRead;

impl Read for EofRead {
    fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
        Ok(0)
    }
}

impl Seek for EofRead {
    fn seek(&mut self, _: SeekFrom) -> std::io::Result<u64> {
        Ok(0)
    }
}

#[test]
fn ignore_bytes_seek_fail_falls_back_and_eof() -> Result<()> {
    let mut s = ByteSource::from_read_seek(
        SeekFail {
            data: vec![1u8; 12_000],
            pos: 0,
        },
        Some(12_000),
    );
    s.ignore_bytes(9_000)?;
    assert_eq!(s.read_u8()?, 1);

    let mut s = ByteSource::from_read_seek(EofRead, Some(100));
    assert!(s.ignore_bytes(16).is_err());
    Ok(())
}
