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

**ffmpeg** is a correctness oracle (subprocess), not a speed peer.
See [compare.md](compare.md) for crates that are documented but not timed.

Apple Silicon (aarch64, NEON), rustc 1.88, `cargo bench --bench wav`
(Criterion 0.8, 40 samples, 3 s). Point estimate (middle of the CI).

| Workload | ryf | hound | symphonia | wavers | vs h / sy / wv |
|---|---|---|---|---|---|
| decode PCM16 mono → f32 | 3.78 µs | 193 µs | 103 µs | 18.2 µs | **51× / 27× / 4.8×** |
| decode PCM16 stereo mix → f32 | 9.89 µs | 551 µs† | 232 µs | 99.1 µs | **56× / 23× / 10×** |
| decode IEEE f32 mono | 3.61 µs | 203 µs | 108 µs | 13.9 µs | **56× / 30× / 3.9×** |
| encode PCM16 mono (from i16) | 5.26 µs | 112 µs | — | — | **21×** |
| encode IEEE f32 mono | 3.29 µs | 44.3 µs | — | — | **13×** |

† hound stereo mix had high variance this run (502–618 µs). Other rows are tight.

`decode_streaming` on the same s16 mono clip is 5.57 µs (same kernels, no
full-file planar alloc). Encode PCM16 packs i16 with a LE memcpy then
`encode_s16`. Encode f32 copies IEEE bits once (no intermediate `Vec`).

These numbers are **not** a 2-hour file on disk and **not** Linux x86
(SSE path unmeasured here).

```sh
just bench
# or: cargo bench --bench wav
```
