# AOEM FFI beta0.8 Integration (2026-03-01)

## Installed layout

Base host path:

- `D:\WorksArea\SUPERVM\aoem`

Repository policy:

- Git tracks only minimal host set (`core` dll + header + manifest + install info).
- Optional variant DLLs (`persist`, `wasm`) are distributed as GitHub Releases assets.

Default variant (for production baseline):

- `D:\WorksArea\SUPERVM\aoem\bin\aoem_ffi.dll`
- `D:\WorksArea\SUPERVM\aoem\include\aoem.h`

Optional variants:

- `persist`: `D:\WorksArea\SUPERVM\aoem\variants\persist\bin\aoem_ffi.dll`
- `wasm`: `D:\WorksArea\SUPERVM\aoem\variants\wasm\bin\aoem_ffi.dll`

## Release asset packaging

Use the release pack script to produce auditable assets (core + persist + wasm + checksums):

```powershell
powershell -File scripts/aoem/package_aoem_beta08.ps1
```

For full-platform release, build Linux/macOS variant libraries first (run on each target OS host/runner):

```powershell
# Run on Windows host
powershell -File scripts/aoem/build_aoem_variants_current_os.ps1 -- `
  -AoemSourceRoot <path-to-AOEM-source> -Platform windows

# Run on Linux host
pwsh -File scripts/aoem/build_aoem_variants_current_os.ps1 -- `
  -AoemSourceRoot <path-to-AOEM-source> -Platform linux

# Run on macOS host
pwsh -File scripts/aoem/build_aoem_variants_current_os.ps1 -- `
  -AoemSourceRoot <path-to-AOEM-source> -Platform macos
```

Native shell option for Linux/macOS hosts:

```bash
# Run on Linux host
bash scripts/aoem/build_aoem_variants_current_os.sh \
  --aoem-source-root <path-to-AOEM-source> \
  --platform linux

# Run on macOS host
bash scripts/aoem/build_aoem_variants_current_os.sh \
  --aoem-source-root <path-to-AOEM-source> \
  --platform macos
```

Build prerequisites for `persist` variant (RocksDB):

```bash
# Ubuntu/Debian
sudo apt-get update
sudo apt-get install -y \
  build-essential clang llvm-dev libclang-dev cmake pkg-config \
  zlib1g-dev libzstd-dev libbz2-dev
```

If `clang-sys` still cannot locate clang, set:

```bash
export LIBCLANG_PATH=/usr/lib/llvm-*/lib
```

WSL/no-sudo fallback (user-space libclang):

```bash
# 1) install pip to user site (PEP668-safe override)
curl -fsSLo ~/get-pip.py https://bootstrap.pypa.io/get-pip.py
python3 ~/get-pip.py --user --break-system-packages

# 2) install bundled libclang wheel
/usr/bin/python3 -m pip install --user --break-system-packages libclang

# 3) provide soname expected by some build scripts
ln -sf ~/.local/lib/python3.12/site-packages/clang/native/libclang.so \
      ~/.local/lib/python3.12/site-packages/clang/native/libclang.so.18.1

# 4) build persist with explicit bindgen include hints
LIBCLANG_PATH=~/.local/lib/python3.12/site-packages/clang/native \
LD_LIBRARY_PATH=~/.local/lib/python3.12/site-packages/clang/native \
BINDGEN_EXTRA_CLANG_ARGS='-I/usr/lib/gcc/x86_64-linux-gnu/13/include -I/usr/include -I/usr/include/x86_64-linux-gnu' \
bash scripts/aoem/build_aoem_variants_current_os.sh \
  --aoem-source-root <path-to-AOEM-source> \
  --platform linux \
  --variants persist
