# ryf

just wav.

Untrusted or telephony **WAVE** → planar `f32` in-process. Zero crates on
the default feature set, no C in the binary, no ffmpeg, no resample.
Read RIFF / RIFX / RF64 / BW64 / Sony Wave64, G.711, MS/IMA ADPCM, wild
headers. Write RIFF PCM 8/16/24/32 + IEEE f32; **RF64** when `u32`
overflows.

[![crates.io](https://img.shields.io/crates/v/ryf.svg)](https://crates.io/crates/ryf)
[![docs.rs](https://img.shields.io/docsrs/ryf)](https://docs.rs/ryf)
[![license](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)
[![rustc](https://img.shields.io/badge/rustc-1.88+-lightgrey.svg)](Cargo.toml)
[![deps](https://img.shields.io/badge/deps-zero-success.svg)](Cargo.toml)

**For:** speech ingest, phone recordings, user `.wav` uploads that
[hound](https://github.com/ruuda/hound) will not open (µ-law, ADPCM, RF64,
broken `fmt`). **Not for:** a player, a DAW, FLAC/MP3, resampling to 16 kHz.

[hound](https://github.com/ruuda/hound) is PCM/IEEE RIFF.
[symphonia](https://github.com/pdeljanov/Symphonia) is a media pipeline.
[dr_wav](https://github.com/mackron/dr_libs) is C. ffmpeg is a process.
Matrix: [compare](docs/compare.md).

## Read

`read` / `decode_bytes` — one plane per channel, archival caps.

`read_speech` / `DecodeOptions::speech()` — **mix to mono**, 2 h / 192 kHz /
4 GiB. Use this for STT or untrusted upload.

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

In-memory: `decode_bytes`. Headerless µ-law/A-law: `decode_g711`.
`impl Read` without seek: `decode_reader`. Write: `encode` / `write_s16` /
`WavWriter`.

[read](docs/read.md) · [write](docs/write.md) · [api](docs/api.md) ·
[benchmarks](docs/benchmarks.md) · [correctness](docs/correctness.md)

## Install

```toml
[dependencies]
ryf = "0.3"
```

rustc **1.88**. Default features (`adpcm` + `simd`) pull **no crates**.

## Speed

Apple Silicon (aarch64, NEON), 2 s @ 16 kHz **in-memory** RIFF → mixed
planar f32. Linux x86 / SSE not measured.
PCM16 mono: **3.78 µs** vs dr_wav 16.3 µs (**4.3×**) / wavers 18.5 (**4.9×**) /
Symphonia 102 (**27×**) / hound 186 (**49×**). Encode s16 is a **tie** with
dr_wav. Cache-hot clip, not a file on disk.
[docs/benchmarks.md](docs/benchmarks.md).

## Not this

ADPCM / RIFX / G.711 encode. GSM, MPEG-in-WAV. Resample. Async. `mmap`.
Not a player. ffmpeg is a **test oracle**, not a runtime dep.

MIT OR Apache-2.0. [CHANGELOG](CHANGELOG.md).
