use ryf::{ByteSource, DecodeOptions, DecodedWav};

pub fn options() -> DecodeOptions {
    DecodeOptions::default()
        .with_max_duration_secs(0.1)
        .with_max_output_bytes(1024 * 1024)
}

pub fn same_planes(a: &[Vec<f32>], b: &[Vec<f32>]) {
    assert_eq!(a.len(), b.len());
    for (a, b) in a.iter().zip(b) {
        assert!(
            a.iter()
                .map(|s| s.to_bits())
                .eq(b.iter().map(|s| s.to_bits()))
        );
    }
}

pub fn check(data: &[u8]) -> Option<(DecodedWav, u64)> {
    let opts = options();
    let decoded = ryf::decode_bytes(data, opts.clone());
    let mut source =
        ByteSource::from_read_seek(std::io::Cursor::new(data), Some(data.len() as u64));
    let mut planes: Vec<Vec<f32>> = Vec::new();
    let streamed = ryf::decode_streaming(&mut source, &opts, |block| {
        if planes.is_empty() {
            planes.resize_with(block.planar.len(), Vec::new);
        }
        assert_eq!(planes.len(), block.planar.len());
        for (out, part) in planes.iter_mut().zip(block.planar) {
            out.extend_from_slice(part);
        }
        Ok(())
    });
    match (decoded, streamed) {
        (Ok(audio), Ok(info)) => {
            assert_eq!(audio.sample_rate, info.sample_rate);
            assert_eq!(audio.channels.len(), info.channels);
            assert_eq!(audio.frames(), info.frames);
            if audio.frames() == 0 {
                assert!(planes.is_empty());
            } else {
                // Compare every sample bit, including NaN payloads.
                same_planes(&audio.channels, &planes);
            }
            Some((audio, source.pos()))
        }
        (Err(a), Err(b)) => {
            assert_eq!(a.to_string(), b.to_string());
            None
        }
        (a, b) => panic!("collect/stream disagree: {a:?} {b:?}"),
    }
}
