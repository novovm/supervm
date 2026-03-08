# AOEM Production Steady TPS Seal (2026-03-08)

- binary: novovm-node
- mode: ffi_v2 (production-only)
- d1_ingress_mode: ops_v2
- d1_input_source: tx_wire
- d1_codec: -
- aoem_ingress_path: ops_v2_fallback
- steady_mode: single process, in-process repeats
- variant: persist
- txs_per_repeat: 1000
- repeats: 2
- tx_wire_file: D:\WEB3_AI\SUPERVM\artifacts\migration\prod-node-steady-tps-2026-03-08\steady.txwire.bin

## TPS

- host_pipeline_diag_tps_steady p50/p90/p99: 18867924.53 / 22222222.22 / 22222222.22
- aoem_kernel_tps p50/p90/p99: 22727272.73 / 22727272.73 / 22727272.73
- host_pipeline_diag_tps_steady_aggregate: 2442002.44
- aoem_kernel_tps_aggregate: 22727272.73

## Notes

- This script uses one long-lived process and one AOEM session.
- Cold-start overhead is excluded from per-repeat steady TPS.
- summary_json: D:\WEB3_AI\SUPERVM\artifacts\migration\prod-node-steady-tps-2026-03-08\prod-node-steady-tps-summary.json
- raw_csv: D:\WEB3_AI\SUPERVM\docs_CN\AOEM-FFI\AOEM-PROD-STEADY-TPS-RAW-2026-03-08.csv
