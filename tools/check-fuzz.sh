#!/bin/sh
# Optional ASan validation; no generated manifest or corpus enters the product.
set -eu
fuzz_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
fuzz_target=${1:?usage: check-fuzz.sh containers|adpcm [seconds]}
fuzz_seconds=${2:-300}
case "$fuzz_target" in
    containers) fuzz_length=8192 ;;
    adpcm) fuzz_length=2048 ;;
    *) echo "Unknown fuzz target: $fuzz_target" >&2; exit 2 ;;
esac
case "$fuzz_seconds" in ''|*[!0-9]*) echo "Seconds must be a positive integer" >&2; exit 2 ;; esac
test "$fuzz_seconds" -gt 0
cd "$fuzz_root"
fuzz_dir=$fuzz_root/target/fuzz-check
mkdir -p "$fuzz_dir"
cat > "$fuzz_dir/Cargo.toml" <<'EOF'
[package]
name = "ryf-fuzz-check"
version = "0.0.0"
edition = "2024"
[package.metadata]
cargo-fuzz = true
[dependencies]
libfuzzer-sys = "=0.4.13"
ryf = { path = "../.." }
[[bin]]
name = "containers"
path = "../../tools/fuzz/containers.rs"
test = false
doc = false
bench = false
[[bin]]
name = "adpcm"
path = "../../tools/fuzz/adpcm.rs"
test = false
doc = false
bench = false
EOF
fuzz_host=$(rustc -vV | sed -n 's/^host: //p')
cargo build --quiet --lib --target "$fuzz_host" --target-dir "$fuzz_dir/preflight"
fuzz_lib=$fuzz_dir/preflight/$fuzz_host/debug
rustc --edition 2024 tools/fuzz/seeds.rs --extern "ryf=$fuzz_lib/libryf.rlib" \
    -L "dependency=$fuzz_lib/deps" -o "$fuzz_dir/seeds"
"$fuzz_dir/seeds" "$fuzz_dir/corpus"
cargo +nightly fuzz run "$fuzz_target" --fuzz-dir "$fuzz_dir" -- \
    "-max_total_time=$fuzz_seconds" "-max_len=$fuzz_length" -rss_limit_mb=1024
