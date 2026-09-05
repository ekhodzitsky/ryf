# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- MS-ADPCM adaptive delta is clamped to `16..=32767`. A large step times
  adaptation 512/614/768 used to wrap through `i16` (e.g. 20000 * 512
  became -25536).
- ADPCM duration / RAM estimate uses the `block_align` nibble formula.
  Header `samplesPerBlock` of 0 no longer estimates zero frames; a lying
  high value no longer inflates the gate.
- `ByteSource` memory seek `Start` / `Current` / `End` no longer go
  through `i64` (offsets `>= 2^63` were reported as seek-before-start).
  `ignore_bytes` / PCM skip use `advance` instead of `u64 as i64`. File
  `read` retries `Interrupted`. `from_file` falls back to seek-end when
  metadata fails.
- Odd-length `data` chunks get a RIFF pad byte. Chunk size stays odd;
  parent size includes the pad (`encode` / `WavWriter`). `WavWriter`
  pads at most once if `finalize` fails and `Drop` retries.
- `sniff_is_riff_wave` rewinds to 0 on I/O error (docs already said so).
- ADPCM `fmt ` leftover after `cbSize` is skipped (padded IMA/MS chunks).
- `fact` sample count 0 is unknown, same as ds64. Probe PCM frames is
  `min(data/frame, fact)` so a lying `fact` matches decode.
- Empty GSM / G.722 `data` keeps one empty plane. `decode_f32` is
  `Empty`, not `InvalidOperation`.
- `decode_s16` rejects `sample_rate` 0 (same as `decode_with`).
- `convert_s16_le_to_f32` converts `min(dst.len(), src.len()/2)` and
  does not panic when the slices disagree.
- Header walk maps unexpected EOF to `Format(Truncated)`, not `Io`.
- MS-ADPCM `fmt ` accepts fewer than 7 coefficient pairs (`cbSize >= 8`).
- `WavWriter::Drop` flushes after patching sizes. `finalize` marks
  done only after a successful flush.
- `WavWriter::write_f32_samples` rejects a non-frame length before
  allocating.
- `probe` frame count uses the same `min(declared, file)` clamp as
  decode. A truncated `data` chunk no longer over-reports duration.
- Wave64: after a chunk GUID, a short size field is `Truncated`, not
  `MissingChunk`.
- `probe` / `probe_with` rewind the stream to 0 afterwards (same as
  `sniff_is_riff_wave`), so a following `decode_with` on the same
  source works.
- G.711 encode writes WAVEFORMATEX (`fmt ` size 18, `cbSize` 0) and a
  `fact` chunk, same layout as IEEE f32. Decode still accepts the older
  PCM-style `fmt ` 16 files.
- Pull short reads (`UnexpectedEof`) are `Format(Truncated)`, matching
  `decode_s16`.
- `ByteSource` file `seek` retries `Interrupted`. `from_file` rewinds
  to 0 even when the `File` cursor was not at the start.
- ADPCM decode / probe honor a smaller `fact` sample count (same as
  PCM / G.722 / GSM). A lying-high `fact` is still ignored.
- A 12-byte `RIFF....WAVE` (no chunks) is `MissingChunk`, not
  `Truncated`. Header peek no longer requires 16 bytes (W64 GUID)
  before the classic path.
- PCM `fmt ` lengths other than 16 / 18 / 40 are accepted (surplus
  skipped). ffmpeg already decoded `fmt ` 20 and Wave64-padded 24.
- IEEE `fmt ` 18 with non-zero `cbSize` (and odd lengths such as 17)
  skip the extra bytes instead of `MalformedFmt`. ffmpeg already did.
- IMA-ADPCM `fmt ` extra longer than 2 bytes, and WAVEFORMATEXTENSIBLE
  `cbSize` longer than 22, skip the surplus. ffmpeg already did.
- IMA stereo duration / RAM estimate drops leftover bytes shorter than
  an 8-byte L/R nibble group, matching the decoder (e.g. `block_align`
  12 is 1 frame, not 5).
- `ByteSource::from_file`: if rewind to 0 fails, `pos` follows the real
  cursor instead of claiming 0.

## [0.6.0] - 2026-09-04

### Added

- G.711 A-law / mu-law encode: `encode_alaw` / `encode_mulaw` (interleaved
  f32) and `WriteFormat::ALaw` / `MuLaw`. Mu-law uses 16-bit segment
  endpoints so encode matches `ulaw2linear`.
- RIFX write: `encode_rifx` / `WavWriter::new_rifx` (PCM / IEEE / G.711).
- `WAVEFORMATEXTENSIBLE` write: `encode_extensible` /
  `WavWriter::new_extensible` (PCM / IEEE; G.711 is `UnsupportedCodec`).
- RIFX ADPCM decode (block headers follow container endian).

### Changed

- README decode table is coverage. Write lists PCM, G.711, RF64, RIFX,
  extensible. No ADPCM / G.722 / GSM encode.

### Breaking

- `WriteFormat` gains `ALaw` and `MuLaw`.

## [0.5.1] - 2026-09-04

### Changed

- Crate description and README: WAVE to planar f32, codecs, zero deps.
  No slogan. No "faster than hound" tagline. Timings live in the
  comparison table with machine and clip named.
- Docs and comments use ordinary punctuation (no `->` in prose).

## [0.5.0] - 2026-09-03

### Added

- Microsoft GSM 06.10 / wav49 decode (WAVE tag `0x0031`). 8 kHz mono,
  65-byte blocks (320 PCM samples). Headerless `decode_gsm` /
  `decode_gsm_mono`. `ProbeCodec::Gsm`. Always on (not the `adpcm`
  feature). 33-byte toast / MSN variable block sizes are not decoded.

### Changed

- **Breaking.** `ProbeCodec` gains `Gsm`.

