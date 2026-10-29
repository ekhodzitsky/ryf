#!/bin/sh
# Execute the public API checks without cross-compiling benchmark dependencies.
set -eu
check_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
check_target=${1:?usage: check-portable.sh TARGET}
case "$check_target" in
    i686-unknown-linux-musl) check_runner=qemu-i386 ;;
    aarch64-unknown-linux-musl) check_runner=qemu-aarch64 ;;
    *) echo "Unsupported execution target: $check_target" >&2; exit 2 ;;
esac
check_host=$(rustc -vV | sed -n 's/^host: //p')
check_linker=$(rustc --print sysroot)/lib/rustlib/$check_host/bin/rust-lld
check_dir=$check_root/target/portable-check
mkdir -p "$check_dir"
cat > "$check_dir/Cargo.toml" <<'EOF'
[package]
name = "ryf-portability-check"
version = "0.0.0"
edition = "2024"
[dependencies]
ryf = { path = "../..", default-features = false, features = ["adpcm"] }
[features]
default = ["simd"]
simd = ["ryf/simd"]
[[bin]]
name = "portable"
path = "../../tools/portable.rs"
EOF
# Capture results separately: POSIX pipelines would hide a failing cargo run.
cargo run --quiet --release --manifest-path "$check_dir/Cargo.toml" \
    --target "$check_target" \
    --config "target.$check_target.linker=\"$check_linker\"" \
    --config "target.$check_target.runner=\"$check_runner\"" \
    > "$check_dir/$check_target.simd.txt"
cargo run --quiet --release --no-default-features \
    --manifest-path "$check_dir/Cargo.toml" --target "$check_target" \
    --config "target.$check_target.linker=\"$check_linker\"" \
    --config "target.$check_target.runner=\"$check_runner\"" \
    > "$check_dir/$check_target.scalar.txt"
cmp "$check_dir/$check_target.simd.txt" "$check_dir/$check_target.scalar.txt"
cat "$check_dir/$check_target.simd.txt"
