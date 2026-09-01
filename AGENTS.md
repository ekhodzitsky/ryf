# ryf — Agent Guide

just wav.

Pure-Rust WAVE family **codec**. One crate. No cloud. No Python.

Input: WAVE bytes or a seekable file. Output: planar `f32` at the file's
native sample rate.

## Aim

**Pure Rust. Zero deps. SOTA WAVE codec as a tiny Unix library.**

- Default features (`adpcm` + `simd`) pull **no crates**.
- No clap, tokio, anyhow, tracing, thiserror, hound, symphonia.
- Encode is classic RIFF: PCM U8/S16/S24/S32 + IEEE f32, 1–26 ch, plus
  molv drop-in `encode_s16` (mono) / `encode_f32` / `write_s16`. Streaming
  `WavWriter`. No RF64/ADPCM/RIFX write.
- No resample. No `mmap` / `libc`. No C/asm except optional SIMD in
  `convert/simd.rs` (each `unsafe` has a SAFETY comment).
- Crate name is `ryf`. Slogan is **just wav**. Not a family prefix.

## How it runs

```sh
cargo test
cargo test --doc
cargo fmt --check
cargo clippy --all-targets -- -D warnings
# impl-only line coverage (exclude sibling tests); target ≥ 90%
cargo llvm-cov --lib --ignore-filename-regex '_tests|proptest' --summary-only -- --skip proptest
```

Library: `decode_bytes` / `decode_s16` / `decode_f32` / `read_s16` /
`read_f32` / `probe_with` / `decode_streaming` / `sniff_wav` / `encode` /
`encode_s16` / `encode_f32` / `write` / `write_s16` / `write_f32` /
`WavWriter`.
Caps: `DecodeOptions::speech()` (default) or `unbounded()`.

## Forbidden

- Cloud APIs
- Python / PyO3 / ffmpeg on the product path (ffmpeg is a **test oracle**)
- Extra crates on the default feature set
- RF64 / ADPCM / RIFX encode
- Shipping long PCM / listen dumps in git
  (tiny ADPCM vectors in `fixtures/` are allowed)

## Hard rules

- No `unwrap` / `expect` in library modules. Tests may still have harvest
  `unwrap`; convert them to `?` when touching a test file.
- English for comments, docs, commits.
- Implementation `src/**/*.rs`: max 400 lines. Test files: max 500.
- `lib.rs`: module tree + public re-exports only.
- Tests live in sibling `*_tests.rs`.
- Git never contains long PCM or listen WAVs.

## Versioning

Keep a Changelog + SemVer. `Cargo.toml` version is the crate version.
Every user-visible change adds a bullet under `## [Unreleased]`.
