# AOEM FFI E2E TPS Mode Comparison (2026-03-08)

## Scope

- Compare `novovm-node` E2E wall-clock TPS under two adapter signal modes.
- Same workload: `Txs=10000`, `Repeats=1`, `BatchCount=1`, `BuildProfile=release`.
- Metrics source:
  - Fast mode: `artifacts/migration/tx-e2e-tps-core-sidecar-2026-03-08-fast/tx-e2e-tps-summary.json`
  - Full mode: `artifacts/migration/tx-e2e-tps-core-sidecar-2026-03-08-full/tx-e2e-tps-summary.json`

## Result Snapshot

| mode | variant | wall_tps_p50 | adapter_ms_p50 | stage_total_ms_p50 |
|---|---|---:|---:|---:|
| fast (`NOVOVM_ADAPTER_SIGNAL_FAST=1`) | core | 124254.16 | 3.65 | 54.86 |
| fast (`NOVOVM_ADAPTER_SIGNAL_FAST=1`) | persist | 137806.15 | 3.56 | 54.17 |
| fast (`NOVOVM_ADAPTER_SIGNAL_FAST=1`) | wasm | 143980.40 | 3.64 | 52.48 |
| full (`NOVOVM_ADAPTER_SIGNAL_FAST=0`) | core | 577.42 | 17241.56 | 17293.43 |
| full (`NOVOVM_ADAPTER_SIGNAL_FAST=0`) | persist | 575.11 | 17312.39 | 17369.62 |
| full (`NOVOVM_ADAPTER_SIGNAL_FAST=0`) | wasm | 609.52 | 16334.41 | 16381.07 |

## Interpretation

- Throughput collapse is dominated by `adapter_ms` in **full** mode (16-17 seconds at 10k tx).
- AOEM execution (`aoem_submit_ms`) remains sub-millisecond in both modes.
- This indicates a **measurement-path bottleneck** (full adapter bridge validation path), not AOEM kernel collapse.

## Policy

- `run_ffi_v2` now defaults adapter signal to fast mode (`NOVOVM_ADAPTER_SIGNAL_FAST` default = true).
- Use full mode only for migration/compatibility deep checks, not for publish TPS baseline.

## Reproduce

```powershell
# fast (publish baseline)
& scripts/migration/run_tx_e2e_tps_core_sidecar_report.ps1 `
  -RepoRoot D:\WEB3_AI\SUPERVM `
  -Txs 10000 -Repeats 1 -BatchCount 1 -AdapterSignalMode fast `
  -OutputDir D:\WEB3_AI\SUPERVM\artifacts\migration\tx-e2e-tps-core-sidecar-2026-03-08-fast `
  -DocOutputPath D:\WEB3_AI\SUPERVM\docs_CN\AOEM-FFI\AOEM-FFI-CORE-SIDECAR-E2E-TX-TPS-SEAL-2026-03-08-FAST.md `
  -RawCsvOutputPath D:\WEB3_AI\SUPERVM\docs_CN\AOEM-FFI\AOEM-FFI-CORE-SIDECAR-E2E-TX-TPS-RAW-2026-03-08-FAST.csv

# full (deep compatibility check)
& scripts/migration/run_tx_e2e_tps_core_sidecar_report.ps1 `
  -RepoRoot D:\WEB3_AI\SUPERVM `
  -Txs 10000 -Repeats 1 -BatchCount 1 -AdapterSignalMode full `
  -OutputDir D:\WEB3_AI\SUPERVM\artifacts\migration\tx-e2e-tps-core-sidecar-2026-03-08-full `
  -DocOutputPath D:\WEB3_AI\SUPERVM\docs_CN\AOEM-FFI\AOEM-FFI-CORE-SIDECAR-E2E-TX-TPS-SEAL-2026-03-08-FULL.md `
  -RawCsvOutputPath D:\WEB3_AI\SUPERVM\docs_CN\AOEM-FFI\AOEM-FFI-CORE-SIDECAR-E2E-TX-TPS-RAW-2026-03-08-FULL.csv
```
