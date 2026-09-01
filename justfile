test:
    cargo test --lib

fmt:
    cargo fmt --check

clippy:
    cargo clippy --all-targets -- -D warnings

check: fmt clippy test
