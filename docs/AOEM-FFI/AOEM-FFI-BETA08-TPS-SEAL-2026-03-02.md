# AOEM FFI beta0.8 TPS Seal (CPU core+persist+wasm 3 matrices, 2026-03-02)

## Seal objectives for this round

- Keep only the FFI V2 typed binary path (`aoem_execute_ops_v2`).
- Unify naming and measured runs for core + persist + wasm (12 lines total).
- Use joint adaptive selection for `threads=auto` (`threads` + `engine_workers`).
- Remove old matrices and legacy metric definitions.

## Fixed methodology (must not be mixed)

- Example program: `crates/aoem-bindings/examples/ffi_perf_worldline.rs`
- core DLL: `SUPERVM/aoem/bin/aoem_ffi.dll`
- persist DLL: `SUPERVM/aoem/variants/persist/bin/aoem_ffi.dll`
- Fixed params:
  - `txs=1,000,000`
  - `key_space=128`
  - `rw=0.5`
  - `seed=123`
  - `warmup_calls=5`

## Four line names

- `cpu_parity_single`: `preset=cpu_parity`, `submit_ops=1`, `threads=1`, `engine_workers=16`
- `cpu_parity_auto_parallel`: `preset=cpu_parity`, `submit_ops=1`, `threads=auto`, `engine_workers=auto`
- `cpu_batch_stress_single`: `preset=cpu_batch_stress`, `submit_ops=1024`, `threads=1`, `engine_workers=16`
- `cpu_batch_stress_auto_parallel`: `preset=cpu_batch_stress`, `submit_ops=1024`, `threads=auto`, `engine_workers=auto`

## Unit definitions

- `ops/s`: transaction throughput (primary KPI)
- `plans/s`: submit throughput (1 plan per submit)
- `calls/s`: FFI call throughput

## Raw data

- `docs/AOEM-FFI/AOEM-FFI-BETA08-V2-RESEAL-RAW-2026-03-02.csv`

## Core measured matrix (3 runs, P50/P90/P99)

| line_name | preset | submit_ops | threads_arg | engine_workers_arg | selected_threads | selected_engine_workers | total_workers | P50 ops/s | P90 ops/s | P99 ops/s | P50 plans/s | P50 calls/s | avg_ops_per_plan |
|---|---|---:|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| cpu_parity_single | cpu_parity | 1 | 1 | 16 | 1 | 16 | 16 | 5,003,439.86 | 5,044,650.20 | 5,044,650.20 | 5,003,439.86 | 5,003,439.86 | 1.00 |
| cpu_parity_auto_parallel | cpu_parity | 1 | auto | auto | 11 | 2 | 22 | 6,825,803.45 | 7,251,195.18 | 7,251,195.18 | 6,825,803.45 | 6,825,803.45 | 1.00 |
| cpu_batch_stress_single | cpu_batch_stress | 1024 | 1 | 16 | 1 | 16 | 16 | 20,900,563.48 | 21,796,179.57 | 21,796,179.57 | 20,419.85 | 20,419.85 | 1,023.54 |
| cpu_batch_stress_auto_parallel | cpu_batch_stress | 1024 | auto | auto | 16 | 1 | 16 | 18,668,976.64 | 18,713,205.16 | 18,713,205.16 | 18,519.62 | 18,519.62 | 1,008.06 |

Note: with current `n=3` samples, `P90/P99` are nearest-rank stats for seal comparison only, not stability conclusions.

## Core per-line 3-run measurements (ops/s)

- `cpu_parity_single`
  - 4,885,267.06
  - 5,003,439.86
  - 5,044,650.20
- `cpu_parity_auto_parallel`
  - 6,825,803.45
  - 7,251,195.18
  - 6,758,624.51
- `cpu_batch_stress_single`
  - 21,796,179.57
  - 7,756,333.24
  - 20,900,563.48
- `cpu_batch_stress_auto_parallel`
  - 18,668,976.64
  - 18,713,205.16
  - 18,100,595.87

## Persist measured matrix (3 runs, P50/P90/P99)

