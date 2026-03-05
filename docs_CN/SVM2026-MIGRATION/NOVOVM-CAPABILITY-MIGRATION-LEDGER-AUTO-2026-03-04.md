# NOVOVM Capability Migration Ledger (Auto Snapshot) - 2026-03-05

- generated_at_utc: 2026-03-04T22:01:25.2708550Z
- functional_report: D:\WEB3_AI\SUPERVM\artifacts\migration\acceptance-gate-unjail-full\functional\functional-consistency.json
- performance_report: D:\WEB3_AI\SUPERVM\artifacts\migration\acceptance-gate-unjail-full\performance-gate\run-1\performance-compare.json
- capability_snapshot: D:\WEB3_AI\SUPERVM\artifacts\migration\capabilities\capability-contract-core.json
- svm2026_baseline: D:\WEB3_AI\SUPERVM\artifacts\migration\baseline\svm2026-baseline-core.json

## Auto Summary

- release_snapshot_profile: full_snapshot_v1
- release_snapshot_pass: True
- release_snapshot_json: D:\WEB3_AI\SUPERVM\artifacts\migration\release-snapshot-2026-03-05\release-snapshot.json
- functional_overall_pass: True
- performance_compare_pass: True
- state_root_available: True
- state_root_pass: True
- tx_codec_signal_available: True
- tx_codec_signal_pass: True
- tx_codec_bytes: 616
- mempool_admission_signal_available: True
- mempool_admission_signal_pass: True
- mempool_admission_accepted: 8
- mempool_admission_rejected: 0
- mempool_admission_fee_floor: 1
- tx_metadata_signal_available: True
- tx_metadata_signal_pass: True
- tx_metadata_accounts: 2
- tx_metadata_min_fee: 1
- tx_metadata_max_fee: 5
- adapter_signal_available: True
- adapter_signal_pass: True
- adapter_signal_backend: native
- adapter_signal_chain: novovm
- adapter_signal_txs: 8
- adapter_signal_accounts: 10
- adapter_plugin_abi_available: True
- adapter_plugin_abi_pass: True
- adapter_plugin_abi_enabled: False
- adapter_plugin_abi_compatible: True
- adapter_plugin_abi_expected: 1
- adapter_plugin_abi_required: 0x1
- adapter_plugin_registry_available: True
- adapter_plugin_registry_pass: True
- adapter_plugin_registry_enabled: True
- adapter_plugin_registry_matched: True
- adapter_plugin_registry_strict: False
- adapter_plugin_registry_chain_allowed: True
- adapter_plugin_registry_hash_check: False
- adapter_plugin_registry_hash_match: True
- adapter_plugin_registry_abi_whitelist: True
- adapter_plugin_registry_abi_allowed: True
- adapter_plugin_registry_expected_hash_check: False
- adapter_plugin_registry_expected_sha256: 
- adapter_plugin_registry_source: D:\WEB3_AI\SUPERVM\config\novovm-adapter-plugin-registry.json
- adapter_compat_matrix_ready: True
- adapter_compat_matrix_has_evm: True
- adapter_compat_matrix_has_bnb: True
- adapter_registry_has_evm: True
- adapter_registry_has_bnb: True
- adapter_non_novovm_sample_ready: True
- adapter_consensus_binding_available: True
- adapter_consensus_binding_pass: True
- adapter_consensus_binding_class: consensus
- adapter_consensus_binding_class_code: 1
- adapter_consensus_binding_hash: 1921b512d2854050a60f2d1ccca854afa4e8fd970afcf2d10959e01967eebc78
- adapter_plugin_abi_negative_enabled: True
- adapter_plugin_abi_negative_available: True
- adapter_plugin_abi_negative_pass: True
- adapter_plugin_abi_negative_abi_fail: True
- adapter_plugin_abi_negative_abi_reason_match: True
- adapter_plugin_abi_negative_cap_fail: True
- adapter_plugin_abi_negative_cap_reason_match: True
- adapter_plugin_abi_negative_reason: 
- adapter_plugin_symbol_negative_enabled: True
- adapter_plugin_symbol_negative_available: True
- adapter_plugin_symbol_negative_pass: True
- adapter_plugin_symbol_negative_fail: True
- adapter_plugin_symbol_negative_reason_match: True
- adapter_plugin_symbol_negative_reason: 
- adapter_plugin_registry_negative_enabled: True
- adapter_plugin_registry_negative_available: True
- adapter_plugin_registry_negative_pass: True
- adapter_plugin_registry_negative_hash_fail: True
- adapter_plugin_registry_negative_hash_reason_match: True
- adapter_plugin_registry_negative_whitelist_fail: True
- adapter_plugin_registry_negative_whitelist_reason_match: True
- adapter_plugin_registry_negative_reason: 
- network_block_wire_negative_signal_enabled: False
- network_block_wire_negative_signal_available: False
- network_block_wire_negative_signal_pass: False
- network_block_wire_negative_signal_expected_fail: False
- network_block_wire_negative_signal_reason_match: False
- network_block_wire_negative_signal_tamper_mode: hash_mismatch
- network_block_wire_negative_signal_verified: 0
- network_block_wire_negative_signal_total: 0
- adapter_backend_compare_enabled: True
- adapter_backend_compare_available: True
- adapter_backend_compare_pass: True
- adapter_backend_compare_state_root_equal: True
- adapter_backend_compare_native_backend: native
- adapter_backend_compare_plugin_backend: plugin
- adapter_backend_compare_native_state_root: c4ab4c398c1ada190f7587a258510dd5a3b13999ecb7a352adbf6a56d9cfec4b
- adapter_backend_compare_plugin_state_root: c4ab4c398c1ada190f7587a258510dd5a3b13999ecb7a352adbf6a56d9cfec4b
- adapter_backend_compare_plugin_path: D:\WEB3_AI\SUPERVM\target\debug\novovm_adapter_sample_plugin.dll
- adapter_backend_compare_reason: 
- batch_a_closure_available: True
- batch_a_closure_pass: True
- batch_a_demo_txs: 8
- batch_a_target_batches: 2
- batch_a_expected_min_batches: 2
- block_wire_signal_available: True
- block_wire_signal_pass: True
- block_wire_codec: novovm_block_header_wire_v1
- block_wire_bytes: 130
- block_output_signal_available: True
- block_output_signal_pass: True
- block_output_batches: 2
- block_output_txs: 8
- commit_output_signal_available: True
- commit_output_signal_pass: True
- network_output_signal_available: True
- network_output_signal_pass: True
- network_closure_signal_available: True
- network_closure_signal_pass: True
- network_pacemaker_signal_available: True
- network_pacemaker_signal_pass: True
- network_process_signal_available: True
- network_process_signal_pass: True
- network_process_rounds: 1
- network_process_rounds_passed: 1
- network_process_round_pass_ratio: 1
- network_process_node_count: 2
- network_process_total_pairs: 1
- network_process_passed_pairs: 1
- network_process_pass_ratio: 1
- network_process_mode: mesh
- network_directed_edges_up: 2
- network_directed_edges_total: 2
- network_directed_edge_ratio: 1
- network_block_wire_available: True
- network_block_wire_pass: True
- network_block_wire_pass_ratio: 1
- network_block_wire_verified: 2
- network_block_wire_total: 2
- network_block_wire_verified_ratio: 1
- network_view_sync_available: True
- network_view_sync_pass: True
- network_view_sync_pass_ratio: 1
- network_new_view_available: True
- network_new_view_pass: True
- network_new_view_pass_ratio: 1
- coordinator_signal_enabled: True
- coordinator_signal_available: True
- coordinator_signal_pass: True
- coordinator_signal_reason: 
- coordinator_negative_signal_enabled: True
- coordinator_negative_signal_available: True
- coordinator_negative_signal_pass: True
- coordinator_negative_unknown_prepare: True
- coordinator_negative_non_participant_vote: True
- coordinator_negative_vote_after_decide: True
- coordinator_negative_duplicate_tx: True
- coordinator_negative_reason: 
- prover_contract_signal_enabled: True
- prover_contract_signal_available: True
- prover_contract_signal_pass: True
- prover_contract_signal_schema_ok: True
- prover_contract_signal_reason_norm: True
- prover_contract_signal_fallback_codes: 2
- prover_contract_signal_reason: 
- prover_contract_negative_enabled: True
- prover_contract_negative_available: True
- prover_contract_negative_pass: True
- prover_contract_negative_missing_formal_fields: True
- prover_contract_negative_empty_reason_codes: True
- prover_contract_negative_normalization_stable: True
- prover_contract_negative_reason: 
- consensus_negative_signal_enabled: True
- consensus_negative_signal_available: True
- consensus_negative_signal_pass: True
- consensus_negative_signal_invalid_signature: True
- consensus_negative_signal_duplicate_vote: True
- consensus_negative_signal_wrong_epoch: True
- consensus_negative_signal_weighted_quorum: True
- consensus_negative_signal_equivocation: True
- consensus_negative_signal_slash_execution: True
- consensus_negative_signal_slash_threshold: True
- consensus_negative_signal_slash_observe_only: True
- consensus_negative_signal_unjail_cooldown: True
- consensus_negative_signal_view_change: True
- consensus_negative_signal_fork_choice: True
- consensus_negative_signal_reason: 
- zk_ready: False
- zk_formal_fields_present: False
- prover_ready: False
- zk_contract_schema_ready: True
- cap_has_fallback_reason: True
- cap_has_fallback_reason_codes: True
- cap_has_zk_formal_flag: True
- fallback_reason: 
- fallback_reason_codes: 
- msm_ready: True
- baseline_ready: True
- consensus_skeleton_ready: True
- network_skeleton_ready: True
- adapter_skeleton_ready: True
- adapter_native_ready: True
- adapter_plugin_ready: True
- full_scan_f01_status: ReadyForMerge
- full_scan_f02_status: ReadyForMerge
- full_scan_f03_status: ReadyForMerge
- full_scan_f04_status: ReadyForMerge
- full_scan_f05_status: ReadyForMerge
- full_scan_f06_status: ReadyForMerge
- full_scan_f07_status: ReadyForMerge
- full_scan_f08_status: ReadyForMerge
- full_scan_f09_status: ReadyForMerge
- full_scan_f10_status: NotStarted
- full_scan_f11_status: NotStarted
- full_scan_f12_status: NotStarted
- full_scan_f13_status: NotStarted
- full_scan_f14_status: InProgress
- full_scan_f15_status: ReadyForMerge
- full_scan_f16_status: ReadyForMerge
- domain_d0_status: Done
- domain_d1_status: Done
- domain_d2_status: Done
- domain_d3_status: Done

