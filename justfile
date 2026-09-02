test:
    cargo test --lib
    cargo test --doc

fmt:
    cargo fmt --check

clippy:
    cargo clippy --all-targets -- -D warnings

cov:
    cargo llvm-cov --lib --ignore-filename-regex '_tests|proptest' --summary-only -- --skip proptest

bench:
    cargo bench --bench wav

# Same harness plus vendored dr_wav (C). Needs a C compiler. Not product path.
bench-c:
    cargo bench --bench wav --features bench-c

check: fmt clippy test
