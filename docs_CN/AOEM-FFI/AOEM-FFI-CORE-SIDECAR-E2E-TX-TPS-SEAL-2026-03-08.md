# AOEM Core + Sidecar Host-Pipeline TPS Diagnostic (2026-03-08)

> Diagnostic-only report. Do not use as AOEM kernel TPS KPI.

## Scope

- Host pipeline via `novovm-node` production ingress (`tx_wire_file -> ops_encode -> aoem_submit`).
- AOEM is always loaded from core dll; persist/wasm are sidecar runtime profiles.
- TPS metric is host wall-clock: `processed_tx / wall_time`.
- This is NOT pure kernel TPS and NOT network/consensus E2E TPS.
- Quantiles: P50 / P90 / P99 (nearest-rank).

## Fixed Params

- txs per run: 8
- accounts: 2
- repeats: 1
- timeout_sec: 180
- batch_count: 1
- ingress_workers: 16
- adapter_signal_mode: fast
- profiles: core_only
- build_profile: release
- aoem_plugin_dir: D:\WEB3_AI\SUPERVM\aoem\plugins

## Matrix

| profile | persist_backend | wasm_runtime | runtime_variant | ingress_workers | runs | pass_runs | wall_tps_p50 | wall_tps_p90 | wall_tps_p99 |
|---|---|---|---|---:|---:|---:|---:|---:|---:|
| core_only | none | none | core | 16 | 1 | 1 | 163.12 | 163.12 | 163.12 |

## Stage P50 (ms)

| profile | mempool_ms_p50 | adapter_ms_p50 | aoem_submit_ms_p50 | batch_a_ms_p50 | network_ms_p50 | stage_total_ms_p50 |
|---|---:|---:|---:|---:|---:|---:|
| core_only |  |  |  |  |  |  |

## Overhead P50

| profile | non_engine_pct_p50 | bootstrap_ms_p50 |
|---|---:|---:|
| core_only |  |  |

## Reproduce

```powershell
& scripts/migration/run_tx_e2e_tps_core_sidecar_report.ps1 -RepoRoot D:\WEB3_AI\SUPERVM -Repeats 1 -Txs 8 -Accounts 2 -TimeoutSec 180 -BatchCount 1 -IngressWorkers 16 -AdapterSignalMode fast -Profiles 'core_only' -AoemPluginDir D:\WEB3_AI\SUPERVM\aoem\plugins -BuildProfile release
```

## Artifacts

- D:\WEB3_AI\SUPERVM\artifacts\migration\tmp-tx-e2e-wire-check5\tx-e2e-tps-summary.json
- D:\WEB3_AI\SUPERVM\docs_CN\AOEM-FFI\AOEM-FFI-CORE-SIDECAR-E2E-TX-TPS-RAW-2026-03-08.csv
