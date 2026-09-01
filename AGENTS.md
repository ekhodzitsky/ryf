# ryf — Agent Guide

just wav.

Pure-Rust WAVE family **codec**. One crate. No cloud. No Python.

Input: WAVE bytes or a seekable file. Output: planar `f32` at the file's
native sample rate.

## Aim

**Pure Rust. Zero deps. SOTA WAVE codec as a tiny Unix library.**

- Default features (`adpcm` + `simd`) pull **no crates**.
- No clap, tokio, anyhow, tracing, thiserror, hound, symphonia.
- Encode is classic RIFF PCM16 mono + IEEE f32 (1–2 ch), harvested from
  molv-wav. No RF64/ADPCM/RIFX write.
- No resample. No `mmap` / `libc`. No C/asm except optional SIMD in
  `convert/simd.rs` (each `unsafe` has a SAFETY comment).
- Crate name is `ryf`. Slogan is **just wav**. Not a family prefix.

## How it runs

```sh
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
# impl-only line coverage (exclude sibling tests); target ≥ 90%
cargo llvm-cov --lib --ignore-filename-regex '_tests|proptest' --summary-only -- --skip proptest
```

Library: `decode_bytes` / `decode_s16` / `decode_f32` / `read_s16` /
`read_f32` / `probe_with` / `decode_streaming` / `sniff_wav` / `encode_s16`
/ `encode_f32` / `write_s16`.
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
