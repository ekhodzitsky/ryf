# API

Output is **planar `f32`** at the file's native sample rate. No resample.

- `ChannelMode::Mono` (default): one mixed track, left-to-right sum / n
  (same arithmetic the ffmpeg oracle tests use).
- `ChannelMode::Split`: one `Vec<f32>` per channel, equal lengths.

Streaming (`decode_streaming`) shares the convert kernels and keeps peak
RAM at ~256 KiB of source PCM plus one planar block.

## Caps

| | `DecodeOptions::speech()` (default) | `DecodeOptions::unbounded()` |
|---|---|---|
| Duration | 2 h | ~no practical ceiling |
| Sample rate | 192 kHz | 384 kHz |
| Planar f32 RAM | 4 GiB | `u64::MAX / 2` |
| Frame-budget rate | 48 kHz | 192 kHz |

`unbounded` still uses a finite rate when sizing the frame budget so a
corrupt header cannot request petabytes from one multiply.
`DecodeOptions::product_stt` is a deprecated alias of `speech()`.

## Features

| Feature | Default | Effect |
|---|---|---|
| `adpcm` | yes | MS + IMA/DVI ADPCM (`0x0002` / `0x0011`) |
| `simd` | yes | NEON (aarch64) + SSE (x86) s16→f32, stereo mix/split, f32 copy. Bit-exact with the scalar kernels. |
| `bench-c` | no | Vendored dr_wav for Criterion. Needs a C compiler. **Not** used by the library. |

Default features pull **no crates**.

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
| `RiffTooLarge` | classic-RIFF writer would overflow `u32` (`encode` upgrades to RF64 instead) |

`is_format_class()` is the NotWave / Format / StreamLengthUnknown / OddPcm /
Empty bucket for higher layers.
