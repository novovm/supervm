# AOEM FFI Core + Sidecar TPS Seal (2026-03-08)

## Goal

- Use unified `AOEM core + optional sidecar` route (no variant DLL mode).
- Keep `ffi_perf_worldline` measurement shape and export P50/P90/P99 (nearest-rank).
- Cover `core/persist/wasm` with 4 lines (single/auto x parity/batch_stress).
- Include `network + consensus` E2E probe matrix (block wire / view-sync / new-view).

## Fixed Parameters

- Example: `crates/aoem-bindings/examples/ffi_perf_worldline.rs`
- Core DLL: `SUPERVM/aoem/bin/aoem_ffi.dll`
- Sidecar dir: `SUPERVM/aoem/plugins`
- Fixed args: `txs=1000000`, `key_space=128`, `rw=0.5`, `seed=123`, `warmup_calls=5`
- Repeats: `n=3`
- E2E network args: `mode=pair_matrix`, `rounds=2`, `node_count=2`, `timeout=20s`

## Statistics

- P50/P90/P99 use nearest-rank.
- Note: when `n=3`, P90/P99 are for comparison only, not stability claims.

## Raw Artifacts

- `D:\WEB3_AI\SUPERVM\docs_CN\AOEM-FFI\AOEM-FFI-CORE-SIDECAR-TPS-RAW-2026-03-08.csv`
- `D:\WEB3_AI\SUPERVM\artifacts\migration\aoem-tps-core-sidecar-2026-03-08-baseline\aoem-core-sidecar-tps-summary.json`
- `D:\WEB3_AI\SUPERVM\artifacts\migration\aoem-tps-core-sidecar-2026-03-08-baseline\network-consensus-core\network-two-process.json` (network+consensus, variant=core)
- `D:\WEB3_AI\SUPERVM\artifacts\migration\aoem-tps-core-sidecar-2026-03-08-baseline\network-consensus-persist\network-two-process.json` (network+consensus, variant=persist)
- `D:\WEB3_AI\SUPERVM\artifacts\migration\aoem-tps-core-sidecar-2026-03-08-baseline\network-consensus-wasm\network-two-process.json` (network+consensus, variant=wasm)

## core Matrix (3-run, P50/P90/P99)

| line_name | preset | submit_ops | threads_arg | engine_workers_arg | runtime_mode | P50 ops/s | P90 ops/s | P99 ops/s | P50 plans/s | P50 calls/s | P50 avg_ops_per_plan |
|---|---|---:|---|---|---|---:|---:|---:|---:|---:|---:|
| cpu_parity_single | cpu_parity | 1 | 1 | 16 | core | 5226268.69 | 5261980.87 | 5261980.87 | 5226268.69 | 5226268.69 | 1 |
| cpu_parity_auto_parallel | cpu_parity | 1 | auto | auto | core | 11564289.94 | 11687226.45 | 11687226.45 | 11564289.94 | 11564289.94 | 1 |
| cpu_batch_stress_single | cpu_batch_stress | 1024 | 1 | 16 | core | 22739572.77 | 24221285.67 | 24221285.67 | 22216.56 | 22216.56 | 1023.54 |
| cpu_batch_stress_auto_parallel | cpu_batch_stress | 1024 | auto | auto | core | 22004959.92 | 23383006.63 | 23383006.63 | 21652.88 | 21652.88 | 1016.26 |

### core Samples (3 runs, ops/s)

- `cpu_parity_single`
  - 5019573.83
  - 5261980.87
  - 5226268.69
- `cpu_parity_auto_parallel`
  - 10424245.96
  - 11564289.94
  - 11687226.45
- `cpu_batch_stress_single`
  - 22739572.77
  - 24221285.67
  - 10268806.55
- `cpu_batch_stress_auto_parallel`
  - 21697857.34
  - 23383006.63
  - 22004959.92

## persist Matrix (3-run, P50/P90/P99)

| line_name | preset | submit_ops | threads_arg | engine_workers_arg | runtime_mode | P50 ops/s | P90 ops/s | P99 ops/s | P50 plans/s | P50 calls/s | P50 avg_ops_per_plan |
|---|---|---:|---|---|---|---:|---:|---:|---:|---:|---:|
| cpu_parity_single | cpu_parity | 1 | 1 | 16 | composed_plugin_sidecar | 5004939.88 | 5024032.46 | 5024032.46 | 5004939.88 | 5004939.88 | 1 |
| cpu_parity_auto_parallel | cpu_parity | 1 | auto | auto | composed_plugin_sidecar | 11429603.36 | 11638124.17 | 11638124.17 | 11429603.36 | 11429603.36 | 1 |
| cpu_batch_stress_single | cpu_batch_stress | 1024 | 1 | 16 | composed_plugin_sidecar | 22212152.71 | 22244070.84 | 22244070.84 | 21701.27 | 21701.27 | 1023.54 |
| cpu_batch_stress_auto_parallel | cpu_batch_stress | 1024 | auto | auto | composed_plugin_sidecar | 18590574.21 | 23416187.61 | 23416187.61 | 18293.13 | 18293.13 | 1016.26 |

