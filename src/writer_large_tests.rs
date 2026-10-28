//! Real disk boundary checks; no sparse files or forced promotion hooks.
use crate::{WavWriter, WriteSpec};
use std::cell::Cell;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::rc::Rc;

fn pattern(bytes: &mut [u8], offset: u64) {
    let value = |index: u64| {
        (index ^ 0x9e37_79b9_7f4a_7c15)
            .wrapping_mul(0xbf58_476d_1ce4_e5b9)
            .to_le_bytes()
    };
    let skip = (offset & 7) as usize;
    let prefix = if skip == 0 {
        0
    } else {
        (8 - skip).min(bytes.len())
    };
    bytes[..prefix].copy_from_slice(&value(offset / 8)[skip..skip + prefix]);
    for (i, word) in bytes[prefix..].chunks_mut(8).enumerate() {
        word.copy_from_slice(&value((offset + prefix as u64) / 8 + i as u64)[..word.len()]);
    }
}

fn magic(path: &std::path::Path) -> std::io::Result<[u8; 4]> {
    let mut tag = [0; 4];
    File::open(path)?.read_exact(&mut tag)?;
    Ok(tag)
}

fn fill<W: Read + Write + Seek>(
    writer: &mut WavWriter<W>,
    before: u64,
    expected: &mut [u8],
) -> crate::Result<()> {
    let mut offset = 0;
    while offset < before {
        let n = (before - offset).min(expected.len() as u64) as usize;
        pattern(&mut expected[..n], offset);
        writer.write_pcm(&expected[..n])?;
        offset += n as u64;
    }
    Ok(())
}

#[test]
#[ignore = "writes and verifies two files over 4 GiB; requires 5 GiB free disk"]
fn real_riff_and_extensible_cross_4gib() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir_in("target")?;
    for ext in [false, true] {
        let path = dir.path().join("boundary.wav");
        let spec = WriteSpec::s16(48000, 1);
        let empty = if ext {
            crate::encode_extensible(spec, &[])?
        } else {
            crate::encode(spec, &[])?
        };
        let before = (u64::from(u32::MAX) - (empty.len() as u64 - 8)) & !1;
        let mut writer = if ext {
            WavWriter::create_extensible(&path, spec)?
        } else {
            WavWriter::create(&path, spec)?
        };
        let mut expected = vec![0; 1024 * 1024];
        fill(&mut writer, before, &mut expected)?;
        assert_eq!(magic(&path)?, *b"RIFF");
        // Crossing the actual size threshold must invoke the public promotion path.
        // Continue past 2^32 payload bytes, beyond both container-size thresholds.
        let total = (1u64 << 32) + 16;
        let n = (total - before) as usize;
        pattern(&mut expected[..n], before);
        writer.write_pcm(&expected[..n])?;
        assert_eq!(magic(&path)?, *b"RF64");
        assert_eq!(writer.data_bytes(), total);
        writer.finalize()?;
        writer.finalize()?;
        drop(writer);

        let mut source = crate::ByteSource::try_from_file(File::open(&path)?)?;
        let header = crate::header::parse_header(&mut source)?;
        assert_eq!(header.declared_data_len, Some(total));
        assert_eq!(header.declared_sample_count, Some(total / 2));
        assert_eq!(std::fs::metadata(&path)?.len(), header.data_pos + total);
        let mut file = File::open(&path)?;
        file.seek(SeekFrom::Start(12))?;
        let mut ds64 = [0; 36];
        file.read_exact(&mut ds64)?;
        assert_eq!(&ds64[..4], b"ds64");
        assert_eq!(
            u64::from_le_bytes(ds64[8..16].try_into()?),
            file.metadata()?.len() - 8
        );
        file.seek(SeekFrom::Start(header.data_pos))?;
        let mut actual = vec![0; expected.len()];
        let mut offset = 0;
        while offset < total {
            let n = (total - offset).min(actual.len() as u64) as usize;
            file.read_exact(&mut actual[..n])?;
            pattern(&mut expected[..n], offset);
            assert_eq!(&actual[..n], &expected[..n], "payload offset {offset}");
            offset += n as u64;
        }
        assert_eq!(file.read(&mut actual[..1])?, 0);
        drop(file);
        drop(source);
        std::fs::remove_file(&path)?;
    }
    Ok(())
}

struct PartialRelocation {
    file: File,
    phase: Rc<Cell<u8>>,
    calls: Rc<Cell<usize>>,
}
impl Read for PartialRelocation {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.calls.set(self.calls.get() + 1);
        self.file.read(bytes)
    }
}
impl Seek for PartialRelocation {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.calls.set(self.calls.get() + 1);
        self.file.seek(position)
    }
}
impl Write for PartialRelocation {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.calls.set(self.calls.get() + 1);
        match self.phase.get() {
            1 => {
                self.phase.set(2);
                self.file.write(&bytes[..bytes.len().min(65535)])
            }
            2 => {
                self.phase.set(3);
                Err(io::Error::other("injected partial relocation failure"))
            }
            _ => self.file.write(bytes),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        self.calls.set(self.calls.get() + 1);
        self.file.flush()
    }
}

#[test]
#[ignore = "writes a real 4 GiB file; requires 5 GiB free disk"]
fn real_boundary_partial_relocation_is_terminal() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir_in("target")?;
    let phase = Rc::new(Cell::new(0));
    let calls = Rc::new(Cell::new(0));
    let sink = PartialRelocation {
        file: File::options()
            .read(true)
            .write(true)
            .create_new(true)
            .open(dir.path().join("fault.wav"))?,
        phase: phase.clone(),
        calls: calls.clone(),
    };
    let mut writer = WavWriter::new(sink, WriteSpec::s16(48000, 1))?;
    let before = (u64::from(u32::MAX) - 36) & !1;
    fill(&mut writer, before, &mut vec![0; 1024 * 1024])?;
    phase.set(1);
    assert!(matches!(
        writer.write_pcm(&[1, 2]),
        Err(crate::WavError::Io(_))
    ));
    assert_eq!(phase.get(), 3);
    assert_eq!(writer.data_bytes(), before);
    let stopped = calls.get();
    for result in [writer.write_pcm(&[3, 4]), writer.finalize()] {
        assert!(matches!(
            result,
            Err(crate::WavError::Format(crate::FormatKind::InvalidOperation))
        ));
    }
    drop(writer);
    assert_eq!(calls.get(), stopped);
    Ok(())
}
