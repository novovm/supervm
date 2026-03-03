# NOVOVM Capability Migration Ledger (Auto Snapshot) - 2026-03-03

- generated_at_utc: 2026-03-03T15:09:32.7726039Z
- functional_report: D:\WorksArea\SUPERVM\artifacts\migration\functional-smoke51-network-wire-negative\functional-consistency.json
- performance_report: D:\WorksArea\SUPERVM\artifacts\migration\performance\performance-compare.json
- capability_snapshot: D:\WorksArea\SUPERVM\artifacts\migration\capabilities\capability-contract-core.json
- svm2026_baseline: D:\WorksArea\SUPERVM\artifacts\migration\baseline\svm2026-baseline-core.json

## Auto Summary

- functional_overall_pass: True
- performance_compare_pass: 
- state_root_available: False
- state_root_proxy_pass: True
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
- adapter_signal_backend: plugin
- adapter_signal_chain: novovm
- adapter_signal_txs: 8
- adapter_signal_accounts: 10
- adapter_plugin_abi_available: True
- adapter_plugin_abi_pass: True
- adapter_plugin_abi_enabled: True
- adapter_plugin_abi_compatible: True
- adapter_plugin_abi_expected: 1
- adapter_plugin_abi_required: 0x1
- adapter_plugin_registry_available: True
- adapter_plugin_registry_pass: True
- adapter_plugin_registry_enabled: True
- adapter_plugin_registry_matched: True
- adapter_plugin_registry_strict: True
- adapter_plugin_registry_chain_allowed: True
- adapter_plugin_registry_hash_check: True
- adapter_plugin_registry_hash_match: True
- adapter_plugin_registry_abi_whitelist: True
- adapter_plugin_registry_abi_allowed: True
- adapter_plugin_registry_expected_hash_check: True
- adapter_plugin_registry_expected_sha256: a22ee3697eab920a68492af8c08d4646ee73b9fa605d8126aed3780363b60d1e
- adapter_plugin_registry_source: D:\WorksArea\SUPERVM\config\novovm-adapter-plugin-registry.json
- adapter_consensus_binding_available: True
- adapter_consensus_binding_pass: True
- adapter_consensus_binding_class: consensus
- adapter_consensus_binding_class_code: 1
- adapter_consensus_binding_hash: 0d94d856656f83c52a8d85bca9962df9f5927b37708485c597000a89d6ef53b4
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
- network_block_wire_negative_signal_enabled: True
- network_block_wire_negative_signal_available: True
- network_block_wire_negative_signal_pass: True
- network_block_wire_negative_signal_expected_fail: True
- network_block_wire_negative_signal_reason_match: True
- network_block_wire_negative_signal_tamper_mode: hash_mismatch
- network_block_wire_negative_signal_verified: 0
- network_block_wire_negative_signal_total: 2
- adapter_backend_compare_enabled: True
- adapter_backend_compare_available: True
- adapter_backend_compare_pass: True
- adapter_backend_compare_state_root_equal: True
- adapter_backend_compare_native_backend: native
- adapter_backend_compare_plugin_backend: plugin
- adapter_backend_compare_native_state_root: c4ab4c398c1ada190f7587a258510dd5a3b13999ecb7a352adbf6a56d9cfec4b
- adapter_backend_compare_plugin_state_root: c4ab4c398c1ada190f7587a258510dd5a3b13999ecb7a352adbf6a56d9cfec4b
- adapter_backend_compare_plugin_path: D:\WorksArea\SUPERVM\target\debug\novovm_adapter_sample_plugin.dll
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
- network_process_signal_available: True
- network_process_signal_pass: True
- network_process_rounds: 2
- network_process_rounds_passed: 2
- network_process_round_pass_ratio: 1
- network_process_node_count: 3
- network_process_total_pairs: 6
- network_process_passed_pairs: 6
- network_process_pass_ratio: 1
- network_process_mode: mesh
- network_directed_edges_up: 12
- network_directed_edges_total: 12
- network_directed_edge_ratio: 1
- network_block_wire_available: True
- network_block_wire_pass: True
- network_block_wire_pass_ratio: 1
- network_block_wire_verified: 12
- network_block_wire_total: 12
- network_block_wire_verified_ratio: 1
- zk_ready: False
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
- full_scan_f04_status: InProgress
- full_scan_f05_status: InProgress
- full_scan_f06_status: NotStarted
- full_scan_f07_status: ReadyForMerge
- full_scan_f08_status: InProgress
- full_scan_f09_status: InProgress
- full_scan_f10_status: NotStarted
- full_scan_f11_status: NotStarted
- full_scan_f12_status: NotStarted
- full_scan_f13_status: NotStarted
- full_scan_f14_status: InProgress
- full_scan_f15_status: InProgress
- full_scan_f16_status: ReadyForMerge

## Full Scan Matrix (F-01~F-16)