| line_name | preset | submit_ops | threads_arg | engine_workers_arg | selected_threads | selected_engine_workers | total_workers | P50 ops/s | P90 ops/s | P99 ops/s | P50 plans/s | P50 calls/s | avg_ops_per_plan |
|---|---|---:|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| cpu_parity_single | cpu_parity | 1 | 1 | 16 | 1 | 16 | 16 | 3,979,344.02 | 3,986,856.13 | 3,986,856.13 | 3,979,344.02 | 3,979,344.02 | 1.00 |
| cpu_parity_auto_parallel | cpu_parity | 1 | auto | auto | 11 | 2 | 22 | 6,567,471.58 | 10,340,705.57 | 10,340,705.57 | 6,567,471.58 | 6,567,471.58 | 1.00 |
| cpu_batch_stress_single | cpu_batch_stress | 1024 | 1 | 16 | 1 | 16 | 16 | 8,726,049.18 | 14,830,414.21 | 14,830,414.21 | 8,525.35 | 8,525.35 | 1,023.54 |
| cpu_batch_stress_auto_parallel | cpu_batch_stress | 1024 | auto | auto | 16 | 1 | 16 | 12,594,331.54 | 12,856,527.58 | 12,856,527.58 | 12,493.58 | 12,493.58 | 1,008.06 |

## Persist per-line 3-run measurements (ops/s)

- `cpu_parity_single`
  - 3,842,695.42
  - 3,979,344.02
  - 3,986,856.13
- `cpu_parity_auto_parallel`
  - 6,567,471.58
  - 6,283,759.50
  - 10,340,705.57
- `cpu_batch_stress_single`
  - 6,300,470.71
  - 14,830,414.21
  - 8,726,049.18
- `cpu_batch_stress_auto_parallel`
  - 12,594,331.54
  - 12,856,527.58
  - 11,923,612.57

## Wasm measured matrix (3 runs, P50/P90/P99)

| line_name | preset | submit_ops | threads_arg | engine_workers_arg | selected_threads | selected_engine_workers | total_workers | P50 ops/s | P90 ops/s | P99 ops/s | P50 plans/s | P50 calls/s | avg_ops_per_plan |
|---|---|---:|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| cpu_parity_single | cpu_parity | 1 | 1 | 16 | 1 | 16 | 16 | 4,924,546.10 | 5,111,282.85 | 5,111,282.85 | 4,924,546.10 | 4,924,546.10 | 1.00 |
| cpu_parity_auto_parallel | cpu_parity | 1 | auto | auto | 11 | 2 | 22 | 12,964,686.79 | 13,079,520.87 | 13,079,520.87 | 12,964,686.79 | 12,964,686.79 | 1.00 |
| cpu_batch_stress_single | cpu_batch_stress | 1024 | 1 | 16 | 1 | 16 | 16 | 21,870,161.23 | 21,980,485.72 | 21,980,485.72 | 21,367.15 | 21,367.15 | 1,023.54 |
| cpu_batch_stress_auto_parallel | cpu_batch_stress | 1024 | auto | auto | 16 | 1 | 16 | 19,109,205.29 | 19,893,015.36 | 19,893,015.36 | 18,956.33 | 18,956.33 | 1,008.06 |

## Wasm per-line 3-run measurements (ops/s)

- `cpu_parity_single`
  - 4,924,546.10
  - 4,300,461.18
  - 5,111,282.85
- `cpu_parity_auto_parallel`
  - 7,146,266.93
  - 12,964,686.79
  - 13,079,520.87
- `cpu_batch_stress_single`
  - 21,870,161.23
  - 17,896,387.08
  - 21,980,485.72
- `cpu_batch_stress_auto_parallel`
  - 19,109,205.29
  - 18,463,367.76
  - 19,893,015.36

## Key conclusions

1. All 12 lines (core + persist + wasm) were measured and sealed into one raw CSV.
2. `threads=auto` now uses a joint guard (`threads * engine_workers <= budget_threads`) instead of only expanding `threads`.
3. Persist path is confirmed on binary V2 (`execute_ops_v2=true`; `json_input_enabled=false`; `json_response_enabled=false`).
4. Wasm path is fixed on binary V2 (`execute_ops_v2=true`; `json_input_enabled=false`; `json_response_enabled=false`) and included in the same matrix.
5. Under current machine/sample set, persist `cpu_batch_stress_auto_parallel` is more stable than `cpu_batch_stress_single`.

## Reproduction commands

