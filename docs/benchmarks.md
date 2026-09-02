# Benchmarks

In-process, same classic RIFF bytes, 2 s @ 16 kHz (~64-128 KiB, **cache-hot**).
Decode is timed through to mixed planar `f32` (the STT output).

Who is timed, and how:

| Peer | Decode | Encode |
|---|---|---|
| **ryf** | `decode_bytes` -> mixed `f32` | LE memcpy pack + `encode_s16` / `encode_f32` |
| **hound** 3.5 | sample iterator; i16 `/ 32768`, stereo sum / n | `write_sample` per sample |
| **symphonia** 0.5 (`wav`+`pcm` only) | probe + PCM decoder from a zero-copy slice, then the same mix | **no encode** |
| **wavers** 1.5 | `Wav::new(Cursor)` + `read::<f32>()`, then the same mix | path-only - **not timed** |
| **dr_wav** 0.14 (C, `--features bench-c`) | `drwav_open_memory_and_read_pcm_frames_f32`, then the same mix | sequential memory write |

**ffmpeg** is a correctness oracle (subprocess), not a speed peer.
See [compare.md](compare.md) for crates and other-language libraries that
are documented but not timed.

Apple Silicon (aarch64, NEON), rustc 1.88, `cargo bench --bench wav --features bench-c`
(Criterion 0.8, 40 samples, 3 s). Point estimate (middle of the CI).

| Workload | ryf | hound | symphonia | wavers | dr_wav | vs h / sy / wv / dr |
|---|---|---|---|---|---|---|
| decode PCM16 mono -> f32 | 3.78 mus | 186 mus | 102 mus | 18.5 mus | 16.3 mus | **49x / 27x / 4.9x / 4.3x** |
| decode PCM16 stereo mix -> f32 | 9.74 mus | 431 mus | 227 mus | 91.7 mus | 77.7 mus | **44x / 23x / 9.4x / 8.0x** |
| decode IEEE f32 mono | 3.56 mus | 204 mus | 106 mus | 19.0 mus* | 9.39 mus | **57x / 30x / 5.3x / 2.6x** |
| encode PCM16 mono (from i16) | 5.72 mus** | 112 mus | - | - | 5.42 mus | **20x** vs hound; **tie** vs C |
| encode IEEE f32 mono | 3.30 mus | 43.9 mus | - | - | 9.12 mus | **13x** vs hound; **2.8x** vs C |

* wavers f32 CI was wide this run (17.8-20.1 mus). Other decode rows are tight.
** ryf encode s16 CI was wide (5.36-6.16 mus); dr_wav was 5.40-5.44 mus.

`decode_streaming` on the same s16 mono clip is 5.45 mus (same kernels, no
full-file planar alloc). Encode PCM16 packs i16 with a LE memcpy then
`encode_s16`. Encode f32 copies IEEE bits once (no intermediate `Vec`).

These numbers are **not** a 2-hour file on disk and **not** Linux x86
(SSE path unmeasured here).

```sh
just bench          # Rust peers
just bench-c        # + vendored dr_wav (needs a C compiler)
# or: cargo bench --bench wav --features bench-c
```
