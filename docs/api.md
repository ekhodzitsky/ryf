# API

Output is **planar `f32`** at the file's native sample rate. No resample.

- `ChannelMode::Split` (library default on [`read`](read.md) /
  [`DecodeOptions::default`]): one `Vec<f32>` per channel, equal lengths.
- `ChannelMode::Mono`: one mixed track, left-to-right sum / n
  (speech ingest; [`read_speech`] / [`DecodeOptions::speech`]).

Streaming (`decode_streaming`) shares the convert kernels and keeps peak
RAM at ~256 KiB of source PCM plus one planar block.

Headerless telephony: `decode_g711` / `decode_g711_alaw` / `decode_g711_mulaw`,
`decode_g722` / `decode_g722_mono`, `decode_gsm` / `decode_gsm_mono`. G.722
is **64 kbit/s only** (56/48 not decoded); output is always 16 kHz. WAVE
tags `0x0064` (Asterisk/SBC), `0x0065` (mmreg), `0x028F` (ffmpeg). GSM is
Microsoft GSM 06.10 / wav49, WAVE tag `0x0031`, 8 kHz, 65-byte blocks.
G.722 and GSM are always on (not the `adpcm` feature). `probe` /
`probe_with` / `sniff_wav`; G.722 probe is `ProbeCodec::G722`, GSM is
`ProbeCodec::Gsm`. Pack helpers: `f32_to_s16le` / `s16le_to_f32`.

## Caps

| | `DecodeOptions::speech()` / `read_speech` | `DecodeOptions::default()` / `unbounded` / `read` |
|---|---|---|
| Duration | 2 h | ~no practical ceiling |
| Sample rate | 192 kHz | 384 kHz |
| Planar f32 RAM | 4 GiB | `u64::MAX / 2` |
| Frame-budget rate | 48 kHz | 192 kHz |

`unbounded` still uses a finite rate when sizing the frame budget so a
corrupt header cannot request petabytes from one multiply.

## Features

| Feature | Default | Effect |
|---|---|---|
| `adpcm` | yes | MS + IMA/DVI ADPCM (`0x0002` / `0x0011`) |
| `simd` | yes | NEON (aarch64) + SSE4.1/SSE2 (x86) s16 to f32, stereo mix/split, f32 copy. Bit-exact with the scalar kernels. |
| `bench-c` | no | Vendored dr_wav for Criterion. Needs a C compiler. **Not** used by the library. |

Default features pull **no crates**.

## Errors

Typed `WavError`. No `anyhow`, no `thiserror`.

| Variant | When |
|---|---|
| `Io` | `Read` / `Seek` / `Write` |
| `NotWave` | not a WAVE container |
| `UnsupportedCodec { tag }` | codec / subtype not implemented (`tag` is `wFormatTag`, or `0`) |
| `Format(FormatKind)` | broken chunk walk (closed set, no `String`) |
| `UnsupportedSampleRate` | rate 0 or above the configured ceiling |
| `TooLong` | actual PCM longer than `max_duration_secs` |
| `OutputTooLarge` | planar f32 would exceed `max_output_bytes` |
| `StreamLengthUnknown` | decode needs a known length |
| `FeatureDisabled` | ADPCM file, `adpcm` feature off |
| `OddPcm` | PCM byte length is not a whole number of frames |
| `Empty` | decode produced zero samples |
| `RiffTooLarge` | size overflow when RF64 is not available |

`FormatKind`: Truncated, MalformedFmt, MalformedChunk, MissingChunk,
ChannelLayout, InvalidSize, UnsupportedWaveFormat, Adpcm, InvalidOperation.

`is_format_class()` is the NotWave / Format / StreamLengthUnknown / OddPcm /
Empty bucket for higher layers.
