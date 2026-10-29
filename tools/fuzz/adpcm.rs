#![no_main]
mod common;
use libfuzzer_sys::fuzz_target;

fn chunk(out: &mut Vec<u8>, tag: &[u8; 4], bytes: &[u8]) {
    out.extend_from_slice(tag);
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(bytes);
}

fn wave(ima: bool, channels: u16, coefs: &[u8], fact: Option<u32>, body: &[u8]) -> Vec<u8> {
    let mut fmt = Vec::new();
    for n in [if ima { 17u16 } else { 2 }, channels] {
        fmt.extend_from_slice(&n.to_le_bytes());
    }
    let spb = if ima { 9u16 } else { 4 };
    for n in [8000u32, 8000 * u32::from(8 * channels) / u32::from(spb)] {
        fmt.extend_from_slice(&n.to_le_bytes());
    }
    for n in [8 * channels, 4, if ima { 2 } else { 8 }, spb] {
        fmt.extend_from_slice(&n.to_le_bytes());
    }
    if !ima {
        fmt.extend_from_slice(&1u16.to_le_bytes());
        fmt.extend_from_slice(coefs);
    }
    let mut wav = b"RIFF\0\0\0\0WAVE".to_vec();
    chunk(&mut wav, b"fmt ", &fmt);
    if let Some(n) = fact {
        chunk(&mut wav, b"fact", &n.to_le_bytes());
    }
    chunk(&mut wav, b"data", body);
    let size = wav.len() as u32 - 8;
    wav[4..8].copy_from_slice(&size.to_le_bytes());
    wav
}

fuzz_target!(|input: &[u8]| {
    if input.len() < 10 {
        return;
    }
    let ima = input[0] & 1 != 0;
    let channels = 1 + u16::from(input[1] & 1);
    let block = usize::from(8 * channels);
    let spb = if ima { 9 } else { 4 };
    let mut body = vec![0; block * (1 + usize::from(input[6] % 16))];
    for (i, b) in body.iter_mut().enumerate() {
        *b = input[7 + i % (input.len() - 7)];
    }
    if input[0] & 4 != 0 {
        for b in body.chunks_exact_mut(block) {
            for c in 0..usize::from(channels) {
                if ima {
                    b[c * 4 + 2] %= 89;
                } else {
                    b[c] = 0;
                }
            }
        }
    }
    let fact = (input[0] & 2 != 0).then_some(u32::from(input[7]));
    let full = wave(ima, channels, &input[2..6], fact, &body);
    let actual = common::check(&full);
    if let Some((audio, position)) = &actual {
        let data_pos = ryf::probe(&mut ryf::ByteSource::from_slice(&full))
            .expect("generated fmt")
            .data_pos;
        assert_eq!(
            *position,
            data_pos + (audio.frames().div_ceil(spb) * block) as u64
        );
    }
    if let Some(n) = fact.filter(|n| *n > 0) {
        let needed = (n as usize).div_ceil(spb) * block;
        if needed <= body.len() {
            let prefix = wave(ima, channels, &input[2..6], fact, &body[..needed]);
            if let Some((expected, _)) = common::check(&prefix) {
                let (audio, _) = actual.expect("unused ADPCM suffix changed a successful decode");
                assert_eq!(audio.sample_rate, expected.sample_rate);
                common::same_planes(&audio.channels, &expected.channels);
            }
        }
    }
});
