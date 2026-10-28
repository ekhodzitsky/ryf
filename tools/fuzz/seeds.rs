//! Deterministic public seeds. Generated corpora stay under target.
use std::{fs, io, path::Path};

fn chunk(tag: &[u8; 4], body: &[u8], be: bool) -> Vec<u8> {
    let mut out = tag.to_vec();
    let size = body.len() as u32;
    out.extend_from_slice(&if be {
        size.to_be_bytes()
    } else {
        size.to_le_bytes()
    });
    out.extend_from_slice(body);
    if !body.len().is_multiple_of(2) {
        out.push(0);
    }
    out
}

fn pcm_fmt(be: bool) -> Vec<u8> {
    let mut fmt = Vec::new();
    for n in [1u16, 1] {
        fmt.extend_from_slice(&if be { n.to_be_bytes() } else { n.to_le_bytes() });
    }
    for n in [8000u32, 16000] {
        fmt.extend_from_slice(&if be { n.to_be_bytes() } else { n.to_le_bytes() });
    }
    for n in [2u16, 16] {
        fmt.extend_from_slice(&if be { n.to_be_bytes() } else { n.to_le_bytes() });
    }
    fmt
}

fn container_seeds(dir: &Path) -> io::Result<()> {
    let data = [-32768i16, -1, 0, 32767];
    for be in [false, true] {
        let fmt = pcm_fmt(be);
        let data: Vec<u8> = data
            .iter()
            .flat_map(|n| if be { n.to_be_bytes() } else { n.to_le_bytes() })
            .collect();
        let mut body = b"WAVE".to_vec();
        body.extend(chunk(b"fmt ", &fmt, be));
        body.extend(chunk(b"data", &data, be));
        let mut wav = if be { b"RIFX" } else { b"RIFF" }.to_vec();
        wav.extend_from_slice(&if be {
            (body.len() as u32).to_be_bytes()
        } else {
            (body.len() as u32).to_le_bytes()
        });
        wav.extend(body);
        fs::write(dir.join(if be { "pcm-rifx" } else { "pcm-riff" }), wav)?;
    }
    let fmt = pcm_fmt(false);
    let data: Vec<u8> = data.iter().flat_map(|n| n.to_le_bytes()).collect();
    let mut ds64 = Vec::new();
    for n in [80u64, 8, 4] {
        ds64.extend_from_slice(&n.to_le_bytes());
    }
    ds64.extend_from_slice(&0u32.to_le_bytes());
    let mut rf64 = b"RF64\xff\xff\xff\xffWAVE".to_vec();
    rf64.extend(chunk(b"ds64", &ds64, false));
    rf64.extend(chunk(b"fmt ", &fmt, false));
    rf64.extend_from_slice(b"data\xff\xff\xff\xff");
    rf64.extend_from_slice(&data);
    fs::write(dir.join("pcm-rf64"), rf64)?;
    let tail = [
        0xf3, 0xac, 0xd3, 0x11, 0x8c, 0xd1, 0, 0xc0, 0x4f, 0x8e, 0xdb, 0x8a,
    ];
    let mut wave64 = vec![
        0x72, 0x69, 0x66, 0x66, 0x2e, 0x91, 0xcf, 0x11, 0xa5, 0xd6, 0x28, 0xdb, 4, 0xc1, 0, 0,
    ];
    wave64.extend_from_slice(&112u64.to_le_bytes());
    wave64.extend_from_slice(b"wave");
    wave64.extend_from_slice(&tail);
    for (tag, body) in [(b"fmt ", fmt), (b"data", data)] {
        wave64.extend_from_slice(tag);
        wave64.extend_from_slice(&tail);
        wave64.extend_from_slice(&(24 + body.len() as u64).to_le_bytes());
        wave64.extend_from_slice(&body);
        wave64.resize(wave64.len() + (8 - body.len() % 8) % 8, 0);
    }
    fs::write(dir.join("pcm-wave64"), wave64)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::args()
        .nth(1)
        .ok_or_else(|| io::Error::other("expected corpus directory"))?;
    let root = Path::new(&root);
    for name in ["containers", "adpcm"] {
        fs::create_dir_all(root.join(name))?;
    }
    container_seeds(&root.join("containers"))?;
    for ima in [0, 1] {
        for stereo in [0, 1] {
            let mut seed = [0u8; 32];
            seed[0] = 6 | ima;
            seed[1] = stereo;
            seed[6] = 2;
            seed[7] = if ima != 0 { 9 } else { 4 };
            fs::write(
                root.join("adpcm").join(format!("boundary-{ima}-{stereo}")),
                seed,
            )?;
        }
    }
    for name in [
        "ms_adpcm_16k_mono.wav",
        "ms_adpcm_16k_stereo.wav",
        "ima_adpcm_16k_mono.wav",
        "ima_adpcm_16k_stereo.wav",
    ] {
        let mut bytes = fs::read(Path::new("fixtures").join(name))?;
        let data_pos = ryf::probe(&mut ryf::ByteSource::from_slice(&bytes))?.data_pos as usize;
        let fact = bytes[..data_pos]
            .windows(12)
            .position(|w| &w[..8] == b"fact\x04\0\0\0")
            .ok_or_else(|| io::Error::other("fixture needs a fact count"))?;
        // Keep the checked-in bitstream, with a logical duration within fuzz caps.
        bytes[fact + 8..fact + 12].copy_from_slice(&128u32.to_le_bytes());
        let audio = ryf::decode_bytes(&bytes, ryf::DecodeOptions::default())?;
        assert_eq!(audio.frames(), 128);
        fs::write(root.join("containers").join(name), bytes)?;
    }
    for name in ["pcm-riff", "pcm-rifx", "pcm-rf64", "pcm-wave64"] {
        let bytes = fs::read(root.join("containers").join(name))?;
        let audio = ryf::decode_bytes(&bytes, ryf::DecodeOptions::default())?;
        assert_eq!(audio.sample_rate, 8000);
        assert_eq!(
            audio.channels[0],
            [-1.0, -1.0 / 32768.0, 0.0, 32767.0 / 32768.0]
        );
    }
    Ok(())
}
