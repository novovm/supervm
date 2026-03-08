# NOVOVM Consensus Network E2E TPS Seal (2026-03-08)

- path: consensus + network + aoem (single-process multi-node simulation)
- variant: persist
- d1_ingress_mode: ops_wire_v1
- d1_input_source: tx_wire
- d1_codec: local_tx_wire_v1_write_u64le_v1
- aoem_ingress_path: ops_wire_v1
- txs_total: 2000
- validators: 4
- batches: 2
- batch_size: 1000
- wall_ms: 113.09

## TPS / Latency

- consensus_network_e2e_tps p50/p90/p99: 61762.71 / 62039.36 / 62039.36
- consensus_network_e2e_latency_ms p50/p90/p99: 16.12 / 16.19 / 16.19
- aoem_kernel_tps p50/p90/p99: 6802721.09 / 8333333.33 / 8333333.33
- network_message_count: 12
- network_message_bytes: 1680

## Artifacts

- summary_json: D:\WEB3_AI\SUPERVM\artifacts\migration\consensus-network-e2e-tps-2026-03-08\consensus-network-e2e-summary.json
- raw_csv: D:\WEB3_AI\SUPERVM\docs_CN\CONSENSUS\NOVOVM-CONSENSUS-NETWORK-E2E-TPS-RAW-2026-03-08.csv
- stdout: D:\WEB3_AI\SUPERVM\artifacts\migration\consensus-network-e2e-tps-2026-03-08\consensus-network-e2e.stdout.log
- stderr: D:\WEB3_AI\SUPERVM\artifacts\migration\consensus-network-e2e-tps-2026-03-08\consensus-network-e2e.stderr.log
