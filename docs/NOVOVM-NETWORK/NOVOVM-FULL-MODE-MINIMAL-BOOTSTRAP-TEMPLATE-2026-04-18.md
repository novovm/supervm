# NOVOVM Full-Mode Minimal Bootstrap Template (2026-04-18)

Status: OPERATIONAL TEMPLATE  
Scope: Production `novovm-node` full-path startup with one ingress source + P2-D daily report

## Purpose

This template provides the minimal, reproducible startup path for `novovm-node` production full mode.

It avoids non-production node modes and keeps P3 disabled (`Decision Only / Not Enabled`).

## Hard runtime constraints (from code contract)

1. `novovm-node` only accepts `NOVOVM_NODE_MODE=full`.
2. `NOVOVM_EXEC_PATH` must be `ffi_v2`.
3. `NOVOVM_ENABLE_HOST_ADMISSION=1` is not allowed.
4. Exactly one ingress source must be set:
   - `NOVOVM_TX_WIRE_FILE`
   - `NOVOVM_OPS_WIRE_FILE`
   - `NOVOVM_OPS_WIRE_DIR`

Reference contracts:
- `crates/novovm-node/src/bin/novovm-node.rs` (full-mode and ingress guard rails)
- `crates/novovm-exec/src/lib.rs` (`AoemRuntimeConfig::from_env` defaults and AOEM path resolution)

## Step 1: Prepare minimal ingress input

Generate a `.txwire` file with a small deterministic sample:

```powershell
cargo run -p novovm-bench --bin novovm-txgen -- `
  --out artifacts/mainline/full-mode/ingress.txwire.bin `
  --txs 128 `
  --accounts 16
```

## Step 2: Set minimal environment (PowerShell)

```powershell
$env:NOVOVM_NODE_MODE = "full"
$env:NOVOVM_EXEC_PATH = "ffi_v2"
$env:NOVOVM_ENABLE_HOST_ADMISSION = "0"

$env:NOVOVM_TX_WIRE_FILE = (Resolve-Path "artifacts/mainline/full-mode/ingress.txwire.bin").Path

Remove-Item Env:NOVOVM_OPS_WIRE_FILE -ErrorAction SilentlyContinue
Remove-Item Env:NOVOVM_OPS_WIRE_DIR -ErrorAction SilentlyContinue
```

If AOEM auto-discovery fails in your machine layout, set one of these:

```powershell
$env:NOVOVM_AOEM_ROOT = "D:\WEB3_AI\SUPERVM\aoem"
# or
$env:NOVOVM_AOEM_DLL = "D:\WEB3_AI\SUPERVM\aoem\core\bin\aoem_ffi.dll"
```

## Step 3: Start production full-path node

```powershell
cargo run -p novovm-node --bin novovm-node
```

Expected startup markers include lines like:
- `d1_ingress_contract: ...`
- `tx_ingress_source: ...`
- `mode=ffi_v2 ...`

## Step 4: Run P2-D daily report

In a separate terminal:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/novovm-p2d-daily-report.ps1 `
  -RpcUrl "http://127.0.0.1:8899"
```

Notes:
- The script prefers RPC.
- If RPC endpoint is unavailable, it falls back to `supervm-mainline-query`.
- Output directory:
  - `artifacts/mainline/p2d-run-phase/<YYYY-MM-DD>/`

## Ingress variants and constraints

1. `NOVOVM_TX_WIRE_FILE`
   - Simplest for full-mode startup.
2. `NOVOVM_OPS_WIRE_FILE`
   - Requires `ops_wire_v1` path selection.
3. `NOVOVM_OPS_WIRE_DIR`
   - Directory must contain `.opsw1` files.
   - Requires `NOVOVM_TX_REPEAT_COUNT=1`.

Do not set more than one ingress variable at once.

## Out of scope

- Enabling P3-A/P3-B/P3-C.
- Re-enabling non-full node modes.
- Changing P2-C/P2-D sealed semantics.

