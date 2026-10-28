# API

Output is **planar `f32`** at the file's native sample rate. No resample.

- `ChannelMode::Split` (library default on [`read`](read.md) /
  [`DecodeOptions::default`]): one `Vec<f32>` per channel, equal lengths.
- `ChannelMode::Mono`: one mixed track, left-to-right sum / n
  (speech ingest; [`read_speech`] / [`DecodeOptions::speech`]).

Streaming (`decode_streaming`) shares the convert kernels and keeps peak
RAM at ~256 KiB of source PCM plus one planar block.

Headerless telephony: `decode_g711` / `decode_g711_alaw` / `decode_g711_mulaw`,
`decode_g722` / `decode_g722_mono`, `decode_g726` / `decode_g726_mono`,
`decode_gsm` / `decode_gsm_mono`. G.722 is **64 kbit/s only** (56/48 not
decoded); output is always 16 kHz. WAVE tags `0x0065` (mmreg) and `0x028F`
(ffmpeg) are G.722. Tag `0x0064` is G.722 when the byte rate is 64 kbit/s
per the `Tag0064::Auto` rule; other layouts are `FormatKind::AmbiguousCodec`
unless `DecodeOptions::with_tag_0064` forces G.722 or G.726. G.726 is mono,
8000 Hz, 16/24/32/40 kbit/s. Headerless calls pass `G726Rate` and
`G726Packing` (`Rfc3551` or `Aal2`); WAVE tag `0x0045` is AAL2.
`G726Decoder::push` / `finish` / `reset` is the incremental state.
GSM is Microsoft GSM 06.10 / wav49, WAVE tag `0x0031`, 8 kHz, 65-byte
blocks. G.722, G.726, and GSM are always on (not the `adpcm` feature).
`probe` / `probe_with` / `sniff_wav`; probe codecs include
`ProbeCodec::G722`, `ProbeCodec::G726`, and `ProbeCodec::Gsm`. Pack
helpers: `f32_to_s16le` / `s16le_to_f32`.

## Caps

| | `DecodeOptions::speech()` / `read_speech` | `DecodeOptions::default()` / `unbounded` / `read` |
|---|---|---|
| Duration | 2 h | ~no practical ceiling |
| Sample rate | 192 kHz | 384 kHz |
| Planar f32 RAM | 4 GiB | `u64::MAX / 2` |
| Frame-budget rate | 48 kHz | 192 kHz |

The RAM cap counts logical planar output bytes. It does not bound peak RSS,
allocator capacity or scratch. `decode_reader` separately limits input to
`max_output_bytes + 1 MiB` and reads at most one extra byte to detect overflow.
That input limit can reject a large file whose mixed output fits the planar
budget. Collect paths use bounded conversion scratch alongside the final
output; streaming avoids allocating the whole output.

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
| `Format(FormatKind)` | broken chunk walk (typed, `non_exhaustive`) |
| `UnsupportedSampleRate` | rate 0 or above the configured ceiling |
| `TooLong` | actual PCM longer than `max_duration_secs` |
| `OutputTooLarge` | planar f32 would exceed `max_output_bytes` |
| `StreamLengthUnknown` | decode needs a known length |
| `FeatureDisabled` | ADPCM file, `adpcm` feature off |
| `OddPcm` | PCM byte length is not a whole number of frames |
| `Empty` | `decode_f32` / `decode_s16` / headerless telephony on zero samples. Planar `read` / `decode_bytes` return 0 frames. |
| `RiffTooLarge` | size overflow when RF64 is not available |

`FormatKind`: Truncated, MalformedFmt, MalformedChunk, MissingChunk,
ChannelLayout, InvalidSize, UnsupportedWaveFormat, Adpcm, InvalidOperation,
AmbiguousCodec (tag `0x0064` with no 64 kbit/s G.722 signature and no
`Tag0064` override). The enum is `non_exhaustive`.

`is_format_class()` is the NotWave / Format / StreamLengthUnknown / OddPcm /
Empty bucket for higher layers.

## Versioning

0.7 is the 1.0-prep line. `WavError`, `FormatKind`, `WriteFormat`,
`ProbeCodec`, `G726Rate`, `G726Packing`, `Tag0064`, `G726Decoder`, and
public option/result structs are `non_exhaustive` so 1.0 can add variants
or fields. `ChannelMode` and `G711Law` stay closed.
`ByteSource` has fallible `try_from_file` / `try_from_read_seek` constructors
for strict initialization; the original constructors remain best-effort.
`ByteSource` is exported at the crate root; the `source` module is
crate-private. Construct `DecodeOptions` / `WriteSpec` with the builders.

The [pre-1.0 contract review](api-stability.md) records error, limit, source
and writer lifecycle policies. Probe checks metadata and codec/rate support;
duration and output-byte budgets are enforced when decoding.
