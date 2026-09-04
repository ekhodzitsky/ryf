# ryf

WAVE to planar `f32`.

[![crates.io](https://img.shields.io/crates/v/ryf.svg)](https://crates.io/crates/ryf)
[![docs.rs](https://img.shields.io/docsrs/ryf)](https://docs.rs/ryf)
[![ci](https://github.com/ekhodzitsky/ryf/actions/workflows/ci.yml/badge.svg)](https://github.com/ekhodzitsky/ryf/actions/workflows/ci.yml)
[![license](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)

A WAVE codec in Rust. Integer PCM, IEEE float, G.711, G.722, GSM, MS/IMA
ADPCM. RIFF, RIFX, RF64, BW64, Wave64. Wild headers. Output is planar
`f32` at the file's native sample rate. Default features pull no crates.
ffmpeg is a [test oracle](docs/correctness.md), not a runtime dep.

## Examples

Write a mono PCM16 file, then read it back:

```rust
fn main() -> ryf::Result<()> {
    let pcm = ryf::f32_to_s16le(&[0.25, -0.5, 0.0]);
    let bytes = ryf::encode_s16(&pcm, 16_000)?;
    let wav = ryf::decode_bytes(&bytes, ryf::DecodeOptions::default())?;
    assert_eq!(wav.sample_rate, 16_000);
    assert_eq!(wav.frames(), 3);
    Ok(())
}
```

From a path, `read` is one plane per channel (archival caps). Speech
ingest (mix to mono, 2 h / 4 GiB): `read_speech`.

```rust
fn main() -> ryf::Result<()> {
    let wav = ryf::read("in.wav")?;
    println!(
        "{} Hz, {} ch, {} frames",
        wav.sample_rate,
        wav.num_channels(),
        wav.frames()
    );
    Ok(())
}
```

Headerless telephony: `decode_g711`, `decode_g722`, `decode_gsm`.
Streaming: `decode_streaming`. Write to a path: `write_s16` / `WavWriter`.

## Decode

| | RIFF | RIFX | RF64 / BW64 | Wave64 |
|---|---|---|---|---|
| Integer PCM 8/16/24/32 | yes | yes | yes | yes |
| IEEE f32 / f64 | yes | yes | yes | yes |
| G.711 A-law / mu-law | yes | yes | yes | yes |
| G.722 64 kbit/s | yes | yes | yes | yes |
| GSM 06.10 / wav49 | yes | yes | yes | yes |
| MS / IMA ADPCM | yes | yes | yes | yes |
| `WAVEFORMATEXTENSIBLE` | PCM / IEEE / G.711 | no | yes | yes |
| Wild `fmt ` / short `data` | yes | yes | yes | yes |

## Write

Integer PCM 8/16/24/32, IEEE f32, G.711 A-law / mu-law. Classic RIFF, RF64
when `u32` sizes overflow, RIFX (`encode_rifx`), `WAVEFORMATEXTENSIBLE`
(`encode_extensible`). Channels `1..=26`. No ADPCM / G.722 / GSM encode.

## Compared to hound, Symphonia, dr_wav

In-process, same 2 s @ 16 kHz RIFF, Apple Silicon. Not a file on disk,
not Linux x86. Full matrix: [compare](docs/compare.md). Timings:
[benchmarks](docs/benchmarks.md).

| | ryf | hound 3.5 | Symphonia 0.5 | dr_wav 0.14 |
|---|---|---|---|---|
| Job | WAVE codec | PCM/IEEE RIFF | media pipeline | C WAV loader |
| Default deps | none | none | several | none (one `.h`) |
| RF64 / RIFX / Wave64 | yes | no | limited | RF64 + Wave64 |
| G.711 / ADPCM | yes | no | G.711 | yes |
| G.722 / GSM | yes | no | no | no |
| Caps (duration / RAM) | yes | no | no | no |
| PCM16 decode | **3.78 mus** | 186 mus | 102 mus | 16.3 mus |
| PCM16 encode | 5.72 mus | 112 mus | no | **5.42 mus** |

## Speed

In-memory 2 s @ 16 kHz RIFF to mixed planar `f32`. Apple Silicon (NEON).
**Not** a file on disk, **not** Linux x86 (SSE unmeasured). Encode s16
is a **tie** with dr_wav. Numbers: [benchmarks](docs/benchmarks.md).

| Workload | ryf | hound | symphonia | wavers | dr_wav |
|---|---|---|---|---|---|
| PCM16 mono to f32 | **3.78 mus** | 186 mus | 102 mus | 18.5 mus | 16.3 mus |
| PCM16 stereo mix | **9.74 mus** | 431 mus | 227 mus | 91.7 mus | 77.7 mus |
| IEEE f32 mono | **3.56 mus** | 204 mus | 106 mus | 19.0 mus | 9.39 mus |
| encode PCM16 mono | 5.72 mus | 112 mus | - | - | **5.42 mus** |
| encode IEEE f32 | **3.30 mus** | 43.9 mus | - | - | 9.12 mus |

## Install

```toml
[dependencies]
ryf = "0.6"
```

rustc **1.88**. Default features (`adpcm` + `simd`) pull **no crates**.

[read](docs/read.md) | [write](docs/write.md) | [api](docs/api.md) |
[benchmarks](docs/benchmarks.md) | [compare](docs/compare.md) |
[correctness](docs/correctness.md) | [CHANGELOG](CHANGELOG.md)

WAVE only: no resample, no player, no `mmap`. MIT OR Apache-2.0.