## Domain Scan (D0~D3)

| Domain | Status | Done Criteria | Auto Evidence |
|---|---|---|---|
| D0 AOEM Foundation Domain | Done | F-01/F-02 = ReadyForMerge | F-01=ReadyForMerge, F-02=ReadyForMerge |
| D1 Execution Facade Domain | Done | F-01/F-02 = ReadyForMerge + functional_pass=True | F-01=ReadyForMerge, F-02=ReadyForMerge, functional_pass=True |
| D2 Protocol Core Domain | Done | F-03/F-04 = ReadyForMerge | F-03=ReadyForMerge, F-04=ReadyForMerge |
| D3 Consensus Network Domain | Done | F-05/F-06/F-07/F-08 = ReadyForMerge | F-05=ReadyForMerge, F-06=ReadyForMerge, F-07=ReadyForMerge, F-08=ReadyForMerge |

## Full Scan Matrix (F-01~F-16)

| ID | Domain | Status | Auto Evidence |
|---|---|---|---|
| F-01 | AOEM execution entry | ReadyForMerge | exec=True, bindings=True, adapter_signal.pass=True |
| F-02 | AOEM runtime config | ReadyForMerge | exec=True, variant_digest.pass=True |
| F-03 | Execution receipt standard | ReadyForMerge | protocol=True, tx_codec=True, block_wire=True, block_out=True, commit_out=True |
| F-04 | State root consistency | ReadyForMerge | state_root.available=True, state_root.pass=True |
| F-05 | Consensus engine | ReadyForMerge | consensus=True, batch_a=True, consensus_negative.enabled=True, consensus_negative.available=True, consensus_negative.pass=True, weighted_quorum=True, equivocation=True, slash_execution=True, slash_threshold=True, slash_observe_only=True, unjail_cooldown=True, view_change=True, fork_choice=True |
| F-06 | Distributed coordinator | ReadyForMerge | coordinator=True, signal_enabled=True, signal_available=True, signal_pass=True, negative_enabled=True, negative_available=True, negative_pass=True |
| F-07 | Network layer | ReadyForMerge | network=True, closure=True, pacemaker=True, process=True, block_wire=True, view_sync=True, new_view=True, block_wire_negative=False |
| F-08 | Chain adapter interface | ReadyForMerge | adapter=True, abi=True, registry=True, consensus=True, compare=True, matrix=True, non_novovm_sample=True, abi_negative_enabled=True, abi_negative_pass=True, symbol_negative_enabled=True, symbol_negative_pass=True, registry_negative_enabled=True, registry_negative_pass=True |
| F-09 | zk execution/aggregation | ReadyForMerge | prover=True, prover_signal=True, prover_negative_enabled=True, prover_negative_available=True, prover_negative_pass=True, schema_ok=True, reason_norm=True, zk_runtime_ready=False |
| F-10 | Web3 storage service | NotStarted | storage_service=False |
| F-11 | Domain system | NotStarted | app_domain=False |
| F-12 | DeFi core | NotStarted | app_defi=False |
| F-13 | Multi-chain plugin capability | NotStarted | adapters_multi=False |
| F-14 | vm-runtime split migration | InProgress | protocol=True, consensus=True, network=True, adapter=True, legacy_vm_runtime_present=False |
| F-15 | AOEM ZK capability contract | ReadyForMerge | zkvm_prove=False, zkvm_verify=False, zk_formal_fields_present=False, schema_ready=True, fallback_reason= |
| F-16 | AOEM MSM acceleration contract | ReadyForMerge | msm_accel=True, msm_backend= |

