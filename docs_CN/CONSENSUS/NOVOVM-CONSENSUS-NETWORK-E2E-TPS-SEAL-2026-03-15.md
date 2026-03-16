# NOVOVM Consensus Network E2E TPS Seal (2026-03-15)

- path: consensus + network + aoem (single-process multi-node simulation)
- variant: persist
- d1_ingress_mode: ops_wire_v1
- d1_input_source: tx_wire
- d1_codec: local_tx_wire_v1_write_u64le_v1
- aoem_ingress_path: ops_wire_v1
- network_transport: inmemory
- txs_total: 20000
- validators: 4
- batches: 40
- batch_size: 500
- process_exit_code: 0
- wall_ms: 96.5

## TPS / Latency

- consensus_network_e2e_tps p50/p90/p99: 2649708.53 / 2875215.64 / 2922267.68
- consensus_network_e2e_latency_ms p50/p90/p99: 0.19 / 0.22 / 0.32
- aoem_kernel_tps p50/p90/p99: 8928571.43 / 9803921.57 / 11363636.36
- network_message_count: 240
- network_message_bytes: 34560
- runtime_total_ms: 29.57
- tx_wire_load_ms: 0.81
- setup_ms: 20.94
- loop_total_ms: 7.81

## Wall Breakdown (inside consensus-network-e2e process)

- stage_batch_admission_ms: 0.01
- stage_ingress_pack_ms: 0.36
- stage_aoem_submit_ms: 2.42
- stage_proposal_build_ms: 0.39
- stage_proposal_broadcast_ms: 0.04
- stage_state_sync_ms: 0.03
- stage_follower_vote_ms: 2.18
- stage_qc_collect_ms: 0.06
- stage_commit_resync_ms: 2.29
- stage_other_ms: 0.02
- qc_poll_iters_total: 119

## Artifacts

- summary_json: D:\WEB3_AI\SUPERVM\artifacts\migration\release-candidate-testnet-bootstrap-smoke-pass2\snapshot\acceptance-gate-full\testnet-bootstrap-gate\consensus-network-e2e\consensus-network-e2e-summary.json
- raw_csv: D:\WEB3_AI\SUPERVM\docs_CN\CONSENSUS\NOVOVM-CONSENSUS-NETWORK-E2E-TPS-RAW-2026-03-15.csv
- stdout: D:\WEB3_AI\SUPERVM\artifacts\migration\release-candidate-testnet-bootstrap-smoke-pass2\snapshot\acceptance-gate-full\testnet-bootstrap-gate\consensus-network-e2e\consensus-network-e2e.stdout.log
- stderr: D:\WEB3_AI\SUPERVM\artifacts\migration\release-candidate-testnet-bootstrap-smoke-pass2\snapshot\acceptance-gate-full\testnet-bootstrap-gate\consensus-network-e2e\consensus-network-e2e.stderr.log