## [0.4.1] - 2026-09-03

### Changed

- Docs / rustdoc: G.722 is **64 kbit/s only** (56/48 not decoded). WAVE
  G.722 output / `probe` rate is always 16 kHz. `probe` uses library
  defaults, not speech caps. Crate rustdoc names encode. `ByteSource`
  inner is `Send` (`from_read_seek` matches the 0.3 contract).

### Fixed

- RIFX G.722: big-endian `fmt ` tag round-trips with headerless decode
  (tags `0x0064` / `0x028F`).

## [0.4.0] - 2026-09-02

### Added

- G.722 decode (64 kbit/s SB-ADPCM). WAVE tags `0x0064` (Asterisk/SBC;
  mmreg names this G.726), `0x0065` (mmreg G.722), `0x028F` (ffmpeg).
  Headerless `decode_g722` / `decode_g722_mono` (`?codec=g722`; SDP
  `8000` or `16000`). Output / `probe` rate is always 16 kHz.
  `ProbeCodec::G722`.

### Changed

- **Breaking.** `ProbeCodec` gains `G722`.

## [0.3.2] - 2026-09-02

### Changed

- Documentation is ASCII only (no arrows or em dashes). Slogan and
  crates.io description stay **Just wav.**

## [0.3.1] - 2026-09-02

### Changed

- Slogan is **Just wav.** (crates.io description and README).

## [0.3.0] - 2026-09-02

### Added

- `read_speech` - mix-to-mono + 2 h / 4 GiB caps (old `read` behaviour).
- `decode_reader` - slurp `impl Read` then `decode_bytes`.
- `WavError::Format(FormatKind)` and `UnsupportedCodec { tag }`.

### Changed

- **Breaking.** `read` / `DecodeOptions::default` are library defaults:
  split channels, archival caps. Speech ingest is explicit (`read_speech` /
  `DecodeOptions::speech`).
- **Breaking.** `f32_to_s16le` / `s16le_to_f32` use the decode scale
  (`/ 32768`). `-1.0` encodes as `i16::MIN`.
- **Breaking.** `ByteSource` requires `Send`. `WavWriter` requires
  `Read + Write + Seek` and auto-promotes RIFF to RF64 like `encode`.
- Crate rustdoc is the API, not the GitHub README.
- README is the ingest job (untrusted / telephony WAVE to planar `f32`);
  `read` vs `read_speech` is on the first screen. Published on crates.io.

### Removed

- Harvest aliases: `DecodeOptions::product_stt`, `MAX_DURATION_S` /
  `MAX_SAMPLE_RATE` / `MAX_DECODE_SAMPLE_RATE`, `convert_s16_mono_pub`.
- `WavError::Format(String)`.

### Performance

- s16 to f32: multiply by `2^-15` instead of divide (bit-exact for every `i16`);
  NEON convert is 16-wide.
- `encode_f32`: one memcpy of IEEE bits on little-endian hosts.
- Classic RIFF PCM/IEEE headers written as one stack buffer.

### Fixed

- ffmpeg oracle: NaN payloads compare equal (Linux CI vs ffmpeg f32).
- G.711 collect/stream coverage (slice + cursor) so llvm-cov stays >= 90%.
- Crate summary names RF64 write (README lede, `Cargo.toml` description).

## [0.2.1] - 2026-09-02

### Added

- G.711 A-law / mu-law 256-entry LUT + bulk convert (collect and pull).
- Headerless G.711: `decode_g711` / `decode_g711_alaw` / `decode_g711_mulaw`
  (`G711Law`; rate and channels stated by the caller).
- RF64 write: `encode` upgrades when RIFF `u32` sizes overflow;
  `encode_rf64` / `write_rf64` / `WavWriter::new_rf64` force RF64.

## [0.2.0] - 2026-09-01

### Added

- `read` / `read_with`: path to planar `f32` (`speech()` caps, or explicit
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
  at 1-26 channels: `WriteSpec` / `WriteFormat`, `encode` / `write`,
  `write_f32`. `encode_s16` / `write_s16` stay molv mono drop-ins.
- `WavWriter`: streaming RIFF writer; sizes patched on `finalize` or drop.
- GitHub Actions: `fmt`, `clippy -D warnings`, `test`, impl-only llvm-cov
  >= 90%.

### Changed

- `encode_f32` accepts 1..=26 channels (was 1..=2).
- `fact` / ds64 `sampleCount` is samples per channel. The old "divide when
  divisible by nChannels" heuristic truncated even-length stereo IEEE.

## [0.1.3] - 2026-09-01

### Changed

- README is the crate docs (`include_str`). Support matrix is now
  container x codec (not a paired two-column list). Documents empty
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
  SIMD s16 to f32. `ByteSource` inlined from gigastt-source. No mmap, no
  `zeroize`, no `tracing`. Default features are `std` only.

### Changed

- Split harvest modules to the 400-line impl / 500-line test caps
  (`header`, `convert`, `pull`, `adpcm`, `wav_tests`). Decode behavior is
  unchanged. README layout lists the split directories.

[Unreleased]: https://github.com/ekhodzitsky/ryf/compare/v0.6.0...HEAD
[0.6.0]: https://github.com/ekhodzitsky/ryf/compare/v0.5.1...v0.6.0
[0.5.1]: https://github.com/ekhodzitsky/ryf/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/ekhodzitsky/ryf/compare/v0.4.1...v0.5.0
[0.4.1]: https://github.com/ekhodzitsky/ryf/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/ekhodzitsky/ryf/compare/v0.3.2...v0.4.0
[0.3.2]: https://github.com/ekhodzitsky/ryf/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/ekhodzitsky/ryf/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/ekhodzitsky/ryf/releases/tag/v0.3.0
