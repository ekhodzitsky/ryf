# Compare

ryf is a **WAVE family ingest** crate: messy containers, telephony codecs,
planar `f32`, RAM/duration caps, **zero** default-feature deps.

It is **not** a player (rodio/cpal), not a resample pipeline, not ffmpeg.

## Speed peers (in-process, same bytes)

These three are timed in [`benchmarks.md`](benchmarks.md) on the same
classic RIFF clip (2 s @ 16 kHz → mixed planar `f32`).

| | [ryf](https://github.com/ekhodzitsky/ryf) | [hound](https://github.com/ruuda/hound) 3.5 | [symphonia](https://github.com/pdeljanov/Symphonia) 0.5 | [wavers](https://github.com/jmg049/wavers) 1.5 |
|---|---|---|---|---|
| Job | WAVE family ingest | PCM/IEEE RIFF iterator | multi-format demux | PCM/IEEE reader |
| RIFF PCM / IEEE f32 | yes | yes | yes (`wav`+`pcm`) | yes |
| RIFX / RF64 / BW64 / Wave64 | yes | no | limited | no |
| G.711 | yes | no | yes | no |
| MS / IMA ADPCM | yes (`adpcm`) | no | limited | no |
| Write | RIFF + RF64 PCM + f32 | PCM/IEEE, extensible | **no** | PCM/IEEE (path) |
| Default deps | **none** | none | several | several (`thiserror`, `bytemuck`, `i24`, …) |
| Caps | yes | no | no | no |
| Output | planar `f32` | typed sample iterator | decoded packets | typed `Samples<T>` |
| In-memory encode bench | yes | yes | — | no (write is path-only) |

hound is the default WAV crate (~1k reverse-deps). It stops at PCM/IEEE
RIFF. Last release 2023.

Symphonia is a media pipeline. The bench enables **only** `wav` + `pcm`,
not the full default codec set. It does not encode.

wavers is a later PCM/IEEE reader that advertises bulk SIMD conversion.
Default build pulls extra crates. Encode is file-path API, so it is
**decode-only** in our Criterion harness.

## Oracle, not a speed peer

**ffmpeg** is the bit-exact `f32` oracle on lossless PCM / G.711 when
present on `PATH`. It is a subprocess, not a library. Not timed.

## Other WAVE crates (not timed)

| Crate | Why it is not in the harness |
|---|---|
| [`wav`](https://crates.io/crates/wav) 1.0 | **Deprecated**; author says use hound. PCM only. LGPL. |
| [`wav_io`](https://crates.io/crates/wav_io) | Small PCM utility. Not a family codec. |
| [`rustwav`](https://crates.io/crates/rustwav) | Kitchen-sink (MP3/Opus/FLAC + resample). Custom license, many deps. Different product. |
| rodio | Playback. Decodes via hound/symphonia. |
| cpal | Host audio I/O, not a WAVE codec. |

If a crate is a WAV **library** aimed at PCM RIFF, it belongs in the
speed table or in this list with a reason. Gaps: open an issue.
