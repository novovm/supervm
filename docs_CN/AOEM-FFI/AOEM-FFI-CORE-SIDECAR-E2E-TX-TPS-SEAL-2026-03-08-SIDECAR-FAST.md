# AOEM Core + Sidecar TX E2E TPS Seal (2026-03-08)

## Scope

- Real tx path via `novovm-node` (`tx_codec -> mempool -> batch_a -> commit -> network`).
- AOEM is always loaded from core dll; persist/wasm are sidecar runtime profiles.
- TPS metric is wall-clock: `processed_tx / wall_time`.
- Quantiles: P50 / P90 / P99 (nearest-rank).

## Fixed Params

- txs per run: 10000
- accounts: 1024
- repeats: 1
- timeout_sec: 300
- batch_count: 64
- ingress_workers: 32
- adapter_signal_mode: fast
- profiles: core_only,core_persist,core_wasm
- build_profile: release
- aoem_plugin_dir: D:\WEB3_AI\SUPERVM\aoem\plugins

## Matrix

| profile | persist_backend | wasm_runtime | runtime_variant | ingress_workers | runs | pass_runs | wall_tps_p50 | wall_tps_p90 | wall_tps_p99 |
|---|---|---|---|---:|---:|---:|---:|---:|---:|
| core_only | none | none | core | 32 | 1 | 1 | 87327.09 | 87327.09 | 87327.09 |
| core_persist | rocksdb | none | core | 32 | 1 | 1 | 96818.26 | 96818.26 | 96818.26 |
| core_wasm | none | wasmtime | core | 32 | 1 | 1 | 97693.74 | 97693.74 | 97693.74 |

## Stage P50 (ms)

| profile | mempool_ms_p50 | adapter_ms_p50 | aoem_submit_ms_p50 | batch_a_ms_p50 | network_ms_p50 | stage_total_ms_p50 |
|---|---:|---:|---:|---:|---:|---:|
| core_only | 1 | 4.11 | 0.38 | 70.34 | 0.35 | 88.21 |
| core_persist | 1.02 | 3.89 | 0.35 | 64.99 | 0.4 | 84.92 |
| core_wasm | 1 | 3.43 | 0.38 | 64.42 | 0.21 | 83.8 |

## Overhead P50

| profile | non_engine_pct_p50 | bootstrap_ms_p50 |
|---|---:|---:|
| core_only | 99.57 | 26.3 |
| core_persist | 99.59 | 18.36 |
| core_wasm | 99.55 | 18.56 |

## Reproduce

```powershell
& scripts/migration/run_tx_e2e_tps_core_sidecar_report.ps1 -RepoRoot D:\WEB3_AI\SUPERVM -Repeats 1 -Txs 10000 -Accounts 1024 -TimeoutSec 300 -BatchCount 64 -IngressWorkers 32 -AdapterSignalMode fast -Profiles 'core_only,core_persist,core_wasm' -AoemPluginDir D:\WEB3_AI\SUPERVM\aoem\plugins -BuildProfile release
```

## Artifacts

- d:\WEB3_AI\SUPERVM\artifacts\migration\tx-e2e-tps-core-sidecar-2026-03-08-sidecar-fast\tx-e2e-tps-summary.json
- d:\WEB3_AI\SUPERVM\docs_CN\AOEM-FFI\AOEM-FFI-CORE-SIDECAR-E2E-TX-TPS-RAW-2026-03-08-SIDECAR-FAST.csv