| ID | Domain | Status | Auto Evidence |
|---|---|---|---|
| F-01 | AOEM execution entry | ReadyForMerge | exec=True, bindings=True, adapter_signal.pass=True |
| F-02 | AOEM runtime config | ReadyForMerge | exec=True, variant_digest.pass=True |
| F-03 | Execution receipt standard | ReadyForMerge | protocol=True, tx_codec=True, block_wire=True, block_out=True, commit_out=True |
| F-04 | State root consistency | InProgress | state_root.available=False, state_root.pass=True |
| F-05 | Consensus engine | InProgress | consensus=True, batch_a=True |
| F-06 | Distributed coordinator | NotStarted | coordinator=False |
| F-07 | Network layer | ReadyForMerge | network=True, process=True, block_wire=True, block_wire_negative=True |
| F-08 | Chain adapter interface | InProgress | adapter=True, abi=True, registry=True, consensus=True, compare=True |
| F-09 | zk execution/aggregation | InProgress | prover=False, zk_ready=False |
| F-10 | Web3 storage service | NotStarted | storage_service=False |
| F-11 | Domain system | NotStarted | app_domain=False |
| F-12 | DeFi core | NotStarted | app_defi=False |
| F-13 | Multi-chain plugin capability | NotStarted | adapters_multi=False |
| F-14 | vm-runtime split migration | InProgress | protocol=True, consensus=True, network=True, adapter=True, legacy_vm_runtime_present=False |
| F-15 | AOEM ZK capability contract | InProgress | zkvm_prove=False, zkvm_verify=False |
| F-16 | AOEM MSM acceleration contract | ReadyForMerge | msm_accel=True, msm_backend= |

## Ledger

| ID | Capability | Status | Auto Progress | Evidence Path | Updated |
|---|---|---|---|---|---|
| F-05 | Consensus engine (~80% verified) | InProgress | novovm-consensus skeleton + tx_codec_signal(pass=True, bytes=616) + mempool_admission_signal(pass=True, accepted=8, rejected=0, fee_floor=1) + tx_metadata_signal(pass=True, accounts=2, fee=1-5) + batch_a_closure(pass=True, txs=8, target_batches=2, expected_min_batches=2) + block_wire_signal(pass=True, codec=novovm_block_header_wire_v1, bytes=130) + block_output_signal(pass=True, batches=2, txs=8) + commit_output_signal(pass=True) are available | D:\WorksArea\SUPERVM\artifacts\migration\functional-smoke51-network-wire-negative\functional-consistency.json | 2026-03-03 |
| F-07 | Network layer (core-complete, production hardening pending) | ReadyForMerge | novovm-network skeleton + network_output_signal(pass=True) + network_closure_signal(pass=True) + network_process_signal(pass=True, mode=mesh, rounds=2/2, round_ratio=1, nodes=3, pairs=6/6, ratio=1, directed=12/12:1, block_wire=True(12/12:1), block_wire_round_ratio=1) + network_block_wire_negative(pass=True, available=True, expected_fail=True, reason_match=True, tamper=hash_mismatch, verified=0/2) are available | D:\WorksArea\SUPERVM\artifacts\migration\functional-smoke51-network-wire-negative\functional-consistency.json | 2026-03-03 |
| F-08 | Chain adapter API interface | InProgress | novovm-adapter-api + native/plugin backends + adapter_signal(pass=True, backend=plugin, chain=novovm, txs=8, accounts=10) + plugin_abi(pass=True, enabled=True, compatible=True, expected=1, required=0x1) + plugin_registry(pass=True, enabled=True, matched=True, strict=True, chain_allowed=True, hash_check=True/True, abi_whitelist=True/True) + consensus_binding(pass=True, available=True, class=consensus/1) + plugin_abi_negative(pass=True, available=True, abi_fail=True/True, cap_fail=True/True) + plugin_symbol_negative(pass=True, available=True, fail=True/True) + plugin_registry_negative(pass=True, available=True, hash_fail=True/True, whitelist_fail=True/True) + compare(enabled=True, available=True, pass=True, state_root_equal=True, native_backend=native, plugin_backend=plugin) are available | D:\WorksArea\SUPERVM\artifacts\migration\functional-smoke51-network-wire-negative\functional-consistency.json | 2026-03-03 |
| F-15 | AOEM ZK capability contract | InProgress | zkvm_prove=False / zkvm_verify=False | D:\WorksArea\SUPERVM\artifacts\migration\capabilities\capability-contract-core.json | 2026-03-03 |
| F-16 | AOEM MSM acceleration contract | ReadyForMerge | msm_accel=True / msm_backend= | D:\WorksArea\SUPERVM\artifacts\migration\capabilities\capability-contract-core.json | 2026-03-03 |

## Notes

- This file is auto-generated and does not replace the manual ledger.
- When state_root_available=false, proxy_digest consistency is used as a temporary gate.
- When baseline_ready=true and performance_compare_pass has a value, it can be used for regression threshold checks.
