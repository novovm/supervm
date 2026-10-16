#!/usr/bin/env bash
# Isolated comparison tools, never a NOVOVM execution dependency.
set -euo pipefail
root="$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"
base="$root/target/reference-agave"
source_dir="$base/v4.3.0/source-linux"
expected=825efd18292aff6ffcf9daa0f7612f21b3531a72
test "$(git -C "$source_dir" rev-parse HEAD)" = "$expected"
test -z "$(git -C "$source_dir" status --porcelain)"
available_kib=$(df -Pk "$base" | awk 'NR==2 {print $4}')
if (( available_kib < 40 * 1024 * 1024 )); then
  echo 'Need at least 40 GiB free before the reference build; no cache deletion.' >&2
  exit 1
fi
export RUSTUP_HOME="$base/rustup"
export CARGO_HOME="$base/cargo-home"
export CARGO_TARGET_DIR="$base/build"
export PROTOC="$base/sysroot/usr/bin/protoc"
export PROTOC_INCLUDE="$base/sysroot/usr/include"
export PKG_CONFIG_PATH="$base/sysroot/usr/lib/x86_64-linux-gnu/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
export LD_LIBRARY_PATH="$base/sysroot/usr/lib/x86_64-linux-gnu${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export RUSTFLAGS="-L native=$base/sysroot/usr/lib/x86_64-linux-gnu"
cd "$source_dir"
cargo +1.97.1 build --release --locked -j 4 -p agave-validator -p solana-genesis -p solana-gossip
