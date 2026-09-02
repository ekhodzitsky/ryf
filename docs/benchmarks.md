# Benchmarks

In-process, same classic RIFF bytes, 2 s @ 16 kHz (the speech-ingest clip
size — ~64–128 KiB, **cache-hot**). Decode is timed through to mixed planar
`f32`.

- **hound** 3.5: typed sample iterator; the bench converts i16 with `/ 32768`
  and mixes stereo with the same sum / n as ryf.
- **Symphonia** 0.5 (`wav` + `pcm` only): probe + PCM decoder from a
  zero-copy `MediaSource` slice, then the same mix. Not the full default
  codec set.
- **ffmpeg** is a correctness oracle (subprocess), not a speed peer.
  Symphonia does not encode.

Apple Silicon (aarch64, NEON), rustc 1.88, `cargo bench --bench wav`
(Criterion 0.8, 40 samples, 3 s). Median.

| Workload | ryf | hound | symphonia | vs hound / vs sy |
|---|---|---|---|---|
| decode PCM16 mono → f32 | 4.98 µs | 181 µs | 96.1 µs | **36× / 19×** |
| decode PCM16 stereo mix → f32 | 10.4 µs | 451 µs | 213 µs | **43× / 20×** |
| decode IEEE f32 mono | 3.46 µs | 185 µs | 102 µs | **53× / 30×** |
| encode PCM16 mono (from i16) | 18.5 µs | 113 µs | — | **6.1×** |
| encode IEEE f32 mono | 23.1 µs | 56.4 µs | — | **2.4×** |

`decode_streaming` on the same s16 mono clip is 5.10 µs (same kernels, no
full-file planar alloc).

These numbers are **not** a 2-hour file on disk and **not** Linux x86
(SSE path unmeasured here).

```sh
just bench
# or: cargo bench --bench wav
```
