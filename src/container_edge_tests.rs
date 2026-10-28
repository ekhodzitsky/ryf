use crate::{ByteSource, DecodeOptions, decode_bytes, probe};
fn chunk(out: &mut Vec<u8>, tag: &[u8; 4], body: &[u8]) {
    out.extend_from_slice(tag);
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(body);
    if body.len() % 2 == 1 {
        out.push(0);
    }
}
fn rf64(entries: &[([u8; 4], u64)]) -> crate::Result<Vec<u8>> {
    let base = crate::encode_s16(&[0, 0], 8000)?;
    let mut ds = Vec::new();
    for n in [0u64, 2, 1] {
        ds.extend_from_slice(&n.to_le_bytes());
    }
    ds.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    for (tag, size) in entries {
        ds.extend_from_slice(tag);
        ds.extend_from_slice(&size.to_le_bytes());
    }
    let mut out = b"RF64\xff\xff\xff\xffWAVE".to_vec();
    chunk(&mut out, b"ds64", &ds);
    chunk(&mut out, b"fmt ", &base[20..36]);
    for (tag, size) in entries {
        out.extend_from_slice(tag);
        out.extend_from_slice(&u32::MAX.to_le_bytes());
        out.resize(out.len() + *size as usize, 0);
        if size % 2 == 1 {
            out.push(0);
        }
    }
    chunk(&mut out, b"data", &[0, 0]);
    let size = out.len() as u64 - 8;
    out[20..28].copy_from_slice(&size.to_le_bytes());
    Ok(out)
}
#[test]
fn rf64_size_entries_are_consumed_and_not_silently_truncated() -> crate::Result<()> {
    for entries in [
        vec![(*b"JUNK", 4), (*b"JUNK", 8)],
        (0..40u32).map(|i| (i.to_le_bytes(), 0)).collect(),
    ] {
        let bytes = rf64(&entries)?;
        assert_eq!(decode_bytes(&bytes, DecodeOptions::default())?.frames(), 1);
    }
    Ok(())
}
fn chunk64(out: &mut Vec<u8>, guid: &[u8; 16], body: &[u8]) {
    out.extend_from_slice(guid);
    out.extend_from_slice(&(24 + body.len() as u64).to_le_bytes());
    out.extend_from_slice(body);
    out.resize(out.len() + (8 - body.len() % 8) % 8, 0);
}
#[test]
fn wave64_keeps_first_format_even_if_later_format_is_invalid() -> crate::Result<()> {
    use crate::header::{W64_GUID_DATA, W64_GUID_FMT, W64_GUID_RIFF, W64_GUID_WAVE};
    let base = crate::encode_s16(&[1, 2], 8000)?;
    let mut bytes = W64_GUID_RIFF.to_vec();
    bytes.extend_from_slice(&0u64.to_le_bytes());
    bytes.extend_from_slice(&W64_GUID_WAVE);
    chunk64(&mut bytes, &W64_GUID_FMT, &base[20..36]);
    chunk64(&mut bytes, &W64_GUID_FMT, &[0]);
    chunk64(&mut bytes, &W64_GUID_DATA, &[1, 2]);
    let size = bytes.len() as u64;
    bytes[16..24].copy_from_slice(&size.to_le_bytes());
    assert_eq!(
        probe(&mut ByteSource::from_slice(&bytes))?.sample_rate,
        8000
    );
    assert_eq!(decode_bytes(&bytes, DecodeOptions::default())?.frames(), 1);
    Ok(())
}

#[test]
fn rf64_table_budget_is_explicit_and_validates_declared_entries() -> crate::Result<()> {
    let mut bytes = rf64(&vec![(*b"JUNK", 0); 4096])?;
    assert_eq!(decode_bytes(&bytes, DecodeOptions::default())?.frames(), 1);
    // tableLength is part of ds64; claiming records beyond its body is invalid.
    bytes[44..48].copy_from_slice(&4097u32.to_le_bytes());
    assert!(decode_bytes(&bytes, DecodeOptions::default()).is_err());
    let bytes = rf64(&vec![(*b"JUNK", 0); 4097])?;
    assert!(decode_bytes(&bytes, DecodeOptions::default()).is_err());
    Ok(())
}
