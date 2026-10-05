# Correctness

- Differential suite vs **ffmpeg** (bit-exact `f32` on lossless PCM / G.711,
  and G.722 / GSM `gsm_ms` decode of the same bitstream) when `ffmpeg` is
  on `PATH`. ffmpeg is a **test oracle**, not a runtime dep.
  NaN payloads compare equal (ffmpeg canonicalizes quiet NaN; we pass bits
  through). Finite samples stay bit-exact.
- G.726 integer PCM is bit-exact against ffmpeg 7.0.2 `s16le` at 16/24/32/40
  kbit/s and both packings. ffmpeg `g726` is AAL2 (left-justified, WAVE tag
  `0x0045`). ffmpeg `g726le` is RFC 3551 (right-justified). Planar `f32` is
  each `i16` times `1/32768`. That scale is bit-exact against the integers.
  It is not a comparison with ffmpeg `f32le`. The 160-sample tone, the
  compressed bytes, and the commands are in `fixtures/g726/`
  (`PROVENANCE.txt`). A longer live encode (silence, a step, then a 440 Hz
  tone) runs when `ffmpeg` is on `PATH`.
- SIMD paths match scalar bit-for-bit.
- `unsafe` is confined to `convert/simd.rs`, G.711 `u8` table lookup, and
  uninit `f32` scratch (Copy, every element written before `Ok`); each
  block has a SAFETY comment.
- CI: Ubuntu (ffmpeg oracle), macOS, Windows. Impl line coverage >= 90%.
- Speed peers and other WAVE crates: [compare.md](compare.md),
  [benchmarks.md](benchmarks.md).

```sh
cargo test
cargo test --doc
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo llvm-cov --lib --ignore-filename-regex '_tests|proptest' --summary-only -- --skip proptest
cargo bench --bench wav
cargo bench --bench wav --features bench-c   # vs dr_wav; needs a C compiler
```