## Ledger

| ID | Capability | Status | Auto Progress | Evidence Path | Updated |
|---|---|---|---|---|---|
| F-05 | Consensus engine (~80% verified) | ReadyForMerge | novovm-consensus skeleton + tx_codec_signal(pass=True, bytes=616) + mempool_admission_signal(pass=True, accepted=8, rejected=0, fee_floor=1) + tx_metadata_signal(pass=True, accounts=2, fee=1-5) + batch_a_closure(pass=True, txs=8, target_batches=2, expected_min_batches=2) + block_wire_signal(pass=True, codec=novovm_block_header_wire_v1, bytes=130) + block_output_signal(pass=True, batches=2, txs=8) + commit_output_signal(pass=True) + consensus_negative_signal(enabled=True, available=True, pass=True, invalid_signature=True, duplicate_vote=True, wrong_epoch=True, weighted_quorum=True, equivocation=True, slash_execution=True, slash_threshold=True, slash_observe_only=True, unjail_cooldown=True, view_change=True, fork_choice=True, reason=) are available | D:\WEB3_AI\SUPERVM\artifacts\migration\acceptance-gate-unjail-full\functional\functional-consistency.json | 2026-03-05 |
| F-06 | Distributed coordinator | ReadyForMerge | novovm-coordinator skeleton + coordinator_signal(enabled=True, available=True, pass=True, reason=) + coordinator_negative_signal(enabled=True, available=True, pass=True, unknown_prepare=True, non_participant_vote=True, vote_after_decide=True, duplicate_tx=True, reason=) | D:\WEB3_AI\SUPERVM\artifacts\migration\acceptance-gate-unjail-full\functional\functional-consistency.json | 2026-03-05 |
| F-07 | Network layer (core-complete, production hardening pending) | ReadyForMerge | novovm-network skeleton + network_output_signal(pass=True) + network_closure_signal(pass=True) + network_pacemaker_signal(pass=True) + network_process_signal(pass=True, mode=mesh, rounds=1/1, round_ratio=1, nodes=2, pairs=1/1, ratio=1, directed=2/2:1, block_wire=True(2/2:1), block_wire_round_ratio=1, view_sync=True(1), new_view=True(1)) + network_block_wire_negative(enabled=false) are available | D:\WEB3_AI\SUPERVM\artifacts\migration\acceptance-gate-unjail-full\functional\functional-consistency.json | 2026-03-05 |
| F-08 | Chain adapter API interface | ReadyForMerge | novovm-adapter-api + native/plugin backends + adapter_signal(pass=True, backend=native, chain=novovm, txs=8, accounts=10) + plugin_abi(pass=True, enabled=False, compatible=True, expected=1, required=0x1) + plugin_registry(pass=True, enabled=True, matched=True, strict=False, chain_allowed=True, hash_check=False/True, abi_whitelist=True/True) + consensus_binding(pass=True, available=True, class=consensus/1) + compat_matrix(ready=True, evm=True, bnb=True, registry_evm=True, registry_bnb=True, non_novovm_sample=True) + plugin_abi_negative(pass=True, available=True, abi_fail=True/True, cap_fail=True/True) + plugin_symbol_negative(pass=True, available=True, fail=True/True) + plugin_registry_negative(pass=True, available=True, hash_fail=True/True, whitelist_fail=True/True) + compare(enabled=True, available=True, pass=True, state_root_equal=True, native_backend=native, plugin_backend=plugin) are available | D:\WEB3_AI\SUPERVM\artifacts\migration\acceptance-gate-unjail-full\functional\functional-consistency.json | 2026-03-05 |
| F-09 | zk execution/aggregation | ReadyForMerge | novovm-prover skeleton + prover_contract_signal(enabled=True, available=True, pass=True, schema_ok=True, reason_norm=True, fallback_codes=2, reason=) + prover_contract_negative_signal(enabled=True, available=True, pass=True, missing_formal_fields=True, empty_reason_codes=True, normalization_stable=True, reason=) + zk_runtime_ready=False | D:\WEB3_AI\SUPERVM\artifacts\migration\acceptance-gate-unjail-full\functional\functional-consistency.json | 2026-03-05 |
| F-15 | AOEM ZK capability contract | ReadyForMerge | zkvm_prove=False / zkvm_verify=False / zk_formal_fields_present=False / schema_ready=True / fallback_reason= | D:\WEB3_AI\SUPERVM\artifacts\migration\capabilities\capability-contract-core.json | 2026-03-05 |
| F-16 | AOEM MSM acceleration contract | ReadyForMerge | msm_accel=True / msm_backend= | D:\WEB3_AI\SUPERVM\artifacts\migration\capabilities\capability-contract-core.json | 2026-03-05 |

## Notes

- This file is auto-generated and does not replace the manual ledger.
- state_root consistency uses hard parity when state_root_available=true; otherwise it falls back to proxy digest.
- When baseline_ready=true and performance_compare_pass has a value, it can be used for regression threshold checks.