### persist Samples (3 runs, ops/s)

- `cpu_parity_single`
  - 5004939.88
  - 5024032.46
  - 4813174.24
- `cpu_parity_auto_parallel`
  - 11285255.14
  - 11638124.17
  - 11429603.36
- `cpu_batch_stress_single`
  - 22244070.84
  - 22212152.71
  - 11739872.6
- `cpu_batch_stress_auto_parallel`
  - 23416187.61
  - 18590574.21
  - 16339575.6

## wasm Matrix (3-run, P50/P90/P99)

| line_name | preset | submit_ops | threads_arg | engine_workers_arg | runtime_mode | P50 ops/s | P90 ops/s | P99 ops/s | P50 plans/s | P50 calls/s | P50 avg_ops_per_plan |
|---|---|---:|---|---|---|---:|---:|---:|---:|---:|---:|
| cpu_parity_single | cpu_parity | 1 | 1 | 16 | composed_plugin_sidecar | 4834661.82 | 4929121.69 | 4929121.69 | 4834661.82 | 4834661.82 | 1 |
| cpu_parity_auto_parallel | cpu_parity | 1 | auto | auto | composed_plugin_sidecar | 11442420.03 | 11609466.82 | 11609466.82 | 11442420.03 | 11442420.03 | 1 |
| cpu_batch_stress_single | cpu_batch_stress | 1024 | 1 | 16 | composed_plugin_sidecar | 22307735.21 | 22580958.38 | 22580958.38 | 21794.66 | 21794.66 | 1023.54 |
| cpu_batch_stress_auto_parallel | cpu_batch_stress | 1024 | auto | auto | composed_plugin_sidecar | 19669783.67 | 19770386.73 | 19770386.73 | 19355.07 | 19355.07 | 1016.26 |

### wasm Samples (3 runs, ops/s)

- `cpu_parity_single`
  - 4929121.69
  - 4834661.82
  - 4722904.81
- `cpu_parity_auto_parallel`
  - 11391247.19
  - 11442420.03
  - 11609466.82
- `cpu_batch_stress_single`
  - 22307735.21
  - 22261005.84
  - 22580958.38
- `cpu_batch_stress_auto_parallel`
  - 19770386.73
  - 19669783.67
  - 18515706.87

## Network + Consensus E2E Matrix (by AOEM variant)

| variant | pass | mode | rounds | round_pass_ratio | pair_pass_ratio | block_wire_pass | view_sync_pass | new_view_pass | consensus_binding_pass | pacemaker_pass | e2e_tps_p50 | e2e_tps_p90 | e2e_tps_p99 |
|---|---|---|---:|---:|---:|---|---|---|---|---|---:|---:|---:|
| core | True | pair_matrix | 2 | 1 | 1 | True | True | True | True | True | 37.41 | 37.45 | 37.45 |
| persist | True | pair_matrix | 2 | 1 | 1 | True | True | True | True | True | 37.45 | 37.5 | 37.5 |
| wasm | True | pair_matrix | 2 | 1 | 1 | True | True | True | True | True | 37.45 | 37.5 | 37.5 |

## Reproduce

```powershell
& scripts/migration/run_aoem_tps_core_sidecar_report.ps1 -RepoRoot D:\WEB3_AI\SUPERVM -Repeats 3 -Txs 1000000 -AoemPluginDir D:\WEB3_AI\SUPERVM\aoem\plugins -IncludeNetworkConsensusMatrix:$true -NetworkProbeMode pair_matrix -NetworkRounds 2 -NetworkNodeCount 2 -NetworkTimeoutSeconds 20
```

## Conclusion

- `core` runtime mode: `core` (pure core DLL).
- `persist/wasm` runtime mode: `composed_plugin_sidecar` (core DLL + sidecar).
- `network + consensus` signals come from `run_network_two_process.ps1` (block wire + pacemaker + e2e_tps).
- All statistics are persisted in JSON/CSV for baseline comparison.
