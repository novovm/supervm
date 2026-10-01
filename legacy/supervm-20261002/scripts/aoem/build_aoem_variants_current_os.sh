#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
supervm_root="$(cd "$script_dir/../.." && pwd)"

aoem_source_root="${AOEM_SOURCE_ROOT:-$supervm_root/../AOEM}"
out_root="${OUT_ROOT:-$supervm_root/artifacts/aoem-platform-build}"
platform="auto"
clean=0
variants=("core" "persist" "wasm")

usage() {
  cat <<'EOF'
Usage:
  build_aoem_variants_current_os.sh [options]

Options:
  --aoem-source-root <path>   AOEM source root (default: $AOEM_SOURCE_ROOT or ../AOEM)
  --out-root <path>           Output root (default: artifacts/aoem-platform-build)
  --platform <auto|linux|macos>
  --variants <csv>            Variants list (default: core,persist,wasm)
  --clean                     Remove per-variant target dirs before build
  -h, --help                  Show this help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --aoem-source-root)
      aoem_source_root="$2"
      shift 2
      ;;
    --out-root)
      out_root="$2"
      shift 2
      ;;
    --platform)
      platform="$2"
      shift 2
      ;;
    --variants)
      IFS=',' read -r -a variants <<<"$2"
      shift 2
      ;;
    --clean)
      clean=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown arg: $1" >&2
      usage
      exit 2
      ;;
  esac
done

if [[ ! -f "$aoem_source_root/crates/ffi/aoem-ffi/Cargo.toml" ]]; then
  echo "missing AOEM FFI Cargo.toml under: $aoem_source_root" >&2
  exit 1
fi

detect_platform() {
  if [[ "$platform" != "auto" ]]; then
    echo "$platform"
    return
  fi
  case "$(uname -s)" in
    Linux) echo "linux" ;;
    Darwin) echo "macos" ;;
    *)
      echo "unsupported host: $(uname -s), set --platform explicitly" >&2
      exit 1
      ;;
  esac
}

variant_features() {
  case "$1" in
    core) echo "" ;;
    persist) echo "rocksdb-persistence" ;;
    wasm) echo "wasmtime-runtime" ;;
    *)
      echo "unsupported variant: $1" >&2
      exit 1
      ;;
  esac
}

platform="$(detect_platform)"
case "$platform" in
  linux) lib_name="libaoem_ffi.so" ;;
  macos) lib_name="libaoem_ffi.dylib" ;;
  *)
    echo "unsupported platform: $platform" >&2
    exit 1
    ;;
esac

manifest="$aoem_source_root/crates/ffi/aoem-ffi/Cargo.toml"
header_src="$aoem_source_root/crates/ffi/aoem-ffi/include/aoem.h"
if [[ ! -f "$header_src" ]]; then
  echo "missing aoem.h: $header_src" >&2
  exit 1
fi

for variant in "${variants[@]}"; do
  features="$(variant_features "$variant")"
  target_dir="$aoem_source_root/cargo-target-ffi-${platform}-${variant}"

  if [[ "$clean" == "1" && -d "$target_dir" ]]; then
    rm -rf "$target_dir"
  fi

  cmd=(cargo build --release --manifest-path "$manifest" --target-dir "$target_dir")
  if [[ -n "$features" ]]; then
    cmd+=(--features "$features")
  fi
  "${cmd[@]}"

  built_lib="$target_dir/release/$lib_name"
  if [[ ! -f "$built_lib" ]]; then
    echo "build output missing for variant=$variant: $built_lib" >&2
    exit 1
  fi

  dst_lib_dir="$out_root/$platform/$variant/bin"
  mkdir -p "$dst_lib_dir"
  cp -f "$built_lib" "$dst_lib_dir/$lib_name"
done

dst_inc_dir="$out_root/$platform/include"
mkdir -p "$dst_inc_dir"
cp -f "$header_src" "$dst_inc_dir/aoem.h"

meta_path="$out_root/$platform/BUILD-META.json"
mkdir -p "$(dirname "$meta_path")"
cat >"$meta_path" <<EOF
{
  "platform": "$platform",
  "variants": ["$(IFS='","'; echo "${variants[*]}")"],
  "aoem_source_root": "$aoem_source_root",
  "output_root": "$out_root",
  "library_name": "$lib_name"
}
EOF

echo "build_ready_platform=$platform"
echo "build_ready_root=$out_root"
