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
- Output buffers are initialized before safe slice access. Unsafe code is
  denied by default; narrow exceptions cover SIMD dispatch/kernels and
  checked initialized-f32 byte views. G.711 table lookup is safe indexing.
- Encoder fingerprints cover every PCM16 input for both G.711 laws.
  Structured malformed MS-ADPCM cases exercise arbitrary coefficients.
  Writer fault injection covers partial writes, reads, seeks, and flushes,
  terminal failures, validation ordering, and file-backed RF64 promotion.
- CI: Ubuntu (ffmpeg oracle), macOS, Windows; independent feature combinations,
  default and feature-off lints, strict rustdoc, package verification,
  benchmark smoke, and focused Miri tests. Impl line coverage >= 90%.
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

Real disk boundary tests are opt-in and require 5 GiB free on the filesystem
containing target. They write ordinary files, verify all payload bytes and
delete their temporary files when finished. Run sequentially:

```sh
cargo test --release real_riff_and_extensible_cross_4gib -- --ignored
cargo test --release real_boundary_partial_relocation_is_terminal -- --ignored
```

The full Miri-compatible library suite needs filesystem access. External
ffmpeg oracle tests are explicitly ignored under Miri; the G.726 tone test
still checks its fixtures and predictor counters without spawning ffmpeg.

```sh
MIRIFLAGS=-Zmiri-disable-isolation cargo +nightly miri test --lib
```

Cross-target execution uses a temporary manifest under target so benchmark
competitors and their native build dependencies need not cross-compile.
Install the Rust musl targets and QEMU user emulators, then compare scalar
and SIMD outputs using the checked-in public API exercise:

```sh
rustup target add i686-unknown-linux-musl aarch64-unknown-linux-musl
sh tools/check-portable.sh i686-unknown-linux-musl
sh tools/check-portable.sh aarch64-unknown-linux-musl
```

These checks also run in CI. See the [API review](api-stability.md) for
contracts and the limits of these experiments.

Reproducible ASan targets and deterministic seed generation live in
`tools/fuzz/`. Seed setup verifies that all container seeds decode; ADPCM
fixture counts are trimmed to 128 frames to fit the per-case budget.
The targets compare memory collect with seekable-reader streaming,
including sample bits and errors. The structured MS/IMA target also checks
that `fact` prevents reads beyond the required blocks and that a discarded
suffix cannot change successful audio. Each input is capped at 0.1 seconds
and 1 MiB logical output; input lengths are 8192/2048 bytes respectively.

The optional runner needs a host C++ compiler, nightly Rust and cargo-fuzz.
Its generated manifest pins libfuzzer-sys 0.4.13. Neither the manifest nor
the C++ fuzz runtime is part of the product dependency graph. From a checkout:

```sh
rustup toolchain install nightly
cargo install cargo-fuzz --version 0.13.2 --locked
sh tools/check-fuzz.sh containers 300
sh tools/check-fuzz.sh adpcm 300
```

CI runs ten-second smoke campaigns. Corpora, lockfiles, binaries and crash
artifacts stay under ignored `target/fuzz-check/`. Existing corpus entries
are retained when rerunning; remove only that generated directory to start
again from the checked-in fixture and deterministic seed inputs. Minimized
confirmed failures belong in sibling regression tests, not large audio dumps.
