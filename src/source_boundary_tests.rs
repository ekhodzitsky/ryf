use super::*;
use std::io::{self, Cursor};

struct Faulty {
    data: Cursor<Vec<u8>>,
    deny_rewind: bool,
    deny_payload: bool,
}
impl Read for Faulty {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        if self.deny_payload && self.data.position() >= 44 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "payload read",
            ));
        }
        self.data.read(b)
    }
}
impl Seek for Faulty {
    fn seek(&mut self, p: SeekFrom) -> io::Result<u64> {
        if self.deny_rewind && matches!(p, SeekFrom::Start(0)) {
            return Err(io::Error::other("rewind denied"));
        }
        self.data.seek(p)
    }
}
#[test]
fn fallible_source_reports_rewind_and_payload_errors() -> Result<()> {
    let bytes = crate::encode_s16(&[1, 2], 8000)?;
    let reader = Faulty {
        data: Cursor::new(bytes.clone()),
        deny_rewind: true,
        deny_payload: false,
    };
    assert!(ByteSource::try_from_read_seek(reader, Some(bytes.len() as u64)).is_err());
    let reader = Faulty {
        data: Cursor::new(bytes.clone()),
        deny_rewind: false,
        deny_payload: true,
    };
    let mut src = ByteSource::try_from_read_seek(reader, Some(bytes.len() as u64))?;
    assert!(
        matches!(decode_s16_from(&mut src),Err(WavError::Io(e)) if e.kind()==io::ErrorKind::PermissionDenied)
    );
    Ok(())
}
#[test]
fn incremental_g726_drains_complete_pending_words() {
    let mut d = crate::G726Decoder::new(crate::G726Rate::Kbps16, crate::G726Packing::Rfc3551);
    assert_eq!(d.push(&[0xff], &mut [0; 1]), (1, 1));
    assert_eq!(d.pending_bits(), 6);
    assert_eq!(d.push(&[], &mut [0; 3]), (3, 0));
    assert_eq!(d.finish(), 0);
    assert_eq!(d.push(&[0xff], &mut [0; 1]), (1, 1));
    assert_eq!(d.finish(), 6);
}
#[test]
fn callback_error_is_reported_and_rewind_allows_retry() -> Result<()> {
    let bytes = crate::encode_s16(&vec![0; 300_000], 8000)?;
    for memory in [false, true] {
        let mut src = if memory {
            ByteSource::from_slice(&bytes)
        } else {
            ByteSource::try_from_read_seek(Cursor::new(bytes.clone()), None)?
        };
        assert!(matches!(
            decode_streaming(&mut src, &DecodeOptions::default(), |_| Err(
                WavError::Empty
            )),
            Err(WavError::Empty)
        ));
        src.seek(SeekFrom::Start(0))?;
        assert_eq!(
            decode_with(&mut src, &DecodeOptions::default())?.frames(),
            150_000
        );
    }
    Ok(())
}

struct LargeWave {
    header: Vec<u8>,
    position: u64,
    length: u64,
}
impl Read for LargeWave {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let n = (self.length.saturating_sub(self.position)).min(bytes.len() as u64) as usize;
        bytes[..n].fill(0);
        if self.position < self.header.len() as u64 {
            let start = self.position as usize;
            let count = n.min(self.header.len() - start);
            bytes[..count].copy_from_slice(&self.header[start..start + count]);
        }
        self.position += n as u64;
        Ok(n)
    }
}
impl Seek for LargeWave {
    fn seek(&mut self, offset: SeekFrom) -> io::Result<u64> {
        let next = match offset {
            SeekFrom::Start(n) => i128::from(n),
            SeekFrom::Current(n) => i128::from(self.position) + i128::from(n),
            SeekFrom::End(n) => i128::from(self.length) + i128::from(n),
        };
        self.position = u64::try_from(next).map_err(|_| io::ErrorKind::InvalidInput)?;
        Ok(self.position)
    }
}
#[test]
fn streaming_large_interleaved_extent_uses_only_block_sized_arithmetic() -> Result<()> {
    let mut header = crate::encode(crate::WriteSpec::s16(8000, 26), &[])?;
    header[4..8].copy_from_slice(&0u32.to_le_bytes());
    header[40..44].copy_from_slice(&u32::MAX.to_le_bytes());
    let length = 8u64 * 1024 * 1024 * 1024;
    let mut source = ByteSource::try_from_read_seek(
        LargeWave {
            header,
            position: 0,
            length,
        },
        Some(length),
    )?;
    let opts = DecodeOptions::default()
        .with_channel_mode(ChannelMode::Mono)
        .with_max_duration_secs(1_000_000.0);
    let result = decode_streaming(&mut source, &opts, |block| {
        assert!(block.frames <= 262144 / 52);
        assert_eq!(block.planar.len(), 1);
        Err(WavError::Empty)
    });
    assert!(matches!(result, Err(WavError::Empty)));
    assert!(source.pos() < 300_000);
    Ok(())
}