```

macOS host baseline:

```bash
xcode-select --install
brew install llvm cmake pkg-config zstd bzip2
export LIBCLANG_PATH="$(brew --prefix llvm)/lib"
```

Then package with full-platform gate:

```powershell
powershell -File scripts/aoem/package_aoem_beta08.ps1 -RequireFullPlatform
```

If current release intentionally excludes macOS, package Windows+Linux only:

```powershell
powershell -File scripts/aoem/package_aoem_beta08.ps1 -SkipMacOS
```

Outputs:

- `artifacts/aoem-beta08/<timestamp>/` (bundle directory)
- `artifacts/aoem-beta08/aoem-beta0.8-<timestamp>.zip` (release upload artifact)
- `SHA256SUMS`, `aoem-manifest.json`, `RELEASE-INDEX.md`

Source AOEM bundle:

- `D:\WorksArea\AOEM\artifacts\aoem-beta08\20260301-221150`

## NOVOVM binding updates

Updated crate:

- `crates/aoem-bindings`

Changes:

- FFI signature aligned with AOEM ABI v1/V2:
  - `aoem_create`
  - `aoem_destroy`
  - `aoem_execute_ops_v2(handle, aoem_op_v2*, op_count, aoem_exec_v2_result*)`
  - `aoem_recommend_parallelism(txs, batch, key_space, rw)`
  - `aoem_create_with_options(aoem_create_options_v1*)` (ingress worker override)
  - `aoem_last_error`
  - `aoem_abi_version`
  - `aoem_version_string`
  - `aoem_capabilities_json`
- NOVOVM host binding (clean path):
  - `AoemDyn::capabilities()`
  - `AoemDyn::create_handle()`
- `AoemHandle::execute_ops_v2()` (canonical perf path, typed binary ABI)
- Added perf example:
  - `crates/aoem-bindings/examples/ffi_perf_smoke.rs`
  - `crates/aoem-bindings/examples/ffi_perf_worldline.rs`
- Startup hard gate in bindings:
  - `AoemDyn::load()` now rejects DLLs when `aoem_abi_version != 1`
  - `AoemDyn::load()` now rejects DLLs when `capabilities.execute_ops_v2 != true`
  - `AoemDyn::load()` now verifies DLL hash against manifest (`aoem/manifest/aoem-manifest.json`) when present
- set `AOEM_DLL_MANIFEST_REQUIRED=1` to force manifest presence; set `AOEM_DLL_MANIFEST=<path>` to override path

## NOVOVM mainline boundary alignment (2026-04-05)

1. AOEM FFI is the execution/kernel boundary only.  
2. Overlay routing governance (`secure|fast`, multi-hop, relay bucket/set/rotation) belongs to node/gateway/plugin host path, not AOEM ABI fields.  
3. Mainline production entry is now `novovmctl` (`novovmctl up` for foreground, `novovmctl daemon` for supervised production) with production default `NOVOVM_OVERLAY_ROUTE_MODE=secure`; AOEM stays transport-agnostic.  
4. Binary ingress/response is still the production baseline; JSON path remains compatibility/debug only.  

## Perf smoke (method only)

Command shape:

```powershell
cargo run --release --example ffi_perf_smoke -- --dll <path> --warmup 50 --iters 500 --points 1100
```

Metric definition:
- `tps_*` = `ops_per_s` (operations per second), no `plans_tps` output.

For current measured numbers, use sealed report only:
- `docs_CN/AOEM-FFI-BETA08-TPS-SEAL-2026-03-02.md`

## AOEM reference comparison notes

- This benchmark is "NOVOVM host -> AOEM FFI -> AOEM engine" with a fixed micro payload (`points=1100`), single caller thread.
- `tps_*` here is micro-payload ops/s, including binary envelope encode/decode and FFI call overhead.
- It is not directly comparable to AOEM kernel worldline throughput in `Flow-Autopsy.md`
  (`A1 full`, `txs=1,000,000`, `threads=16`, ingress batching).
- For strict apples-to-apples with AOEM worldline, run a dedicated multi-thread host benchmark with the same `txs/threads/key_space/rw` tuple.

## Notes

- This benchmark is host-to-FFI plan execution smoke, not blockchain E2E throughput.
- Variant selection should always be validated by `aoem_capabilities_json()` before node startup.
- Recommended default for immediate NOVOVM integration: `core` (or `persist` if durable state path is required now).
- Throughput is power-sensitive on laptop platforms; always record `PowerOnline` and active power plan with TPS.
- AOEM FFI production default is binary input path. JSON input is compatibility/debug only.
  - default: `json_input_enabled=false`
  - temporary enable: set `AOEM_FFI_ALLOW_JSON=1`
- AOEM FFI production default is binary response path. JSON response is compatibility/debug only.
  - default: `json_response_enabled=false`
  - temporary enable: set `AOEM_FFI_RESPONSE_JSON=1`
- The same binary-path policy applies to `persist` variant as well (`execute_ops_v2=true`, JSON flags false in generated profile).
- Current `wasm` variant package is in V2 throughput lane (`execute_ops_v2=true`, JSON flags false) and can be measured with the same worldline matrix.
- `threads=auto` in `ffi_perf_worldline` now uses AOEM FFI single-source recommendation
  (`aoem_recommend_parallelism`), not host-local hardcoded logic.
- `threads=auto` / `engine-workers=auto` now use joint adaptive selection with budget guard:
  - choose `(threads, engine_workers)` together
  - enforce `threads * engine_workers <= budget_threads`
  - avoids over-subscription patterns like `16 x 16`
- `ffi_perf_worldline` default preset is now single-engine parity worldline:
  - `--preset cpu_parity` (default)
  - implicit defaults: `threads=1`, `engine_workers=16`
  - multi-handle aggregate must be explicit: `--preset cpu_batch_stress`
- worldline naming (recommended):
  - `cpu_parity_single`: `preset=cpu_parity`, `submit_ops=1`, `threads=1`
  - `cpu_parity_auto_parallel`: `preset=cpu_parity`, `submit_ops=1`, `threads=auto`
  - `cpu_batch_stress_single`: `preset=cpu_batch_stress`, `submit_ops=1024`, `threads=1`
  - `cpu_batch_stress_auto_parallel`: `preset=cpu_batch_stress`, `submit_ops=1024`, `threads=auto`

## Install-time runtime profile (recommended)

To avoid re-probing on every run, generate an install-time profile once and persist it:

```powershell
cargo run --release --example ffi_install_probe -- `
  --dll D:\WorksArea\SUPERVM\aoem\bin\aoem_ffi.dll
```

