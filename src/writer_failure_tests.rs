use crate::{FormatKind, WavError, WavWriter, WriteSpec};
use std::cell::Cell;
use std::io::{self, Cursor, Read, Seek, SeekFrom, Write};
use std::rc::Rc;

struct Faulty {
    bytes: Cursor<Vec<u8>>,
    fail_after: Rc<Cell<Option<usize>>>,
    fail_seek: Rc<Cell<bool>>,
    fail_flush: Rc<Cell<bool>>,
    calls: Rc<Cell<usize>>,
    fail_read: Rc<Cell<bool>>,
}
impl Read for Faulty {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        self.calls.set(self.calls.get() + 1);
        if self.fail_read.replace(false) {
            return Err(io::Error::other("injected read"));
        }
        self.bytes.read(b)
    }
}
impl Seek for Faulty {
    fn seek(&mut self, p: SeekFrom) -> io::Result<u64> {
        self.calls.set(self.calls.get() + 1);
        if self.fail_seek.replace(false) {
            Err(io::Error::other("injected seek"))
        } else {
            self.bytes.seek(p)
        }
    }
}
impl Write for Faulty {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.calls.set(self.calls.get() + 1);
        if let Some(n) = self.fail_after.get() {
            if n == 0 {
                self.fail_after.set(None);
                return Err(io::Error::other("injected write"));
            }
            let take = n.min(b.len());
            self.fail_after.set(Some(n - take));
            let n = take;
            return self.bytes.write(&b[..n]);
        }
        self.bytes.write(b)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.calls.set(self.calls.get() + 1);
        if self.fail_flush.replace(false) {
            Err(io::Error::other("injected flush"))
        } else {
            Ok(())
        }
    }
}
fn invalid<T>(r: crate::Result<T>) {
    assert!(matches!(
        r,
        Err(WavError::Format(FormatKind::InvalidOperation))
    ));
}

#[test]
fn writer_failure_is_terminal_and_drop_does_not_mutate() -> crate::Result<()> {
    for step in 0..4 {
        let budget = Rc::new(Cell::new(None));
        let seek = Rc::new(Cell::new(false));
        let flush = Rc::new(Cell::new(false));
        let calls = Rc::new(Cell::new(0));
        let mut sink = Faulty {
            bytes: Cursor::new(Vec::new()),
            fail_after: budget.clone(),
            fail_seek: seek.clone(),
            fail_flush: flush.clone(),
            calls: calls.clone(),
            fail_read: Rc::new(Cell::new(false)),
        };
        let snapshot;
        let calls_before_drop;
        {
            let mut w = WavWriter::new(&mut sink, WriteSpec::u8(8000, 1))?;
            w.write_pcm(&[0x11])?;
            match step {
                0 => {
                    budget.set(Some(1));
                    assert!(w.write_pcm(&[0x22, 0x33]).is_err());
                }
                1 => {
                    budget.set(Some(0));
                    assert!(w.finalize().is_err());
                }
                2 => {
                    seek.set(true);
                    assert!(w.finalize().is_err());
                }
                _ => {
                    flush.set(true);
                    assert!(w.finalize().is_err());
                }
            }
            invalid(w.write_pcm(&[0x44]));
            invalid(w.finalize());
            // Read through the shared cursor after the writer is dropped below.
            snapshot = w.data_bytes();
            calls_before_drop = calls.get();
        }
        assert_eq!(calls.get(), calls_before_drop);
        assert_eq!(snapshot, 1);
        if step == 0 {
            assert_eq!(&sink.bytes.get_ref()[44..], &[0x11, 0x22]);
        }
    }
    Ok(())
}

#[test]
fn invalid_spec_and_origin_preserve_existing_data() -> crate::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("existing.wav");
    for create in [
        WavWriter::create,
        WavWriter::create_rf64,
        WavWriter::create_rifx,
        WavWriter::create_extensible,
    ] {
        for spec in [WriteSpec::s16(0, 1), WriteSpec::f32(u32::MAX, 26)] {
            std::fs::write(&path, b"KEEP")?;
            assert!(create(&path, spec).is_err());
            assert_eq!(std::fs::read(&path)?, b"KEEP");
        }
    }
    let mut cursor = Cursor::new(vec![0x77; 8]);
    cursor.set_position(8);
    assert!(WavWriter::new(&mut cursor, WriteSpec::s16(8000, 1)).is_err());
    assert_eq!(cursor.into_inner(), vec![0x77; 8]);
    Ok(())
}

#[test]
fn file_writer_can_promote_and_append_after_relocation() -> crate::Result<()> {
    let dir = tempfile::tempdir()?;
    for ext in [false, true] {
        let path = dir.path().join("promoted.wav");
        let mut w = if ext {
            WavWriter::create_extensible(&path, WriteSpec::s16(8000, 1))?
        } else {
            WavWriter::create(&path, WriteSpec::s16(8000, 1))?
        };
        w.write_pcm(&[1, 2])?;
        w.force_rf64()?;
        w.write_pcm(&[3, 4])?;
        w.finalize()?;
        assert_eq!(crate::read_s16(&path)?.1, &[1, 2, 3, 4]);
    }
    Ok(())
}

#[test]
fn promotion_failure_is_terminal() -> crate::Result<()> {
    for failure in 0..3 {
        let read = Rc::new(Cell::new(false));
        let seek = Rc::new(Cell::new(false));
        let budget = Rc::new(Cell::new(None));
        let calls = Rc::new(Cell::new(0));
        let mut sink = Faulty {
            bytes: Cursor::new(Vec::new()),
            fail_after: budget.clone(),
            fail_seek: seek.clone(),
            fail_flush: Rc::new(Cell::new(false)),
            calls: calls.clone(),
            fail_read: read.clone(),
        };
        let before;
        {
            let mut writer = WavWriter::new(&mut sink, WriteSpec::s16(8000, 1))?;
            writer.write_pcm(&[1, 2])?;
            match failure {
                0 => read.set(true),
                1 => seek.set(true),
                _ => budget.set(Some(1)),
            }
            assert!(writer.force_rf64().is_err());
            invalid(writer.write_pcm(&[3, 4]));
            invalid(writer.finalize());
            before = calls.get();
        }
        assert_eq!(calls.get(), before);
    }
    Ok(())
}
