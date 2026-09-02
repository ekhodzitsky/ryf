# ryf - Agent Guide

Just wav.

Pure-Rust WAVE family **codec**. One crate. No cloud. No Python.

Input: WAVE bytes or a seekable file. Output: planar `f32` at the file's
native sample rate.

## Aim

**Pure Rust. Zero deps. SOTA WAVE codec as a tiny Unix library.**

- Default features (`adpcm` + `simd`) pull **no crates**.
- No clap, tokio, anyhow, tracing, thiserror, hound, symphonia, wavers on
  the **product** path. `hound`, `symphonia`, `wavers`, and `criterion` are
  **dev-only** bench competitors. Optional `bench-c` vendors **dr_wav** (C)
  via `cc` for Criterion only - never linked into the library.
- Encode is PCM U8/S16/S24/S32 + IEEE f32, 1-26 ch: classic RIFF when it
  fits, RF64 when it does not (`encode_rf64` / `WavWriter::new_rf64` to
  force). Molv drop-in `encode_s16` (mono) / `encode_f32` / `write_s16`.
  No ADPCM/RIFX write.
- No resample. No `mmap` / `libc`. No C/asm except optional SIMD in
  `convert/simd.rs` (each `unsafe` has a SAFETY comment).
- Crate name is `ryf`. Slogan is **Just wav.** Not a family prefix.
  Published on crates.io (`ryf = "0.3"`).

## How it runs

```sh
cargo test
cargo test --doc
cargo fmt --check
cargo clippy --all-targets -- -D warnings
# impl-only line coverage (exclude sibling tests); target >= 90%
cargo llvm-cov --lib --ignore-filename-regex '_tests|proptest' --summary-only -- --skip proptest
cargo bench --bench wav
cargo bench --bench wav --features bench-c   # vs dr_wav; needs a C compiler
```

Library: `decode_bytes` / `decode_s16` / `decode_f32` / `decode_reader` /
`read` / `read_speech` / `read_with` / `read_s16` / `read_f32` /
`decode_g711` / `probe_with` / `decode_streaming` / `sniff_wav` / `encode` /
`encode_rf64` / `encode_s16` / `encode_f32` / `write` / `write_s16` /
`write_f32` / `WavWriter`.
Caps: `DecodeOptions::default()` is `unbounded` + split;
`speech()` / `read_speech()` mix to mono with 2 h / 4 GiB.

## Forbidden

- Cloud APIs
- Python / PyO3 / ffmpeg on the product path (ffmpeg is a **test oracle**)
- Extra crates on the default feature set
- ADPCM / RIFX encode
- Shipping long PCM / listen dumps in git
  (tiny ADPCM vectors in `fixtures/` are allowed)

## Hard rules

- No `unwrap` / `expect` in library modules. Tests may still have harvest
  `unwrap`; convert them to `?` when touching a test file.
- English for comments, docs, commits. Documentation is ASCII only
  (no arrows, em dashes, or similar).
- Implementation `src/**/*.rs`: max 400 lines. Test files: max 500.
- `lib.rs`: module tree + public re-exports only.
- Tests live in sibling `*_tests.rs`.
- Git never contains long PCM or listen WAVs.

## Versioning

Keep a Changelog + SemVer. `Cargo.toml` version is the crate version.
Every user-visible change adds a bullet under `## [Unreleased]`.