Default generated profile path:

- `D:\WorksArea\SUPERVM\aoem\config\aoem-runtime-profile.json`

Runtime behavior in `aoem-bindings`:

1. first load tries install profile (`reason=aoem_install_profile`);
2. if profile missing/invalid, fallback to AOEM FFI online recommendation;
3. if FFI symbol unavailable, fallback to host-safe heuristic.

Optional override:

- `AOEM_RUNTIME_PROFILE=<absolute-path-to-profile.json>`

## Binary worldline throughput (A1-aligned, high throughput path, FFI V2)

Use typed V2 binary ops (no JSON encode/decode in host loop):

```powershell
cargo run --release --example ffi_perf_worldline -- `
  --preset cpu_parity `
  --dll <path-to-aoem_ffi.dll> `
  --txs 1000000 --key-space 128 --rw 0.5 --submit-ops 1 --seed 123 --warmup-calls 5 `
  --threads 1 --engine-workers 16
```

Observed numbers are intentionally not duplicated here to avoid stale baselines.
Use sealed report:
- `docs_CN/AOEM-FFI-BETA08-TPS-SEAL-2026-03-02.md`

Important notes:

- This path uses `aoem_execute_ops_v2` typed ABI through FFI.
- JSON input/response is not in the hot path.
- Default in this example is single-engine worldline (`preset=cpu_parity`).
- `cpu_parity` now uses single-op submit semantics by default (`submit_ops=1`).
- Aggregate stress lane must be explicit (`preset=cpu_batch_stress`).
- A1 baseline is AOEM native process-internal worldline (`aoem_kernel_baseline`), maintained in AOEM docs.

Latest sealed baseline:

- `docs_CN/AOEM-FFI-BETA08-TPS-SEAL-2026-03-02.md`
- publish default KPI: `core + preset=cpu_parity` only.

### Legacy envelope APIs (`aoem_execute_batch` / `aoem_execute`)

These APIs are kept for compatibility and diagnostics, not as the default perf route.

Current guidance:
- default throughput worldline uses FFI V2 typed ops.
- old envelope benchmark parameters are removed from `ffi_perf_worldline`.
