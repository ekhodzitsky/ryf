# ryf

just wav.

Pure-Rust **WAVE family** codec. Zero crates on the default feature set.
Read RIFF / RIFX / RF64 / BW64 / Sony Wave64. Write classic RIFF PCM16 and
IEEE f32. Output is planar `f32` at the file's native sample rate.

[![license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![rustc](https://img.shields.io/badge/rustc-1.88+-lightgrey.svg)](Cargo.toml)
[![deps](https://img.shields.io/badge/deps-zero-success.svg)](Cargo.toml)
[![changelog](https://img.shields.io/badge/changelog-Keep%20a%20Changelog-blue.svg)](CHANGELOG.md)

## Why

[hound](https://github.com/ruuda/hound) is the PCM/IEEE RIFF workhorse.
[symphonia](https://github.com/pdeljanov/Symphonia) is a multi-format
pipeline. **ryf is the WAVE family crate**: the containers and codecs that
show up in speech ingest (G.711, MS/IMA ADPCM, RF64, Wave64, extensible
GUIDs, wild headers), planar `f32` out, hard RAM/duration caps, no extra
dependencies.

| | ryf | hound | symphonia |
|---|---|---|---|
| Containers | RIFF, RIFX, RF64/BW64, Wave64 | RIFF (+ extensible) | via codecs |
| PCM 8/16/24/32, IEEE f32 | yes | yes | yes |
| IEEE f64 | yes | no | yes |
| G.711 A-law / µ-law | yes | no | yes |
| MS + IMA ADPCM | yes (`adpcm`) | no | limited |
| Write | RIFF PCM16 mono, f32 1–2 ch | PCM/IEEE, many depths | no |
| Default deps | **none** | none | several |
| Output | planar `f32`, native rate | typed sample iterator | decoded packets |
| Duration / RAM caps | yes (`speech` / `unbounded`) | no | no |
| Resample / mmap | no | no | no |

## Install

Private git. Default features (`adpcm` + `simd`) pull **no crates**.

```toml
[dependencies]
ryf = { git = "ssh://git@github.com/ekhodzitsky/ryf.git" }
```

```toml
# PCM/IEEE/G.711 only (no ADPCM, no SIMD)
ryf = { git = "ssh://git@github.com/ekhodzitsky/ryf.git", default-features = false }
```

Requires **Rust 1.88**, edition 2024.

## Quick start

Decode a buffer to mixed mono `f32`:

```rust
fn main() -> ryf::Result<()> {
    let pcm = ryf::f32_to_s16le(&[0.25, -0.5, 0.0]);
    let wav = ryf::encode_s16(&pcm, 16_000)?;
    let decoded = ryf::decode_bytes(&wav, ryf::DecodeOptions::speech())?;
    assert_eq!(decoded.sample_rate, 16_000);
    assert_eq!(decoded.channels.len(), 1); // Mono mixes
    assert_eq!(decoded.channels[0].len(), 3);
    Ok(())
}
```

Keep channels separate, sniff, probe, stream, write:

```rust
fn main() -> ryf::Result<()> {
    use ryf::{
        ByteSource, ChannelMode, DecodeOptions, decode_bytes, decode_f32, decode_streaming,
        encode_f32, encode_s16, f32_to_s16le, probe, sniff_wav,
    };

    let stereo = encode_f32(&[0.1, -0.1, 0.2, -0.2], 16_000, 2)?;
    let split = decode_bytes(
        &stereo,
        DecodeOptions::speech().with_channel_mode(ChannelMode::Split),
    )?;
    assert_eq!(split.channels.len(), 2);

    assert!(sniff_wav(&stereo));
    let info = probe(&mut ByteSource::from_slice(&stereo))?;
    assert_eq!(info.sample_rate, 16_000);

    let wav = encode_s16(&f32_to_s16le(&[0.1; 8]), 16_000)?;
    let mut src = ByteSource::from_slice(&wav);
    let mut frames = 0usize;
    let streamed = decode_streaming(&mut src, &DecodeOptions::speech(), |block| {
        frames += block.frames;
        Ok(())
    })?;
    assert_eq!(streamed.frames, frames);

    let (sr, mono) = decode_f32(&encode_f32(&[0.0, 0.5], 24_000, 1)?)?;
    assert_eq!(sr, 24_000);
    assert_eq!(mono.len(), 2);
    Ok(())
}
```

From a path: `read_s16` / `read_f32`. From a `File` or any `Read + Seek`:
`ByteSource::from_file` / `from_read_seek` + `decode_with`.

## Read

Every listed codec is accepted in every listed container unless noted.

| Codec | RIFF | RIFX (BE) | RF64 / BW64 | Wave64 |
|---|---|---|---|---|
| PCM u8 / s16 / s24 / s32 | yes | yes | yes | yes |
| S24 in 4-byte containers | yes | yes | yes | yes |
| IEEE f32 / f64 | yes | yes | yes | yes |
| G.711 A-law / µ-law | yes | yes | yes | yes |
| MS-ADPCM, IMA/DVI ADPCM | yes | — | yes | yes |

Also:

- `WAVE_FORMAT_EXTENSIBLE` (PCM / IEEE / G.711 + Ambisonic GUIDs)
- Wild headers: `valid_bits = 0`, empty channel mask, short `data`,
  streaming `u32::MAX` sizes, PCM fmt sizes 16 / 18 / 40
- `TooLong` is decided from **bytes on disk**, not a lying `fact` / `ds64`

## Write

Classic little-endian RIFF only. No RF64, no RIFX, no ADPCM encode.

| | Channels | Empty payload |
|---|---|---|
| `encode_s16` / `write_s16` — LE PCM16 | 1 | allowed |
| `encode_f32` — IEEE f32 | 1–2 | allowed |

Empty **decode** is still `WavError::Empty`. Empty **encode** is a valid
44-byte (PCM16) or 58-byte (f32) WAVE.

## Output

- **Planar `f32`**, native sample rate. No resample.
- `ChannelMode::Mono` (default): one mixed track, left-to-right sum / n.
- `ChannelMode::Split`: one `Vec<f32>` per channel, equal lengths.
- Decode scale for integer PCM is `/ 32768` (s16), `/ 2^23` (s24), `/ 2^31` (s32).
- Encode pack `f32_to_s16le` uses peak **32767** (`±1.0 → ±32767`). That is
  the molv write path; it is **not** the inverse of decode. Round-trip
  through encode then decode is lossless on *length and rate*, not bit-exact
  on sample values.

## Caps

| | `DecodeOptions::speech()` (default) | `DecodeOptions::unbounded()` |
|---|---|---|
| Duration | 2 h | ~no practical ceiling |
| Sample rate | 192 kHz | 384 kHz |
| Planar f32 RAM | 4 GiB | `u64::MAX / 2` |
| Frame-budget rate | 48 kHz | 192 kHz |

`unbounded` still uses a finite rate when sizing the frame budget so a
corrupt header cannot request petabytes from one multiply.
`product_stt()` is an alias of `speech()`.

Streaming (`decode_streaming`) shares the convert kernels and keeps peak
RAM at ~256 KiB of source PCM plus one planar block.

## Errors

Typed `WavError`. No `anyhow`, no `thiserror`.

| Variant | When |
|---|---|
| `Io` | `Read` / `Seek` / `Write` |
| `NotWave` / `Format` | not a WAVE container, or a broken chunk walk |
| `UnsupportedCodec` | codec / subtype / channel layout not implemented |
| `UnsupportedSampleRate` | rate 0 or above the configured ceiling |
| `TooLong` | actual PCM longer than `max_duration_secs` |
| `OutputTooLarge` | planar f32 would exceed `max_output_bytes` |
| `StreamLengthUnknown` | decode needs a known length |
| `FeatureDisabled` | ADPCM file, `adpcm` feature off |
| `OddPcm` | PCM byte length is not a whole number of frames |
| `Empty` | decode produced zero samples |
| `RiffTooLarge` | encode would not fit in a RIFF `u32` |

`is_format_class()` is the NotWave / Format / StreamLengthUnknown / OddPcm /
Empty bucket for higher layers.

## Features

| Feature | Default | Effect |
|---|---|---|
| `adpcm` | yes | MS + IMA/DVI ADPCM (`0x0002` / `0x0011`) |
| `simd` | yes | NEON (aarch64) + SSE (x86) s16→f32, stereo mix/split, f32 copy. Bit-exact with the scalar kernels. |

## Correctness

- Differential suite vs **ffmpeg** (bit-exact `f32` on lossless PCM / G.711)
  when `ffmpeg` is on `PATH`. ffmpeg is a **test oracle**, not a runtime dep.
- SIMD paths match scalar bit-for-bit.
- `unsafe` is confined to `convert/simd.rs` and uninit `f32` scratch (Copy,
  every element written before `Ok`); each block has a SAFETY comment.

```sh
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

## Non-goals

- RF64 / ADPCM / RIFX **encode**
- GSM, MPEG-in-WAV, and other exotic codecs
- Resampling
- Async I/O (sync demux; call from `spawn_blocking` in async apps)
- `mmap`, `zeroize`, `tracing`

## License

[MIT](LICENSE) © Evgeny Khodzitsky
