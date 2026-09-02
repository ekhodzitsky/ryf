# Benchmarks

In-process, same classic RIFF bytes, 2 s @ 16 kHz (~64–128 KiB, **cache-hot**).
Decode is timed through to mixed planar `f32` (the STT output).

Who is timed, and how:

| Peer | Decode | Encode |
|---|---|---|
| **ryf** | `decode_bytes` → mixed `f32` | LE memcpy pack + `encode_s16` / `encode_f32` |
| **hound** 3.5 | sample iterator; i16 `/ 32768`, stereo sum / n | `write_sample` per sample |
| **symphonia** 0.5 (`wav`+`pcm` only) | probe + PCM decoder from a zero-copy slice, then the same mix | **no encode** |
| **wavers** 1.5 | `Wav::new(Cursor)` + `read::<f32>()`, then the same mix | path-only — **not timed** |
| **dr_wav** 0.14 (C, `--features bench-c`) | `drwav_open_memory_and_read_pcm_frames_f32`, then the same mix | sequential memory write |

**ffmpeg** is a correctness oracle (subprocess), not a speed peer.
See [compare.md](compare.md) for crates and other-language libraries that
are documented but not timed.

Apple Silicon (aarch64, NEON), rustc 1.88, `cargo bench --bench wav --features bench-c`
(Criterion 0.8, 40 samples, 3 s). Point estimate (middle of the CI).

| Workload | ryf | hound | symphonia | wavers | dr_wav | vs h / sy / wv / dr |
|---|---|---|---|---|---|---|
| decode PCM16 mono → f32 | 3.78 µs | 186 µs | 102 µs | 18.5 µs | 16.3 µs | **49× / 27× / 4.9× / 4.3×** |
| decode PCM16 stereo mix → f32 | 9.74 µs | 431 µs | 227 µs | 91.7 µs | 77.7 µs | **44× / 23× / 9.4× / 8.0×** |
| decode IEEE f32 mono | 3.56 µs | 204 µs | 106 µs | 19.0 µs† | 9.39 µs | **57× / 30× / 5.3× / 2.6×** |
| encode PCM16 mono (from i16) | 5.72 µs‡ | 112 µs | — | — | 5.42 µs | **20×** vs hound; **tie** vs C |
| encode IEEE f32 mono | 3.30 µs | 43.9 µs | — | — | 9.12 µs | **13×** vs hound; **2.8×** vs C |

† wavers f32 CI was wide this run (17.8–20.1 µs). Other decode rows are tight.
‡ ryf encode s16 CI was wide (5.36–6.16 µs); dr_wav was 5.40–5.44 µs.

`decode_streaming` on the same s16 mono clip is 5.45 µs (same kernels, no
full-file planar alloc). Encode PCM16 packs i16 with a LE memcpy then
`encode_s16`. Encode f32 copies IEEE bits once (no intermediate `Vec`).

These numbers are **not** a 2-hour file on disk and **not** Linux x86
(SSE path unmeasured here).

```sh
just bench          # Rust peers
just bench-c        # + vendored dr_wav (needs a C compiler)
# or: cargo bench --bench wav --features bench-c
```
