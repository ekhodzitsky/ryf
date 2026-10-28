agent-dev-kb: 0.10.0

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
(`--plain` / `--json`). One task has one agreed scope; sessions may resume it.
Local research: `doc create`; draft conclusions: `decision create`;
work: `task create`. Accepted, sanitized decisions belong in tracked
project docs; local records link to them. Labels: `audit`, `research`.

## Coding principles

Read and follow these principles before acting. The project-specific rules
below supplement the managed block; stricter project constraints take precedence.

<!-- agent-dev-kb:coding-principles sha256=812b4079cb88a1e24ce24ff6107bfe123f099ad2f3a994a705c3fec8a5745815 -->
These principles apply throughout implementation, review, and verification.
Use the project's existing task workflow; Backlog.md applies when configured.
Do not introduce a competing task or spec framework.

### Continuous application

- Read applicable repository instructions, linked principles, and local
  constraints before changing code. Reload after context loss, a change of
  repository or scope, or an update to those instructions.
- Identify the affected contracts, invariants, failure modes, and checks.
  Keep this proportional to risk; a small change needs no separate document.
- Preserve these constraints while implementing. Review the final diff for
  violations, including changes to tests, dependencies, and build settings.
- Never weaken checks, suppress diagnostics, or update expected results merely
  to make a failing change pass. Explain intentional contract changes.
- Keep justified exceptions narrow, explicit, and documented at their use.
  Follow repository approval requirements; do not invent additional gates.
- Verify the final code state. Report skipped checks, pre-existing failures,
  and unresolved risks. Do not claim guarantees beyond the evidence.

### Design and scope

- State material assumptions. Resolve consequential ambiguity before dependent
  work; proceed with reasonable defaults for reversible implementation details.
- Prefer the simplest complete solution. Avoid speculative features, config,
  and abstractions. Optimize for clarity and maintenance, not line count.
- Make the smallest complete change: include necessary callers, tests, and
  documentation. Match local style; avoid unrelated cleanup.
- Encode meaningful invariants in types and visibility where practical.
  Inspect every construction path, including builders and deserialization.
  A validated value does not prove that mutable external state stays valid.
- Keep domain logic independent of I/O where useful. Make time, randomness,
  and scheduling controllable when correctness depends on them. Do not create
  interfaces or crates solely to satisfy an architectural pattern.
- Treat public APIs, CLI behavior, serialized data, and storage formats as
  contracts. Document intentional incompatibility and its migration path.

### Failure and resource handling

- Validate untrusted input at boundaries. Never silently discard errors or
  substitute defaults when that hides invalid input or partial failure.
- Define observable state after errors, cancellation, and partial completion.
  Returning an error does not establish that an external action did not occur.
- Retry only when repeated effects are safe; bound attempts and elapsed time.
  Preserve useful error context without exposing secrets or duplicating logs.
- Bound input sizes, buffering, concurrency, and other resources exposed to
  untrusted or growing workloads. Define behavior when a limit is reached.
- Give resources and background work explicit owners and shutdown behavior.
  Check fallible completion explicitly; cleanup alone cannot report success.
- Distinguish atomic visibility from durability. Test the failure guarantees
  actually promised by the storage or external-service contract.

### Verification and evidence

- Use checks that exercise observable contracts, boundaries, and failure modes.
  Reproduce a bug with a regression test when feasible, then demonstrate the fix.
  Do not add tests that merely restate implementation or trivial edits.
- For critical logic, assess whether tests detect plausible wrong behavior;
  coverage alone is insufficient. Use targeted mutation testing when useful.
- Preserve minimized fuzz inputs, seeds, or event sequences as reproducible
  regressions, preferably in the normal test suite. Reruns are not a flaky-test fix.
- Select additional checks by risk: property/fuzz tests for parsers and state
  machines; fault injection for partial failures; schedule exploration for
  concurrency; Miri for exercised unsafe paths; bounded proofs for suitable
  critical components. Document model limits; none proves the whole system.
- Measure performance changes against a relevant baseline and realistic workload.
  Include correctness and resource costs; do not optimize from intuition alone.
- Run repository gates and checks relevant to affected supported configurations.
  After further edits, rerun affected checks; distinguish passed from not run.
- Justify new dependencies by capability, maintenance, features, transitive cost,
  licensing, security, and toolchain compatibility. Reuse is not automatic trust.
- Keep secrets and external tracker ids out of changes. Use the repository's
  language for comments and commits; otherwise use English.

### Rust

- Prefer safe Rust. Deny `unsafe_code` by default; use `forbid` for crates that
  must remain entirely safe. These settings do not ban unsafe in dependencies.
- Any permitted unsafe must be isolated and necessary, with a local safety
  argument covering its obligations. Document unsafe API caller obligations.
  Require evidence for performance justifications; never bypass ownership or
  thread-safety requirements to make code compile. Use explicit unsafe blocks
  inside unsafe functions and run applicable Miri tests; a pass is not proof.
- Deny Clippy `unwrap_used` and `expect_used` on production code. Test-only use
  may be allowed by the repository. A rare production exception requires a
  locally established invariant and a narrowly scoped lint exception.
- Review other panic paths too: indexing, assertions, arithmetic, and explicit
  panic. Expected input and operational failures belong in `Result`/`Option`.
  Do not hide errors with `.ok()`, discarded results, or fallback defaults.
- Enforce `unused_must_use`; use typed errors at reusable boundaries and useful
  context at application boundaries. Do not use catch-unwind as normal recovery.
- Prefer clear ownership and borrowing. Use `Clone`, `Arc`, and interior
  mutability deliberately; neither ban them nor use them to avoid design work.
- Keep locks short and avoid holding synchronous lock guards across `.await`.
  Choose mutex types from actual contention and suspension needs.
- Make async cancellation, task supervision, backpressure, and shutdown explicit.
  Dropping a handle or timing out does not establish that work stopped; already
  running blocking work may continue. Inspect the APIs' cancellation guarantees.
- Maintain `rust-version` and test affected supported features, targets, and
  MSRV. `--all-features` alone is not a feature matrix; test valid combinations.
  Use API compatibility checks for libraries, plus behavioral contract tests.
- Keep formatting and lint checks reproducible with the repository toolchain.
  Select useful Clippy lints; do not enable all restriction lints indiscriminately.
  Document local lint exceptions and avoid blanket crate-wide allowances.

### Minimal code
<!-- Ladder adapted from ponytail (MIT), github.com/DietrichGebert/ponytail -->
Choose the first suitable option that preserves the required contracts:
1. Is the behavior needed? If not, omit it.
2. Can existing project code supply it clearly? Reuse it.
3. Does the standard library fit the requirements? Prefer it.
4. Does an existing dependency or platform facility fit safely and portably?
   Compare their actual costs and guarantees; avoid duplicating working code.
5. Otherwise, add the smallest clear implementation or justified dependency.
Never cut trust-boundary validation, data-loss handling, security,
accessibility, or required verification for brevity.
<!-- /agent-dev-kb:coding-principles -->

### Project-specific principles

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

### Additional Rust rules
No `unwrap` / `expect` on production paths. Honor clippy (often
`-D warnings`). Write the failing test first when that area is
already tested.
