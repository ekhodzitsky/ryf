agent-dev-kb: 0.9.1

# ryf - Agent Guide

Pure-Rust WAVE **codec**. One crate. No cloud. No Python. WAVE only.

Input: WAVE bytes or a seekable file. Output: planar `f32` at the file's
native sample rate.

## Aim

**Pure Rust. Zero deps. SOTA WAVE codec as a tiny Unix library.**

- Default features (`adpcm` + `simd`) pull **no crates**.
- No clap, tokio, anyhow, tracing, thiserror, hound, symphonia, wavers on
  the **product** path. `hound`, `symphonia`, `wavers`, and `criterion` are
  **dev-only** bench competitors. Optional `bench-c` vendors **dr_wav** (C)
  via `cc` for Criterion only - never linked into the library.
- Encode is PCM U8/S16/S24/S32 + IEEE f32 + G.711 A-law/mu-law, 1-26 ch:
  classic RIFF when it fits, RF64 when it does not (`encode_rf64` /
  `WavWriter::new_rf64` to force). RIFX: `encode_rifx` /
  `WavWriter::new_rifx`. `WAVEFORMATEXTENSIBLE` PCM/IEEE/G.711:
  `encode_extensible` / `WavWriter::new_extensible` (both RF64 when sizes
  overflow). Mono PCM16 helper `encode_s16` / `write_s16`; `encode_f32`;
  `encode_alaw` / `encode_mulaw`. No ADPCM / G.722 / G.726 / GSM encode.
- No resample. No `mmap` / `libc`. No C/asm except optional SIMD in
  `convert/simd.rs` (each `unsafe` has a SAFETY comment).
- Crate name is `ryf`. Not a family prefix. Published on crates.io
  (`ryf = "0.7"`). No slogan. 0.7 is the 1.0-prep line (`non_exhaustive`
  public enums/structs; `source` is crate-private).

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
`decode_g711` / `decode_g722` / `decode_g726` / `decode_gsm` / `probe_with` / `decode_streaming` /
`sniff_wav` / `encode` /
`encode_rf64` / `encode_rifx` / `encode_extensible` / `encode_alaw` /
`encode_mulaw` / `encode_s16` / `encode_f32` / `write` / `write_s16` /
`write_f32` / `WavWriter`.
Caps: `DecodeOptions::default()` is `unbounded` + split;
`speech()` / `read_speech()` mix to mono with 2 h / 4 GiB
(duration uses min(file rate, 48 kHz)).

## Forbidden

- Cloud APIs
- Python / PyO3 / ffmpeg on the product path (ffmpeg is a **test oracle**)
- Extra crates on the default feature set
- ADPCM / G.722 / G.726 / GSM encode
- Shipping long PCM / listen dumps in git
  (tiny ADPCM vectors in `fixtures/` are allowed)

## Hard rules

- No `unwrap` / `expect` in library modules. Tests may still have harvest
  `unwrap`; convert them to `?` when touching a test file.
- English for comments, docs, commits. Documentation is ASCII only.
  Ordinary punctuation in prose. No arrows or `->` outside Rust syntax.
- Implementation `src/**/*.rs`: max 400 lines. Test files: max 500.
- `lib.rs`: module tree + public re-exports only.
- Tests live in sibling `*_tests.rs`.
- Git never contains long PCM or listen WAVs.

## Versioning

Keep a Changelog + SemVer. `Cargo.toml` version is the crate version.
Every user-visible change adds a bullet under `## [Unreleased]`.

## Local-only files

Never commit, force-add, or publish local agent configuration, memories,
sessions, caches, credentials, or private task-tracker data, even to a
private repository. Keep these paths ignored (patterns apply at any depth):

```gitignore
.grok
.serena
.claude
.codex
.cursor
.kimi
.kimi-code
.agents
.aider
.aider.*
.claude-memory-compiler
.codebase-memory
.codebase-memory-*
.mcp.json
.omc
.omx
.superpowers
.gstack
backlog
.backlog
backlog.config.yml
```

These patterns cover directories, files, and symlinks with those names.
Other tool-generated indexes, databases, logs, session transcripts, and
machine-specific MCP configs are local-only too: identify their actual
paths before creating them and add precise ignore rules.

Real `.env` files, credentials, private keys, and tokens must stay local.
Keep sanitized examples only when the project explicitly uses them.
Do not print secret-bearing file contents while checking.

`AGENTS.md`, `CLAUDE.md`, `agents/`, shared editor settings, and product
specifications are not automatically local-only. Follow the project's
rules; add project-specific exclusions without hiding product files.

Before staging, inspect `git status --short` and `git diff --cached
--name-only`, then check `git ls-files` for forbidden paths. Ignore rules
do not remove files already tracked. Report tracked violations by path;
never delete local data or rewrite history as part of setup. Removing
tracked files from the index needs an explicit cleanup scope. Do not
commit or publish while violations remain.

## Backlog.md

This repo uses Backlog.md (not GitHub Issues, not a second spec tool).
Run `backlog instructions overview` before work. Mutate via the CLI
(`--plain` / `--json`). One task = one session = one PR.
Docs → `doc create`; conclusions → `decision create`; work →
`task create`. Labels: `audit`, `research`.

## Coding principles

Karpathy-inspired. Caution over speed; skip ceremony on one-liners.
The queue is Backlog.md. No second spec or task framework.

1. **Think before coding.** State assumptions. Show interpretations
   when the request is ambiguous. Push back if something simpler
   exists. Stop and ask when unclear.
2. **Simplicity first.** No extra features, single-use abstractions,
   or speculative config. If 200 lines could be 50, rewrite.
3. **Surgical changes.** Touch only the requested lines. Match the
   surrounding style. Mention unrelated dead code; do not delete it.
   Remove only your own unused imports.
4. **Goal-driven.** Tests for invalid input, bug repro, and
   before-and-after. Every step has a verify. "Make it work" is not
   a criterion.
5. **Always.** Run this repo's checks before claiming done. No new
   dependency without a why and its transitive cost. No secrets, no
   tracker ids. Comments and commits follow the repo language, else
   English.

### Rust
No `unwrap` / `expect` on production paths. Honor clippy (often
`-D warnings`). Write the failing test first when that area is
already tested.

### Minimal code
<!-- Ladder adapted from ponytail (MIT), github.com/DietrichGebert/ponytail -->
Stop at the first rung that holds:
1. Needed at all? No: skip it (YAGNI).
2. Already in this codebase? Reuse, do not rewrite.
3. Stdlib does it? Use it.
4. Native platform feature? Use it.
5. Installed dependency? Use it.
6. One line? One line.
7. Only then: the minimum that works.
Never cut trust-boundary validation, data-loss handling, security,
or accessibility.
