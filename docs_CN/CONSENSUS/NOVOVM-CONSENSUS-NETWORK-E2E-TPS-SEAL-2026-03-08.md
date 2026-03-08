# NOVOVM Consensus Network E2E TPS Seal (2026-03-08)

- path: consensus + network + aoem (single-process multi-node simulation)
- variant: persist
- d1_ingress_mode: ops_wire_v1
- d1_input_source: tx_wire
- d1_codec: local_tx_wire_v1_write_u64le_v1
- aoem_ingress_path: ops_wire_v1
- network_transport: inmemory
- txs_total: 1000000
- validators: 4
- batches: 1000
- batch_size: 1000
- process_exit_code: 0
- wall_ms: 289.3

## TPS / Latency

- consensus_network_e2e_tps p50/p90/p99: 4668534.08 / 4766444.23 / 4837929.37
- consensus_network_e2e_latency_ms p50/p90/p99: 0.21 / 0.24 / 0.45
- aoem_kernel_tps p50/p90/p99: 13888888.89 / 14492753.62 / 14705882.35
- network_message_count: 6000
- network_message_bytes: 840000
- runtime_total_ms: 257.77
- tx_wire_load_ms: 20.3
- setup_ms: 10.85
- loop_total_ms: 226.6

## Wall Breakdown (inside consensus-network-e2e process)

- stage_batch_admission_ms: 0.11
- stage_ingress_pack_ms: 11.49
- stage_aoem_submit_ms: 80.12
- stage_proposal_build_ms: 16.65
- stage_proposal_broadcast_ms: 0.76
- stage_state_sync_ms: 0.52
- stage_follower_vote_ms: 45.29
- stage_qc_collect_ms: 1.16
- stage_commit_resync_ms: 69.95
- stage_other_ms: 0.56
- qc_poll_iters_total: 2999

## Artifacts

- summary_json: D:\WEB3_AI\SUPERVM\artifacts\migration\consensus-network-e2e-tps-inmem-2026-03-08-220824\consensus-network-e2e-summary.json
- raw_csv: D:\WEB3_AI\SUPERVM\docs_CN\CONSENSUS\NOVOVM-CONSENSUS-NETWORK-E2E-TPS-RAW-2026-03-08.csv
- stdout: D:\WEB3_AI\SUPERVM\artifacts\migration\consensus-network-e2e-tps-inmem-2026-03-08-220824\consensus-network-e2e.stdout.log
- stderr: D:\WEB3_AI\SUPERVM\artifacts\migration\consensus-network-e2e-tps-inmem-2026-03-08-220824\consensus-network-e2e.stderr.log
