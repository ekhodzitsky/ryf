# Correctness

- Differential suite vs **ffmpeg** (bit-exact `f32` on lossless PCM / G.711)
  when `ffmpeg` is on `PATH`. ffmpeg is a **test oracle**, not a runtime dep.
  NaN payloads compare equal (ffmpeg canonicalizes quiet NaN; we pass bits
  through). Finite samples stay bit-exact.
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
