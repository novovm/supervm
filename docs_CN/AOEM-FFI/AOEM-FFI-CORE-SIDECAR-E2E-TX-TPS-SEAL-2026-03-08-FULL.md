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
- adapter_signal_mode: full
- build_profile: release
- aoem_plugin_dir: D:\WEB3_AI\SUPERVM\aoem\plugins

## Matrix

| variant | runs | pass_runs | timeout_runs | all_pass | wall_tps_p50 | wall_tps_p90 | wall_tps_p99 |
|---|---:|---:|---:|---|---:|---:|---:|
| core | 1 | 1 | 0 | True | 577.42 | 577.42 | 577.42 |
| persist | 1 | 1 | 0 | True | 575.11 | 575.11 | 575.11 |
| wasm | 1 | 1 | 0 | True | 609.52 | 609.52 | 609.52 |

## Stage P50 (ms)

| variant | mempool_ms_p50 | adapter_ms_p50 | aoem_submit_ms_p50 | batch_a_ms_p50 | network_ms_p50 | stage_total_ms_p50 |
|---|---:|---:|---:|---:|---:|---:|
| core | 0.96 | 17241.56 | 0.36 | 36.54 | 0.41 | 17293.43 |
| persist | 1.25 | 17312.39 | 0.43 | 40.25 | 0.31 | 17369.62 |
| wasm | 0.9 | 16334.41 | 0.58 | 30.96 | 0.19 | 16381.07 |

## Reproduce

```powershell
& scripts/migration/run_tx_e2e_tps_core_sidecar_report.ps1 -RepoRoot D:\WEB3_AI\SUPERVM -Repeats 1 -Txs 10000 -TimeoutSec 300 -BatchCount 1 -AdapterSignalMode full -AoemPluginDir D:\WEB3_AI\SUPERVM\aoem\plugins -BuildProfile release
```

## Artifacts

- d:\WEB3_AI\SUPERVM\artifacts\migration\tx-e2e-tps-core-sidecar-2026-03-08-full\tx-e2e-tps-summary.json
- d:\WEB3_AI\SUPERVM\docs_CN\AOEM-FFI\AOEM-FFI-CORE-SIDECAR-E2E-TX-TPS-RAW-2026-03-08-FULL.csv
