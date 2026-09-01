# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Split harvest modules to the 400-line impl / 500-line test caps
  (`header`, `convert`, `pull`, `adpcm`, `wav_tests`). Decode behavior is
  unchanged. README layout lists the split directories.

### Added

- Harvest of the WAVE reader from gigastt-wav: RIFF / RIFX / RF64 / BW64 /
  Wave64, PCM 8/16/24/32, IEEE f32/f64, G.711, MS + IMA ADPCM, pull-streaming,
  SIMD s16→f32. `ByteSource` inlined from gigastt-source. No encode, no mmap,
  no `zeroize`, no `tracing`. Default features are `std` only.

## [0.1.0] - unreleased

Scaffold. Not published.
