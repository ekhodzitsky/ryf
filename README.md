# ryf

just wav.

Pure-Rust **WAVE family** codec. Zero crates on the default feature set.
Read RIFF / RIFX / RF64 / BW64 / Sony Wave64. Write RIFF PCM 8/16/24/32 +
IEEE f32; **RF64** when `u32` overflows. Planar `f32` out, native rate.
No resample.

[![license](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)
[![rustc](https://img.shields.io/badge/rustc-1.88+-lightgrey.svg)](Cargo.toml)
[![deps](https://img.shields.io/badge/deps-zero-success.svg)](Cargo.toml)

[hound](https://github.com/ruuda/hound) is PCM/IEEE RIFF. [symphonia](https://github.com/pdeljanov/Symphonia)
is a multi-format pipeline. [wavers](https://github.com/jmg049/wavers) is a later
PCM/IEEE reader. **ryf is the WAVE family crate**: G.711, MS/IMA ADPCM, RF64,
Wave64, extensible GUIDs, wild headers, hard RAM/duration caps.

| | ryf | hound | symphonia | wavers |
|---|---|---|---|---|
| Containers | RIFF, RIFX, RF64/BW64, W64 | RIFF | via codecs | RIFF |
| G.711 / ADPCM | yes / yes | no / no | yes / limited | no / no |
| Write | RIFF + RF64 PCM + f32 | PCM/IEEE | no | PCM/IEEE (path) |
| Default deps | **none** | none | several | several |
| Caps | yes | no | no | no |

[compare](docs/compare.md) · [benchmarks](docs/benchmarks.md) ·
[read](docs/read.md) · [write](docs/write.md) · [api](docs/api.md) ·
[correctness](docs/correctness.md)

## Install

```toml
[dependencies]
ryf = { git = "https://github.com/ekhodzitsky/ryf" }
```

rustc **1.88**. Not on crates.io (`publish = false`). Default features
(`adpcm` + `simd`) pull **no crates**.

```rust
fn main() -> ryf::Result<()> {
    let pcm = ryf::f32_to_s16le(&[0.25, -0.5, 0.0]);
    let path = std::env::temp_dir().join(format!("ryf-readme-{}.wav", std::process::id()));
    ryf::write_s16(&path, &pcm, 16_000)?;
    let wav = ryf::read(&path)?;
    assert_eq!(wav.sample_rate, 16_000);
    assert_eq!(wav.frames(), 3);
    let _ = std::fs::remove_file(&path);
    Ok(())
}
```

`read` / `decode_bytes` / `decode_g711` / `encode` / `WavWriter`.
`cargo run --example decode`.

## Speed

Apple Silicon, 2 s @ 16 kHz **in-memory** RIFF → mixed planar f32.
PCM16 mono: **3.78 µs** vs hound 193 µs (**51×**) / Symphonia 103 µs (**27×**) /
wavers 18.2 µs (**4.8×**). Encode f32: **3.29 µs** vs hound 44 µs (**13×**).
Cache-hot STT clip, not a file on disk. Linux x86 not measured.
Full table: [docs/benchmarks.md](docs/benchmarks.md).

ffmpeg is a **test oracle**, not a runtime dep.
[docs/correctness.md](docs/correctness.md).

## Not this

ADPCM / RIFX encode. GSM, MPEG-in-WAV. Resample. Async. `mmap`.

MIT OR Apache-2.0. [CHANGELOG](CHANGELOG.md).
