# AOEM Core + Sidecar TX E2E TPS Seal (2026-03-08)

## Scope

- Real tx path via `novovm-node` (`tx_codec -> mempool -> batch_a -> commit -> network`).
- TPS metric is wall-clock: `processed_tx / wall_time`.
- Quantiles: P50 / P90 / P99 (nearest-rank).

## Fixed Params

- txs per run: 10000
- repeats: 1
- timeout_sec: 300
- batch_count: 1
- adapter_signal_mode: fast
- build_profile: release
- aoem_plugin_dir: D:\WEB3_AI\SUPERVM\aoem\plugins

## Matrix

| variant | runs | pass_runs | timeout_runs | all_pass | wall_tps_p50 | wall_tps_p90 | wall_tps_p99 |
|---|---:|---:|---:|---|---:|---:|---:|
| core | 1 | 1 | 0 | True | 124254.16 | 124254.16 | 124254.16 |
| persist | 1 | 1 | 0 | True | 137806.15 | 137806.15 | 137806.15 |
| wasm | 1 | 1 | 0 | True | 143980.4 | 143980.4 | 143980.4 |

## Stage P50 (ms)

| variant | mempool_ms_p50 | adapter_ms_p50 | aoem_submit_ms_p50 | batch_a_ms_p50 | network_ms_p50 | stage_total_ms_p50 |
|---|---:|---:|---:|---:|---:|---:|
| core | 1.43 | 3.65 | 0.38 | 36.14 | 0.2 | 54.86 |
| persist | 0.93 | 3.56 | 0.38 | 34.58 | 0.29 | 54.17 |
| wasm | 0.99 | 3.64 | 0.36 | 33.39 | 0.21 | 52.48 |

## Reproduce

```powershell
& scripts/migration/run_tx_e2e_tps_core_sidecar_report.ps1 -RepoRoot D:\WEB3_AI\SUPERVM -Repeats 1 -Txs 10000 -TimeoutSec 300 -BatchCount 1 -AdapterSignalMode fast -AoemPluginDir D:\WEB3_AI\SUPERVM\aoem\plugins -BuildProfile release
```

## Artifacts

- d:\WEB3_AI\SUPERVM\artifacts\migration\tx-e2e-tps-core-sidecar-2026-03-08-fast\tx-e2e-tps-summary.json
- d:\WEB3_AI\SUPERVM\docs_CN\AOEM-FFI\AOEM-FFI-CORE-SIDECAR-E2E-TX-TPS-RAW-2026-03-08-FAST.csv
