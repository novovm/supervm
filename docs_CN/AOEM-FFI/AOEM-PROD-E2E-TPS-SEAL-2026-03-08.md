# AOEM Production Path TPS Seal (2026-03-08)

- binary: novovm-node
- mode: ffi_v2 (production-only)
- d1_ingress_mode: ops_wire_v1
- d1_input_source: tx_wire
- d1_codec: local_tx_wire_v1_write_u64le_v1
- aoem_ingress_path: ops_wire_v1
- variant: persist
- txs: 1000
- accounts: 1000
- repeats: 1
- tx_wire_file: D:\WEB3_AI\SUPERVM\artifacts\migration\prod-node-e2e-tps-2026-03-08\prod-e2e.txwire.bin
- executable: D:\WEB3_AI\SUPERVM\target\debug\novovm-node.exe

## TPS

- host_pipeline_diag_tps p50/p90/p99: 14864.14 / 14864.14 / 14864.14
- aoem_kernel_tps p50/p90/p99: 7936507.94 / 7936507.94 / 7936507.94
- consensus_network_e2e_tps p50/p90/p99:  /  / 
- consensus_network_e2e_tps_note: not_measured_in_single_node_ffi_v2_path; use scripts/migration/run_consensus_network_e2e_tps.ps1
- wall_ms p50: 67.28

## Notes

- This script measures production novovm-node path only.
- It does not call any gate/probe/legacy binary.
- host_pipeline_diag_tps includes process startup and DLL load for each run.
- host_pipeline_diag_tps includes ingress payload read + host marshaling.
- aoem_kernel_tps is derived from AOEM elapsed_us and excludes process startup.
- For strict steady-state publish numbers, prefer long-lived process mode (single process, reused AOEM handle/session).

- summary_json: D:\WEB3_AI\SUPERVM\artifacts\migration\prod-node-e2e-tps-2026-03-08\prod-node-e2e-tps-summary.json
- raw_csv: D:\WEB3_AI\SUPERVM\docs_CN\AOEM-FFI\AOEM-PROD-E2E-TPS-RAW-2026-03-08.csv

## Reproduce

```powershell
& scripts/migration/run_prod_node_e2e_tps.ps1 -RepoRoot D:\WEB3_AI\SUPERVM -Repeats 1 -Txs 1000 -Accounts 1000 -AoemVariant persist -BuildProfile debug -D1IngressMode auto -D1Codec ''
```
