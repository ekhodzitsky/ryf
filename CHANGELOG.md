# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.3] - 2026-09-01

### Changed

- README is the crate docs (`include_str`). Support matrix is now
  container × codec (not a paired two-column list). Documents empty
  encode vs empty decode, s16 encode peak 32767 vs decode `/32768`,
  and a hound / symphonia comparison.
- `WavError::packet_io` is `pub(crate)` (pull-loop helper, not a public
  constructor).

## [0.1.2] - 2026-09-01

### Fixed

- ADPCM `fmt` parser uses an MS/IMA `if`/`else` instead of a nested match
  with a dead catch-all (`unreachable!` in 0.1.1).

### Changed

- `read_s16` copies PCM16 from a file-backed `ByteSource` instead of
  `fs::read` + slice decode (same bytes on disk).
- ADPCM pull-stream reuses `i16_frames_to_f32` (same helper as collect).

## [0.1.1] - 2026-09-01

### Added

- Classic RIFF encode harvested from molv-wav: `encode_s16`, `encode_f32`,
  `write_s16`, plus pack helpers `f32_to_s16le` / `s16le_to_f32`.
- Molv/gigastt drop-in surface: `decode_s16` / `decode_f32` / `read_s16` /
  `read_f32` / `sniff_wav`, `DecodeOptions::product_stt`, `MAX_DURATION_S`
  aliases. Empty PCM16/f32 encode is allowed (sluh/kover).

## [0.1.0] - 2026-09-01

### Added

- Harvest of the WAVE reader from gigastt-wav: RIFF / RIFX / RF64 / BW64 /
  Wave64, PCM 8/16/24/32, IEEE f32/f64, G.711, MS + IMA ADPCM, pull-streaming,
  SIMD s16→f32. `ByteSource` inlined from gigastt-source. No mmap, no
  `zeroize`, no `tracing`. Default features are `std` only.

### Changed

- Split harvest modules to the 400-line impl / 500-line test caps
  (`header`, `convert`, `pull`, `adpcm`, `wav_tests`). Decode behavior is
  unchanged. README layout lists the split directories.