```powershell
cd D:\WorksArea\SUPERVM\crates\aoem-bindings

# cpu_parity_single
.\target\release\examples\ffi_perf_worldline.exe --dll D:\WorksArea\SUPERVM\aoem\bin\aoem_ffi.dll --preset cpu_parity --submit-ops 1 --threads 1 --engine-workers 16 --txs 1000000 --key-space 128 --rw 0.5 --seed 123 --warmup-calls 5

# cpu_parity_auto_parallel
.\target\release\examples\ffi_perf_worldline.exe --dll D:\WorksArea\SUPERVM\aoem\bin\aoem_ffi.dll --preset cpu_parity --submit-ops 1 --threads auto --engine-workers auto --txs 1000000 --key-space 128 --rw 0.5 --seed 123 --warmup-calls 5

# cpu_batch_stress_single
.\target\release\examples\ffi_perf_worldline.exe --dll D:\WorksArea\SUPERVM\aoem\bin\aoem_ffi.dll --preset cpu_batch_stress --submit-ops 1024 --threads 1 --engine-workers 16 --txs 1000000 --key-space 128 --rw 0.5 --seed 123 --warmup-calls 5

# cpu_batch_stress_auto_parallel
.\target\release\examples\ffi_perf_worldline.exe --dll D:\WorksArea\SUPERVM\aoem\bin\aoem_ffi.dll --preset cpu_batch_stress --submit-ops 1024 --threads auto --engine-workers auto --txs 1000000 --key-space 128 --rw 0.5 --seed 123 --warmup-calls 5

# persist: set path before running
$env:AOEM_PERSISTENCE_PATH='D:\WorksArea\SUPERVM\aoem\data\rocksdb-matrix-persist-20260302\manual-run'

# persist cpu_parity_single
.\target\release\examples\ffi_perf_worldline.exe --dll D:\WorksArea\SUPERVM\aoem\variants\persist\bin\aoem_ffi.dll --preset cpu_parity --submit-ops 1 --threads 1 --engine-workers 16 --txs 1000000 --key-space 128 --rw 0.5 --seed 123 --warmup-calls 5

# persist cpu_parity_auto_parallel
.\target\release\examples\ffi_perf_worldline.exe --dll D:\WorksArea\SUPERVM\aoem\variants\persist\bin\aoem_ffi.dll --preset cpu_parity --submit-ops 1 --threads auto --engine-workers auto --txs 1000000 --key-space 128 --rw 0.5 --seed 123 --warmup-calls 5

# persist cpu_batch_stress_single
.\target\release\examples\ffi_perf_worldline.exe --dll D:\WorksArea\SUPERVM\aoem\variants\persist\bin\aoem_ffi.dll --preset cpu_batch_stress --submit-ops 1024 --threads 1 --engine-workers 16 --txs 1000000 --key-space 128 --rw 0.5 --seed 123 --warmup-calls 5

# persist cpu_batch_stress_auto_parallel
.\target\release\examples\ffi_perf_worldline.exe --dll D:\WorksArea\SUPERVM\aoem\variants\persist\bin\aoem_ffi.dll --preset cpu_batch_stress --submit-ops 1024 --threads auto --engine-workers auto --txs 1000000 --key-space 128 --rw 0.5 --seed 123 --warmup-calls 5

# wasm cpu_parity_single
.\target\release\examples\ffi_perf_worldline.exe --dll D:\WorksArea\SUPERVM\aoem\variants\wasm\bin\aoem_ffi.dll --preset cpu_parity --submit-ops 1 --threads 1 --engine-workers 16 --txs 1000000 --key-space 128 --rw 0.5 --seed 123 --warmup-calls 5

# wasm cpu_parity_auto_parallel
.\target\release\examples\ffi_perf_worldline.exe --dll D:\WorksArea\SUPERVM\aoem\variants\wasm\bin\aoem_ffi.dll --preset cpu_parity --submit-ops 1 --threads auto --engine-workers auto --txs 1000000 --key-space 128 --rw 0.5 --seed 123 --warmup-calls 5

# wasm cpu_batch_stress_single
.\target\release\examples\ffi_perf_worldline.exe --dll D:\WorksArea\SUPERVM\aoem\variants\wasm\bin\aoem_ffi.dll --preset cpu_batch_stress --submit-ops 1024 --threads 1 --engine-workers 16 --txs 1000000 --key-space 128 --rw 0.5 --seed 123 --warmup-calls 5

# wasm cpu_batch_stress_auto_parallel
.\target\release\examples\ffi_perf_worldline.exe --dll D:\WorksArea\SUPERVM\aoem\variants\wasm\bin\aoem_ffi.dll --preset cpu_batch_stress --submit-ops 1024 --threads auto --engine-workers auto --txs 1000000 --key-space 128 --rw 0.5 --seed 123 --warmup-calls 5
```

