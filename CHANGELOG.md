# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0] - 2026-09-02

### Added

- `read_speech` — mix-to-mono + 2 h / 4 GiB caps (old `read` behaviour).
- `decode_reader` — slurp `impl Read` then `decode_bytes`.
- `WavError::Format(FormatKind)` and `UnsupportedCodec { tag }`.

### Changed

- **Breaking.** `read` / `DecodeOptions::default` are library defaults:
  split channels, archival caps. Speech ingest is explicit (`read_speech` /
  `DecodeOptions::speech`).
- **Breaking.** `f32_to_s16le` / `s16le_to_f32` use the decode scale
  (`/ 32768`). `-1.0` encodes as `i16::MIN`.
- **Breaking.** `ByteSource` requires `Send`. `WavWriter` requires
  `Read + Write + Seek` and auto-promotes RIFF → RF64 like `encode`.
- Crate rustdoc is the API, not the GitHub README.
- README says **why ryf** versus hound / Symphonia / dr_wav / ffmpeg.

### Removed

- Harvest aliases: `DecodeOptions::product_stt`, `MAX_DURATION_S` /
  `MAX_SAMPLE_RATE` / `MAX_DECODE_SAMPLE_RATE`, `convert_s16_mono_pub`.
- `WavError::Format(String)`.

### Performance

- s16→f32: multiply by `2^-15` instead of divide (bit-exact for every `i16`);
  NEON convert is 16-wide.
- `encode_f32`: one memcpy of IEEE bits on little-endian hosts.
- Classic RIFF PCM/IEEE headers written as one stack buffer.

### Fixed

- ffmpeg oracle: NaN payloads compare equal (Linux CI vs ffmpeg f32).
- G.711 collect/stream coverage (slice + cursor) so llvm-cov stays ≥ 90%.
- Crate summary names RF64 write (README lede, `Cargo.toml` description).

## [0.2.1] - 2026-09-02

### Added

- G.711 A-law / µ-law 256-entry LUT + bulk convert (collect and pull).
- Headerless G.711: `decode_g711` / `decode_g711_alaw` / `decode_g711_mulaw`
  (`G711Law`; rate and channels stated by the caller).
- RF64 write: `encode` upgrades when RIFF `u32` sizes overflow;
  `encode_rf64` / `write_rf64` / `WavWriter::new_rf64` force RF64.

## [0.2.0] - 2026-09-01

### Added

- `read` / `read_with`: path → planar `f32` (`speech()` caps, or explicit
  `DecodeOptions`). `DecodedWav::num_channels` / `frames`.
- Dual license MIT OR Apache-2.0 (`LICENSE` + `LICENSE-APACHE`).
- `examples/decode.rs`, `examples/encode.rs`.
- CI test matrix: Ubuntu, macOS, Windows (ffmpeg oracle on Ubuntu).
- Crate metadata: `repository`, `homepage`, `documentation`.

### Deprecated

- Harvest aliases: `DecodeOptions::product_stt`, `MAX_DURATION_S` /
  `MAX_SAMPLE_RATE` / `MAX_DECODE_SAMPLE_RATE`, `convert_s16_mono_pub`.
  Use `speech()`, `DEFAULT_*`, `convert_s16_le_to_f32`.

## [0.1.6] - 2026-09-01

### Added

- Decode benches vs Symphonia 0.5 (`wav` + `pcm`, zero-copy `MediaSource`)
  on the same 2 s @ 16 kHz RIFF clips. README table is now three-way.

## [0.1.5] - 2026-09-01

### Added

- Criterion benches vs hound 3.5 (`cargo bench --bench wav` / `just bench`):
  decode PCM16 mono/stereo-mix and IEEE f32, encode PCM16/f32, on 2 s @
  16 kHz in-memory RIFF. Numbers in the README.

## [0.1.4] - 2026-09-01

### Added

- Classic RIFF write of integer PCM U8 / S16 / packed S24 / S32 and IEEE f32
  at 1–26 channels: `WriteSpec` / `WriteFormat`, `encode` / `write`,
  `write_f32`. `encode_s16` / `write_s16` stay molv mono drop-ins.
- `WavWriter`: streaming RIFF writer; sizes patched on `finalize` or drop.
- GitHub Actions: `fmt`, `clippy -D warnings`, `test`, impl-only llvm-cov
  ≥ 90%.

### Changed

- `encode_f32` accepts 1..=26 channels (was 1..=2).
- `fact` / ds64 `sampleCount` is samples per channel. The old “divide when
  divisible by nChannels” heuristic truncated even-length stereo IEEE.

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
