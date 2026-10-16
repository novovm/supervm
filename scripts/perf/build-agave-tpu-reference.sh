#!/usr/bin/env bash
set -euo pipefail
root="$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"
base="$root/target/reference-agave"
# Extract the pinned crate here and add only an empty [workspace] table to its
# manifest so Cargo cannot attach this reference tool to the product workspace.
available_kib=$(df -Pk "$base" | awk 'NR==2 {print $4}')
(( available_kib >= 20 * 1024 * 1024 )) || { echo 'Need 20 GiB free; no cache deletion.' >&2; exit 1; }
archive="$base/solana-transaction-bench-0.1.0.crate"
test "$(sha256sum "$archive" | cut -d ' ' -f 1)" = 9ab43a2e638a761a854954f3b579d19702a6d72da45dbd3ccc9fa31ede64d997
export RUSTUP_HOME="$base/rustup"
export CARGO_HOME="$base/cargo-home"
export CARGO_TARGET_DIR="$base/build"
export PROTOC="$base/sysroot/usr/bin/protoc"
export PROTOC_INCLUDE="$base/sysroot/usr/include"
export CPATH="$base/sysroot/usr/include${CPATH:+:$CPATH}"
export PKG_CONFIG_PATH="$base/sysroot/usr/lib/x86_64-linux-gnu/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
export LD_LIBRARY_PATH="$base/sysroot/usr/lib/x86_64-linux-gnu${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export RUSTFLAGS="-L native=$base/sysroot/usr/lib/x86_64-linux-gnu"
export NOVOVM_AGAVE_ADAPTER_ROOT="$root"
cd "$base/solana-transaction-bench-0.1.0"
if ! git apply --reverse --check "$root/scripts/perf/agave-fixed-load.patch" 2>/dev/null; then
  git apply --check "$root/scripts/perf/agave-fixed-load.patch"
  git apply "$root/scripts/perf/agave-fixed-load.patch"
fi
if ! git apply --reverse --check "$root/scripts/perf/agave-queue-trace.patch" 2>/dev/null; then
  git apply --check "$root/scripts/perf/agave-queue-trace.patch"
  git apply "$root/scripts/perf/agave-queue-trace.patch"
fi
if ! git apply --reverse --check "$root/scripts/perf/agave-sequential-amounts.patch" 2>/dev/null; then
  git apply --check "$root/scripts/perf/agave-sequential-amounts.patch"
  git apply "$root/scripts/perf/agave-sequential-amounts.patch"
fi
cargo +1.97.1 build --release --locked -j 4 --bin solana-transaction-bench
