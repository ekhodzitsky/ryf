test:
    cargo test --lib

fmt:
    cargo fmt --check

clippy:
    cargo clippy --all-targets -- -D warnings

cov:
    cargo llvm-cov --lib --ignore-filename-regex '_tests\.rs$|wav_tests|wav_proptest' --summary-only -- --skip proptest

check: fmt clippy test
