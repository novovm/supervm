# AOEM Production Persistent Pipeline And Metrics (2026-03-08)

## 1. Purpose

This document fixes the production-path execution contract:

- process-level init is one-time (`aoem_global_init`)
- AOEM handle/session is long-lived and reused
- TPS metrics are split into strict scopes (kernel / host diag / consensus+network e2e)

This avoids cold-start noise polluting production TPS.

## 2. Persistent Pipeline

```text
+-----------------------------------------------+
| Host process startup                          |
| - load aoem_ffi.dll                           |
| - aoem_global_init() (once, idempotent)       |
+------------------------------+----------------+
                               |
                               v
+-----------------------------------------------+
| AOEM runtime/session creation                 |
| - AoemExecFacade::open()                      |
| - create_session()                            |
| - session/handle reused                        |
+------------------------------+----------------+
                               |
                               v
+-----------------------------------------------+
| Per-batch execution                           |
| - tx ingress read                             |
| - minimal ExecOp marshaling                   |
| - aoem_execute_ops_v2                         |
| - AOEM returns processed/success/writes/etc   |
+------------------------------+----------------+
                               |
                               v
+-----------------------------------------------+
| Chain binding / commit                         |
| - minimal host-side binding only              |
| - consensus/network path (when enabled)       |
+-----------------------------------------------+
```

## 3. Metric Contract

### 3.1 `aoem_kernel_tps`

- Source: AOEM `elapsed_us` (from `mode=ffi_v2 ... elapsed_us=...`)
- Scope: AOEM steady-state execute path (`execute_ops_v2`)
- Excludes: process startup, DLL load

### 3.2 `host_pipeline_diag_tps`

- Source: host wall-clock per run
- Scope: diagnostic upper bound for host path
- Includes: process startup, DLL load, tx wire read, host marshaling
- Use: diagnosis only, not publish-grade kernel TPS

### 3.3 `consensus_network_e2e_tps`

- Source: multi-node full-chain run (network + consensus + execution + commit)
- This is the production end-to-end business TPS target
- In single-node `ffi_v2` script this field stays null by design

## 4. Integration Status

- AOEM exports `aoem_global_init` and keeps it idempotent.
- SUPERVM host (`aoem-bindings`) calls global init after DLL load.
- Production script now writes explicit metric scopes into summary/report.

## 5. Publish Rules

- Never compare `host_pipeline_diag_tps` with `aoem_kernel_tps` as same-layer numbers.
- Use `aoem_kernel_tps` for AOEM core seal.
- Use `consensus_network_e2e_tps` for full-chain production seal.
- If script repeats launch fresh processes, cold-start cost remains in host diag by definition.

## 6. Recommended Scripts

- Single-node production path (includes cold start in host diag):
  - `scripts/migration/run_prod_node_e2e_tps.ps1`
- Single-process steady path (reuse one process/session, excludes cold start in steady host diag):
  - `scripts/migration/run_prod_node_steady_tps.ps1`
- Consensus + network + AOEM multi-node simulation e2e:
  - `scripts/migration/run_consensus_network_e2e_tps.ps1`
