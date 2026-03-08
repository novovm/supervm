# AOEM Core + Sidecar TX E2E TPS Seal (2026-03-08)

## Scope

- Real tx path via `novovm-node` (`tx_codec -> mempool -> batch_a -> commit -> network`).
- AOEM is always loaded from core dll; persist/wasm are sidecar runtime profiles.
- TPS metric is wall-clock: `processed_tx / wall_time`.
- Quantiles: P50 / P90 / P99 (nearest-rank).

## Fixed Params

- txs per run: 1000
- accounts: 256
- repeats: 1
- timeout_sec: 120
- batch_count: 16
- ingress_workers: 16
- adapter_signal_mode: fast
- profiles: core_only
- build_profile: release
- aoem_plugin_dir: D:\WEB3_AI\SUPERVM\aoem\plugins

## Matrix

| profile | persist_backend | wasm_runtime | runtime_variant | ingress_workers | runs | pass_runs | wall_tps_p50 | wall_tps_p90 | wall_tps_p99 |
|---|---|---|---|---:|---:|---:|---:|---:|---:|
| core_only | none | none | core | 16 | 1 | 1 | 176.68 | 176.68 | 176.68 |

## Stage P50 (ms)

| profile | mempool_ms_p50 | adapter_ms_p50 | aoem_submit_ms_p50 | batch_a_ms_p50 | network_ms_p50 | stage_total_ms_p50 |
|---|---:|---:|---:|---:|---:|---:|
| core_only | 0.14 | 0.5 | 0.06 | 5581.62 | 0.98 | 5592.36 |

## Overhead P50

| profile | non_engine_pct_p50 | bootstrap_ms_p50 |
|---|---:|---:|
| core_only | 100 | 67.64 |

## Reproduce

```powershell
& scripts/migration/run_tx_e2e_tps_core_sidecar_report.ps1 -RepoRoot D:\WEB3_AI\SUPERVM -Repeats 1 -Txs 1000 -Accounts 256 -TimeoutSec 120 -BatchCount 16 -IngressWorkers 16 -AdapterSignalMode fast -Profiles 'core_only' -AoemPluginDir D:\WEB3_AI\SUPERVM\aoem\plugins -BuildProfile release
```

## Artifacts

- d:\WEB3_AI\SUPERVM\artifacts\migration\tmp-sidecar-fast\tx-e2e-tps-summary.json
- d:\WEB3_AI\SUPERVM\docs_CN\AOEM-FFI\tmp-sidecar-fast.csv
