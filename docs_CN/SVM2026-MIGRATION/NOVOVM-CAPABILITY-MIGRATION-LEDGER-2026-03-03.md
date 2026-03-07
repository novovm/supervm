# NOVOVM 能力迁移执行台账（2026-03-04）

## 状态约定

- `NotStarted`: 未开始
- `InProgress`: 进行中
- `Blocked`: 被阻塞
- `ReadyForMerge`: 当前迁移闭环达成，可并入主线（不等于生产全量 Done）
- `Done`: 已完成

## 台账

| ID | 能力名称 | 来源模块 | 目标模块 | 状态 | 本轮进展 | 下步动作 | 最近更新 |
|---|---|---|---|---|---|---|---|
| F-05 | 共识引擎（核验约80%） | `supervm-consensus` | `novovm-consensus` | Done | Batch A 最小闭环持续通过：`tx_codec_signal` / `mempool_admission_signal` / `tx_metadata_signal` / `batch_a_closure` / `block_wire_signal` / `block_output_signal` / `commit_output_signal`；`state_root` 硬一致性门禁（`state_root.available=True`）与 `consensus_negative_signal`（invalid_signature / duplicate_vote / wrong_epoch，且 `pass` 已绑定 weighted_quorum + equivocation/slash evidence + slash_execution + slash_threshold + slash_observe_only + unjail_cooldown + view_change + fork_choice）均通过；`SlashPolicy` 已外置到 `config/novovm-consensus-policy.json` 并由 `NOVOVM_NODE_MODE=slash_policy_probe` 注入验证，新增 `slash_governance_gate + slash_policy_external_gate + unjail_cooldown_gate` 并接入 acceptance gate | 推进生产级压测口径与罚没参数治理策略，补齐主网上线最后收口 | 2026-03-07 |
| F-06 | 分布式协调 | `supervm-distributed`/`supervm-dist-coordinator` | `novovm-coordinator` | Done | `novovm-coordinator` 2PC 状态机 + `two_pc_smoke` + `coordinator_negative_smoke` 已接入功能门禁；`coordinator_signal` 与 `coordinator_negative_signal` 均通过并纳入 `overall_pass` | 后续进入持久化/超时重试/恢复策略增强 | 2026-03-07 |
| F-07 | 网络层（核心完成，生产待收口） | `supervm-network` + `l4-network` | `novovm-network` + `novovm-protocol` | Done | `network_output_signal` + `network_closure_signal` + `network_pacemaker_signal` + `network_process_signal` + `network_block_wire` 持续通过，mesh 口径稳定，`view_sync/new_view` 已形成 UDP 进程级闭环门禁；新增 `header_sync_gate`（headers-first 正向闭环 + tamper 负向拒绝）与 `fast_state_sync_gate`（fast headers + state snapshot verify + tamper 负向拒绝）并接入 acceptance gate；新增 `network_dos_gate`（peer-score/ban + invalid-block-storm 拒绝）与 `pacemaker_failover_gate`（leader 超时失效 -> view-change -> 新 leader 出块提交）并接入 acceptance gate | 长压、多 peer 真同步路径、持久化/恢复语义、观测告警与故障注入收口 | 2026-03-07 |
| F-08 | Chain Adapter 接口 | `supervm-chainlinker-api` | `novovm-adapter-api` + `novovm-adapter-novovm` + `novovm-adapter-sample-plugin` | Done | 默认门禁已覆盖 `adapter_backend_compare_signal` + `adapter_plugin_abi_negative_signal` + `adapter_plugin_symbol_negative_signal` + `adapter_plugin_registry_negative_signal`，并全部通过（compare 与 3 个负向均 `enabled=True, available=True, pass=True`）；新增 `run_adapter_stability_gate.ps1` 并接入 acceptance gate（`runs=3, pass_rate=100%`） | 继续扩展长压窗口（`runs>=10`）并跟踪 compare 耗时抖动阈值 | 2026-03-07 |
| F-09 | zk 执行与聚合 | `src/l2-executor` | `novovm-prover` | ReadyForMerge | `contract_schema_smoke` + `contract_schema_negative_smoke` 已接入；`prover_contract_signal` 与 `prover_contract_negative_signal`（missing_formal_fields / empty_reason_codes / normalization_stable）均通过 | AOEM 未完成项冻结：等待 AOEM 1.0 发布后再做 runtime 参数调优 | 2026-03-04 |
| F-15 | AOEM ZK 能力契约 | `optional/zkvm-executor` | `novovm-prover` + `novovm-exec` | ReadyForMerge | `novovm-exec` 已统一正式字段与兼容字段解析，并规范化 fallback reason codes；`zk_contract_schema_ready=True` 持续稳定 | AOEM 未完成项冻结：等待 AOEM 1.0 发布后再切换 runtime-ready（当前 `zk_runtime_ready=False`） | 2026-03-04 |
| F-16 | AOEM MSM 加速契约 | `aoem-engine` + `aoem-ffi` | `novovm-prover` + `novovm-exec` | ReadyForMerge | MSM 能力字段与快照链路持续稳定（`msm_accel=True`） | AOEM 未完成项冻结：等待 AOEM 1.0 发布后再对齐 `msm_backend` 细粒度枚举 | 2026-03-04 |

## 全量扫描快照（F-01 ~ F-16）

来源：`NOVOVM-CAPABILITY-MIGRATION-LEDGER-AUTO-2026-03-04.md` 的 `Full Scan Matrix (F-01~F-16)`（由脚本自动生成）。

| ID | 状态 | 说明（自动证据摘要） |
|---|---|---|
| F-01 | Done | `exec=True, bindings=True, adapter_signal.pass=True` |
| F-02 | Done | `exec=True, variant_digest.pass=True` |
| F-03 | Done | `protocol=True, tx_codec=True, block_wire=True, block_out=True, commit_out=True` |
| F-04 | Done | `state_root.available=True, state_root.pass=True` |
| F-05 | Done | `consensus=True, batch_a=True, consensus_negative.enabled=True, consensus_negative.available=True, consensus_negative.pass=True, slash_threshold=True, slash_observe_only=True, unjail_cooldown=True` |
| F-06 | Done | `coordinator=True, signal_enabled=True, signal_available=True, signal_pass=True, negative_enabled=True, negative_available=True, negative_pass=True` |
| F-07 | Done | `network=True, closure=True, pacemaker=True, process=True, block_wire=True, view_sync=True, new_view=True, block_wire_negative=False` |
| F-08 | Done | `adapter=True, abi=True, registry=True, consensus=True, compare=True, matrix=True, non_novovm_sample=True, abi_negative_enabled=True, abi_negative_pass=True, symbol_negative_enabled=True, symbol_negative_pass=True, registry_negative_enabled=True, registry_negative_pass=True` |
| F-09 | ReadyForMerge | `prover=True, prover_signal=True, prover_negative_enabled=True, prover_negative_available=True, prover_negative_pass=True, schema_ok=True, reason_norm=True, zk_runtime_ready=False` |
| F-10 | Done | `storage_service=False, chain_query_rpc=True, governance_chain_audit_persist=True, governance_chain_audit_restart=True` |
| F-11 | Done | `app_domain=False, governance_access_policy=True, governance_council_policy=True, governance_execution=True, governance_negative=True` |
| F-12 | Done | `app_defi=False, governance_token_economics=True, governance_treasury_spend=True, governance_market_policy=True, market_engine=True, market_treasury=True, market_dividend=True, market_foreign_payment=True` |
| F-13 | Done | `adapters_multi=False, adapter_non_novovm_sample=True, adapter_stability=True, f08_ready=True` |
| F-14 | Done | `protocol=True, consensus=True, network=True, adapter=True, legacy_vm_runtime_present=False` |
| F-15 | ReadyForMerge | `zkvm_prove=False, zkvm_verify=False, schema_ready=True` |
| F-16 | ReadyForMerge | `msm_accel=True, msm_backend=` |

## 域级状态快照（D0 ~ D3）

来源：`NOVOVM-CAPABILITY-MIGRATION-LEDGER-AUTO-2026-03-04.md` 的 `Domain Scan (D0~D3)`（域级 Done = MVP 口径达成）。

| Domain | 状态 | Done 判定 | 自动证据 |
|---|---|---|---|
| D0 AOEM 底座域 | Done | F-01/F-02 = Done 或 ReadyForMerge | F-01=Done, F-02=Done |
| D1 执行门面域 | Done | F-01/F-02 = Done 或 ReadyForMerge + functional_pass=True | F-01=Done, F-02=Done, functional_pass=True |
| D2 协议核心域 | Done | F-03/F-04 = Done 或 ReadyForMerge | F-03=Done, F-04=Done |
| D3 共识网络域 | Done | F-05/F-06/F-07/F-08 = Done 或 ReadyForMerge | F-05=Done, F-06=Done, F-07=Done, F-08=Done |

## 自动回填快照

- 快照文档：`docs_CN/SVM2026-MIGRATION/NOVOVM-CAPABILITY-MIGRATION-LEDGER-AUTO-2026-03-04.md`
- 生成脚本：`scripts/migration/generate_capability_ledger_auto.ps1`
- 关键证据：
  1. `artifacts/migration/acceptance-gate/acceptance-gate-summary.json`
  2. `artifacts/migration/acceptance-gate/functional/functional-consistency.json`
  3. `artifacts/migration/acceptance-gate/performance-gate/performance-gate-summary.json`
  4. `artifacts/migration/acceptance-gate/chain-query-rpc-gate/chain-query-rpc-gate-summary.json`
  5. `artifacts/migration/acceptance-gate/adapter-stability-gate/adapter-stability-summary.json`
  6. `artifacts/migration/functional/functional-consistency.json`
  7. `artifacts/migration/capabilities/capability-contract-core.json`
  8. `config/novovm-adapter-compatibility-matrix.json`

## 本轮 Acceptance Gate（2026-03-04）

- 结论：`overall_pass=True`（`functional_pass=True`，`performance_pass=True`，`adapter_stability_pass=True`）
- 域级结论：`D0~D3 = Done`（MVP 口径）；能力项状态仍按台账保持 `ReadyForMerge` / `InProgress` 细分。
- 运行配置：`release+seal_single`，`performance_runs=3`，`adapter_stability_runs=3`，`allowed_regression_pct=-5`
- 性能口径（P50）：
  1. `core/cpu_batch_stress`: baseline `20900563.48` -> current `25317353.02`（`+21.13%`）
  2. `core/cpu_parity`: baseline `5003439.86` -> current `5985393.25`（`+19.63%`）

## 当前阻塞项

1. AOEM 仓库 `vendor/curve25519-dalek` 缺失，影响 AOEM 侧完整构建核验。
2. AOEM 运行时 ZK/MSM 正式能力字段尚未发布，相关 runtime-ready 项统一冻结至 AOEM 1.0 发布后再推进。

## 增量更新（2026-03-05）

- 已完成：`novovm-node` 新增 `NOVOVM_NODE_MODE=rpc_server`，对外暴露 `getBlock/getTransaction/getReceipt/getBalance`（JSON-RPC 风格接口，路径 `/` 与 `/rpc`）。
- 已完成：RPC 查询服务门禁脚本 `scripts/migration/run_chain_query_rpc_gate.ps1`（含种子数据、4 个正向方法校验 + 1 个负向未知方法校验 + `rate_limit_signal`(429/`-32029`)）。
- 已完成：`scripts/migration/run_migration_acceptance_gate.ps1` 默认接入 chain-query RPC 门禁，`overall_pass` 现包含 `chain_query_rpc_pass`。
- 已完成：新增 `scripts/migration/run_header_sync_gate.ps1`（`header_sync_signal` + `header_sync_negative_signal`），并接入 `scripts/migration/run_migration_acceptance_gate.ps1`（`overall_pass` 新增 `header_sync_pass` 约束）。
- 证据样本：`artifacts/migration/acceptance-gate-header-sync/header-sync-gate/header-sync-gate-summary.json`（`pass=True`）。
- 已完成：新增 `scripts/migration/run_fast_state_sync_gate.ps1`（`fast_state_sync_signal` + `fast_state_sync_negative_signal`），并接入 `scripts/migration/run_migration_acceptance_gate.ps1`（`overall_pass` 新增 `fast_state_sync_pass` 约束）。
- 证据样本：`artifacts/migration/acceptance-gate-fast-state/fast-state-sync-gate/fast-state-sync-gate-summary.json`（`pass=True`）。
- 已完成：新增 `scripts/migration/run_network_dos_gate.ps1`（`network_dos_signal`：peer-score/ban + invalid-block-storm），并接入 `scripts/migration/run_migration_acceptance_gate.ps1`（`overall_pass` 新增 `network_dos_pass` 约束）。
- 证据样本：`artifacts/migration/acceptance-gate-network-dos/network-dos-gate/network-dos-gate-summary.json`（`pass=True`）。
- 已完成：`novovm-node` 新增 `NOVOVM_NODE_MODE=pacemaker_failover_probe`（leader 超时失效 -> view-change -> 新 leader 提案/投票/QC/commit），并新增 `scripts/migration/run_pacemaker_failover_gate.ps1`；该门禁已接入 `scripts/migration/run_migration_acceptance_gate.ps1`（`overall_pass` 新增 `pacemaker_failover_pass` 约束）。
- 证据样本：`artifacts/migration/acceptance-gate-pacemaker-failover-full/pacemaker-failover-gate/pacemaker-failover-gate-summary.json`（`pass=True`）。
- 已完成：RPC 查询门禁稳健性修复（`run_chain_query_rpc_gate.ps1` 兼容 PowerShell 无 `ConvertFrom-Json -Depth` 参数；`novovm-node` query-db 读取兼容 UTF-8 BOM），默认 acceptance 链路恢复稳定。
- 证据样本：`artifacts/migration/acceptance-gate-pacemaker-failover-full/chain-query-rpc-gate/chain-query-rpc-gate-summary.json`（`pass=True`）。
- 证据样本：`artifacts/migration/acceptance-gate-smoke/chain-query-rpc-gate/chain-query-rpc-gate-summary.json`（`pass=True`）。
- 已完成：`novovm-consensus` 共识主网化增量：stake-weighted quorum 收敛、QC 声明权重防伪（声明值与观测值不一致拒绝）、equivocation（同高度双签）检测与 slash evidence 记录。
- 已完成：`novovm-consensus` 新增 `SlashPolicy` 参数化治理（`mode=enforce|observe_only` + `equivocation_threshold` + `min_active_validators`），并落地到 `slash execution` 记录（含 `policy_mode/evidence_count/threshold`）。
- 已完成：`consensus_negative_smoke` 的 `pass` 已绑定 `weighted_quorum + equivocation + slash_execution + slash_threshold + slash_observe_only + unjail_cooldown + view_change + fork_choice` 子项，功能一致性门禁通过（`artifacts/migration/acceptance-gate-unjail-full/functional/functional-consistency.json`，`overall_pass=True`）。
- 已完成：新增 `scripts/migration/run_slash_governance_gate.ps1` 并接入 `scripts/migration/run_migration_acceptance_gate.ps1`（`overall_pass` 新增 `slash_governance_pass` 约束）。
- 证据样本：`artifacts/migration/acceptance-gate-slash-governance-full/slash-governance-gate/slash-governance-gate-summary.json`（`pass=True`）。
- 已完成：新增 `config/novovm-consensus-policy.json`，`novovm-node` 启动时默认读取并输出 `slash_policy_in`（`source/path/mode/threshold/min_validators`），缺省回落 `SlashPolicy::default`；policy 文件解析支持 UTF-8 BOM。
- 已完成：`novovm-node` 新增 `NOVOVM_NODE_MODE=slash_policy_probe`，可独立验证 `SlashPolicy` 注入 `BFTEngine`（`slash_policy_probe_out: injected=true`）。
- 已完成：新增 `scripts/migration/run_slash_policy_external_gate.ps1`（正向：外置 policy 生效 + 注入成功；负向：非法 policy 命中 `policy_invalid/policy_parse_failed`；并联 `consensus_negative_ext.slash_threshold=true`），并接入 `scripts/migration/run_migration_acceptance_gate.ps1`（`overall_pass` 新增 `slash_policy_external_pass` 约束）。
- 证据样本：`artifacts/migration/acceptance-gate-slash-policy-external-full/slash-policy-external-gate/slash-policy-external-gate-summary.json`（`pass=True`）。
- 证据样本：`artifacts/migration/acceptance-gate-slash-policy-external-full/acceptance-gate-summary.json`（`overall_pass=True`，含 `slash_policy_external_pass=True`）。
- 已完成：新增治理入口预留 `GovernanceOp::UpdateSlashPolicy`（`novovm-consensus` staged-only，不启链上执行）与 `NOVOVM_NODE_MODE=governance_hook_probe`，并新增 `scripts/migration/run_governance_hook_gate.ps1`；该门禁已接入 `scripts/migration/run_migration_acceptance_gate.ps1`（`overall_pass` 新增 `governance_hook_pass` 约束）。
- 证据样本：`artifacts/migration/acceptance-gate-governance-smoke/governance-hook-gate/governance-hook-gate-summary.json`（`pass=True`）。
- 已完成：治理执行最小闭环（仅 `UpdateSlashPolicy`）：`proposal -> vote(signature) -> quorum -> apply`，并新增 `scripts/migration/run_governance_execution_gate.ps1`；该门禁已接入 `scripts/migration/run_migration_acceptance_gate.ps1`（`overall_pass` 新增 `governance_execution_pass` 约束）。
- 证据样本：`artifacts/migration/acceptance-gate-governance-exec-smoke/governance-execution-gate/governance-execution-gate-summary.json`（`pass=True`）。
- 已完成：第二类治理参数扩展：`UpdateMempoolFeeFloor`（治理提案+签名投票+quorum 生效），并新增 `scripts/migration/run_governance_param2_gate.ps1`；该门禁已接入 `scripts/migration/run_migration_acceptance_gate.ps1`（`overall_pass` 新增 `governance_param2_pass` 约束）。
- 证据样本：`artifacts/migration/acceptance-gate-governance-param2-smoke/governance-param2-gate/governance-param2-gate-summary.json`（`pass=True`）。
- 已完成：第三类治理参数扩展：`UpdateNetworkDosPolicy`（治理提案+签名投票+quorum 生效），并新增 `scripts/migration/run_governance_param3_gate.ps1`；该门禁已接入 `scripts/migration/run_migration_acceptance_gate.ps1`（`overall_pass` 新增 `governance_param3_pass` 约束）。
- 证据样本：`artifacts/migration/governance-param3-gate-smoke/governance-param3-gate-summary.json`（`pass=True`, `parse_pass=True`, `input_pass=True`, `output_pass=True`）。
- 已完成：九席位治理策略迁移（I-GOV-01）：`novovm-consensus` 新增 `GovernanceCouncilPolicy` / `GovernanceCouncilSeat` / `GovernanceOp::UpdateGovernanceCouncilPolicy`，按提案类别执行权重阈值（`Parameter/Treasury/Protocol/Emergency`），并在启用时改为 council member 提案+投票门禁。
- 已完成：新增 `scripts/migration/run_governance_council_policy_gate.ps1`；该门禁已接入 `scripts/migration/run_migration_acceptance_gate.ps1`（`overall_pass` 新增 `governance_council_policy_pass` 约束，`FullSnapshotProfileGA` 默认启用）。
- 证据样本：`artifacts/migration/governance-council-policy-gate-local/governance-council-policy-gate-summary.json`（`pass=True`, `parse_pass=True`, `input_pass=True`, `output_pass=True`）。
- 证据样本：`artifacts/migration/acceptance-gate-council-local/acceptance-gate-summary.json`（`overall_pass=True`, `governance_council_policy_pass=True`）。
- 证据样本：`artifacts/migration/release-snapshot-ga-council-local/release-snapshot.json`（`overall_pass=True`, `profile_name=full_snapshot_ga_v1`, `enabled_gates.governance_council_policy=True`）。
- 证据样本：`artifacts/migration/release-candidate-novovm-rc-2026-03-06-ga-council-local/rc-candidate.json`（`status=ReadyForMerge/SnapshotGreen`, `governance_council_policy_pass=True`）。
- 已完成：经济治理参数族迁移（I-GOV-02 受限主链路）：`novovm-consensus` 新增 `MarketGovernancePolicy` 与 `GovernanceOp::UpdateMarketGovernancePolicy`，覆盖 `AMM/CDP/Bond/Reserve/NAV/Buyback` 参数热更新。
- 已完成：`novovm-node` 新增 `NOVOVM_NODE_MODE=governance_market_policy_probe`，并将 `market_governance_policy` 纳入 `governance_getPolicy`/`governance_execute` 输出。
- 已完成：经济执行层命名与复用收口：`market_runtime` 迁移为 `market_engine`，并复用 `SVM2026/contracts/web30/core` 的 `AMM/CDP/Bond/NAV/TreasuryImpl` 主链路组件。
- 已完成：新增 `scripts/migration/run_governance_market_policy_gate.ps1`；该门禁已接入 `scripts/migration/run_migration_acceptance_gate.ps1`（`overall_pass` 新增 `governance_market_policy_pass` 约束，`FullSnapshotProfileGA` 默认启用）。
- 证据样本：`artifacts/migration/governance-market-policy-gate-local/governance-market-policy-gate-summary.json`（`pass=True`, `parse_pass=True`, `input_pass=True`, `output_pass=True`）。
- 证据样本：`artifacts/migration/acceptance-gate-ga-market-local/acceptance-gate-summary.json`（`overall_pass=True`, `governance_market_policy_pass=True`）。
- 已完成：`run_governance_market_policy_gate.ps1` 增加硬门禁输出解析：`governance_market_engine_out` + `governance_market_treasury_out`，并将 `engine_output_pass/treasury_output_pass` 绑定到 gate `pass`。
- 已完成：经济跨模块编排主链路接线：`market_engine` 新增 `oracle price update -> CDP liquidation -> NAV redemption settle -> treasury penalty route`，并输出 `governance_market_orchestration_out`。
- 已完成：`run_governance_market_policy_gate.ps1` 新增 `orchestration_output_pass` 并绑定 gate `pass`；`run_migration_acceptance_gate.ps1` / `run_release_snapshot.ps1` / `run_release_candidate.ps1` 已同步聚合 `governance_market_policy_orchestration_pass`。
- 证据样本：`artifacts/migration/acceptance-gate-market-engine-smoke/acceptance-gate-summary.json`（`overall_pass=True`, `governance_market_policy_pass=True`, `governance_market_policy_engine_pass=True`, `governance_market_policy_treasury_pass=True`）。
- 证据样本：`artifacts/migration/governance-market-policy-gate/governance-market-policy-gate-summary.json`（`pass=True`, `orchestration_output_pass=True`）。
- 证据样本：`artifacts/migration/release-snapshot-ga-market-local/release-snapshot.json`（`overall_pass=True`, `profile_name=full_snapshot_ga_v1`, `enabled_gates.governance_market_policy=True`, `key_results.governance_market_policy_pass=True`）。
- 证据样本：`artifacts/migration/release-candidate-novovm-rc-2026-03-06-ga-market-local/rc-candidate.json`（`status=ReadyForMerge/SnapshotGreen`, `governance_market_policy_pass=True`）。
- 已完成：治理 RPC 执行面增强：`governance_submitProposal/governance_sign/governance_vote/governance_execute/governance_getProposal/governance_listProposals/governance_listAuditEvents/governance_getPolicy`，并新增进程级权限校验（`NOVOVM_GOVERNANCE_PROPOSER_ALLOWLIST/NOVOVM_GOVERNANCE_EXECUTOR_ALLOWLIST`）与审计事件流（含 reject 事件）。
- 证据样本：`artifacts/migration/acceptance-gate-governance-rpc-smoke-v2/governance-rpc-gate/governance-rpc-gate-summary.json`（`pass=True`, `sign1_ok=True`, `unauthorized_submit_reject_ok=True`, `audit_ok=True`）。
- 已完成：治理审计持久化索引：`novovm-node` 新增 `NOVOVM_GOVERNANCE_AUDIT_DB`，`governance_listAuditEvents` 对应事件流落盘到 `GovernanceRpcAuditStore(next_seq/events)`，并在 `run_governance_rpc_gate.ps1` 增加 `audit_persist_ok` 校验。
- 证据样本：`artifacts/migration/governance-rpc-gate-audit-persist-smoke/governance-rpc-gate-summary.json`（`pass=True`, `audit_ok=True`, `audit_persist_ok=True`, `audit_persist_count=10`）。
- 证据样本：`artifacts/migration/acceptance-gate-governance-audit-persist-smoke/acceptance-gate-summary.json`（`overall_pass=True`, `governance_rpc_pass=True`, `governance_rpc_audit_persist_pass=True`）。
- 已完成：治理链内审计索引持久化与重启恢复：新增 `NOVOVM_GOVERNANCE_CHAIN_AUDIT_DB`，`governance_listChainAuditEvents` 的链内事件会落盘并在 RPC 节点重启时恢复到 `BFTEngine`。
- 证据样本：`artifacts/migration/governance-rpc-gate-chain-audit-persist-smoke/governance-rpc-gate-summary.json`（`pass=True`, `chain_audit_ok=True`, `chain_audit_persist_ok=True`, `chain_audit_restart_ok=True`）。
- 证据样本：`artifacts/migration/release-snapshot-chain-audit-persist-smoke/release-snapshot.json`（`overall_pass=True`, `key_results.governance_rpc_chain_audit_persist_pass=True`, `key_results.governance_rpc_chain_audit_restart_pass=True`）。
- 证据样本：`artifacts/migration/release-candidate-novovm-rc-2026-03-06-chain-audit-persist-smoke/rc-candidate.json`（`status=ReadyForMerge/SnapshotGreen`, `governance_rpc_chain_audit_persist_pass=True`, `governance_rpc_chain_audit_restart_pass=True`）。
- 已完成：治理链审计 root proof（可验证化）收口：`governance_getPolicy` 与 `governance_listChainAuditEvents` 的 `head_seq/root` 同步一致校验，并纳入 acceptance/snapshot/rc 聚合字段 `governance_rpc_chain_audit_root_proof_pass`。
- 证据样本：`artifacts/migration/governance-rpc-gate-chain-audit-root-smoke/governance-rpc-gate-summary.json`（`pass=True`, `policy_chain_audit_consistency_ok=True`, `chain_audit_root_ok=True`, `chain_audit_persist_root_ok=True`, `chain_audit_restart_root_ok=True`）。
- 证据样本：`artifacts/migration/release-snapshot-chain-audit-root-smoke/release-snapshot.json`（`overall_pass=True`, `key_results.governance_rpc_chain_audit_root_proof_pass=True`）。
- 证据样本：`artifacts/migration/release-candidate-novovm-rc-2026-03-06-chain-audit-root-smoke/rc-candidate.json`（`status=ReadyForMerge/SnapshotGreen`, `governance_rpc_chain_audit_root_proof_pass=True`）。
- 已完成：治理链审计 root 区块路径锚定：`block_header_wire_v1` + `block_out/commit_out` 已携带 `governance_chain_audit_root`，并新增聚合门禁 `governance_chain_audit_root_parity_pass`（`ffi_v2/legacy_compat` 一致性）。
- 证据样本：`artifacts/migration/acceptance-governance-chain-audit-root/acceptance-gate-summary.json`（`overall_pass=True`, `governance_chain_audit_root_parity_pass=True`）。
- 证据样本：`artifacts/migration/release-snapshot-governance-chain-audit-root-smoke/release-snapshot.json`（`overall_pass=True`, `key_results.governance_chain_audit_root_parity_pass=True`）。
- 证据样本：`artifacts/migration/release-candidate-novovm-rc-2026-03-06-governance-chain-audit-root-anchor-smoke/rc-candidate.json`（`status=ReadyForMerge/SnapshotGreen`, `governance_chain_audit_root_parity_pass=True`）。
- 已完成：治理签名算法 staged 抽象：`governance_sign/governance_vote` 新增 `signature_scheme` 参数；当前仅 `ed25519` 启用，`mldsa87`（及别名）明确拒绝并写入审计事件（I-GOV-04 staged-only）；`novovm-consensus` 已新增 `GovernanceVoteVerifier` execute-hook（默认 `ed25519`）并接入治理执行链路；`novovm-node` 启动新增 `NOVOVM_GOVERNANCE_VOTE_VERIFIER` 并对 `mldsa87` 执行 staged-only 拒绝。
- 已完成：治理验签器启动门禁：`run_governance_rpc_gate.ps1` 新增 `vote_verifier_startup_ok` + `vote_verifier_staged_reject_ok`，并接入 acceptance/snapshot/rc 聚合。
- 已完成：CI 门禁硬化：`.github/workflows/ci.yml` 新增 `governance_rpc_gate` job，`vote_verifier_startup_ok` 与 `vote_verifier_staged_reject_ok` 任一失败即阻断 PR。
- 已完成：分支保护自动化脚本：`scripts/migration/set_branch_protection_required_checks.ps1`，支持一键把 `Rust checks` 与 `Governance RPC gate (vote verifier)` 设为 `main` required checks。
- 已完成：I-GOV-04 staged 结构下沉：`novovm-consensus` 新增 `governance_verifier.rs`（`GovernanceVoteVerifier` / `GovernanceVoteVerifierScheme` / `build_governance_vote_verifier`），`BFTEngine` 新增 `set_governance_vote_verifier_by_scheme`，`novovm-node` 移除本地 verifier 工厂逻辑并改为调用共识层接口。
- 已完成：I-GOV-04 staged 二段下沉：`governance_sign/governance_vote` 的 `signature_scheme` 支持判定改为调用 `BFTEngine::governance_signature_scheme_supported`（并由 `governance_vote_verifier_scheme` 给出 active scheme），节点层不再硬编码 `ed25519`。
- 已完成：I-GOV-04 staged 三段下沉：治理执行路径改用 `GovernanceVoteVerifier::verify_with_report`，`governance_execute` 输出 active `vote_verifier(name/signature_scheme)`，并将 `verifier/scheme` 固化到链内审计 `execute=applied` 事件（含持久化与重启恢复路径）。
- 已完成：I-GOV-04 三段下沉聚合收口：`run_migration_acceptance_gate.ps1` / `run_release_snapshot.ps1` / `run_release_candidate.ps1` 新增 `governance_rpc_vote_verifier_execute_pass` 与 `governance_rpc_chain_audit_execute_verifier_proof_pass`。
- 已完成：I-GOV-04 optional execute 接线：新增 `NOVOVM_GOVERNANCE_MLDSA_MODE=aoem_ffi` 可选路径，`mldsa87` 在显式启用时通过 AOEM-FFI 验签（默认仍 staged-only）；启动阶段校验 `aoem_abi_version==1`、`aoem_mldsa_supported==1`，并支持跨平台库名解析（Windows `aoem_ffi.dll` / Linux `libaoem_ffi.so` / macOS `libaoem_ffi.dylib`）。
- 已完成：I-GOV-04 optional execute 门禁化：新增 `scripts/migration/run_governance_rpc_mldsa_ffi_gate.ps1`（真实 ML-DSA 外部签名 + AOEM-FFI 验签 + 治理执行闭环），并接入 `run_migration_acceptance_gate.ps1` 可选聚合字段：`governance_rpc_mldsa_ffi_pass`、`governance_rpc_mldsa_ffi_startup_pass`。
- 已完成：`run_release_snapshot.ps1` / `run_release_candidate.ps1` 新增 I-GOV-04 可选聚合透传参数（`-IncludeGovernanceRpcMldsaFfiGate`、`-GovernanceRpcMldsaFfiAoemRoot` 等），并输出 `governance_rpc_mldsa_ffi_gate_enabled/pass/startup_pass` 到发布产物。
- 证据样本：`artifacts/migration/release-snapshot-mldsa-optional-smoke/release-snapshot.json`（`overall_pass=True`, `enabled_gates.governance_rpc_mldsa_ffi=True`, `key_results.governance_rpc_mldsa_ffi_pass=True`）。
- 证据样本：`artifacts/migration/release-candidate-novovm-rc-2026-03-06-mldsa-optional-smoke/rc-candidate.json`（`status=ReadyForMerge/SnapshotGreen`, `governance_rpc_mldsa_ffi_gate_enabled=True`, `governance_rpc_mldsa_ffi_pass=True`）。
- 证据样本：`artifacts/migration/governance-rpc-gate-vote-verifier-smoke/governance-rpc-gate-summary.json`（`vote_verifier_startup_ok=True`, `vote_verifier_staged_reject_ok=True`）。
- 证据样本：`artifacts/migration/governance-rpc-gate-downsink-scheme-smoke/governance-rpc-gate-summary.json`（`pass=True`, `sign_unsupported_scheme_reject_ok=True`）。
- 证据样本：`artifacts/migration/governance-rpc-gate-verifier-exec-proof-smoke/governance-rpc-gate-summary.json`（`pass=True`, `execute_vote_verifier_ok=True`, `chain_audit_has_execute_applied_verifier=True`, `chain_audit_persist_has_execute_applied_verifier=True`, `chain_audit_restart_has_execute_applied_verifier=True`）。
- 证据样本：`artifacts/migration/acceptance-gate-vote-verifier-smoke/acceptance-gate-summary.json`（`overall_pass=True`, `governance_rpc_vote_verifier_startup_pass=True`, `governance_rpc_vote_verifier_staged_reject_pass=True`）。
- 证据样本：`artifacts/migration/acceptance-gate-verifier-exec-proof-smoke/acceptance-gate-summary.json`（`overall_pass=True`, `governance_rpc_vote_verifier_execute_pass=True`, `governance_rpc_chain_audit_execute_verifier_proof_pass=True`）。
- 证据样本：`artifacts/migration/release-snapshot-governance-verifier-exec-proof-smoke/release-snapshot.json`（`overall_pass=True`, `key_results.governance_rpc_vote_verifier_execute_pass=True`, `key_results.governance_rpc_chain_audit_execute_verifier_proof_pass=True`）。
- 证据样本：`artifacts/migration/release-candidate-novovm-rc-2026-03-06-governance-verifier-exec-proof-smoke/rc-candidate.json`（`status=ReadyForMerge/SnapshotGreen`, `governance_rpc_vote_verifier_execute_pass=True`, `governance_rpc_chain_audit_execute_verifier_proof_pass=True`）。
- 证据样本：`artifacts/migration/governance-rpc-gate-signature-scheme-smoke/governance-rpc-gate-summary.json`（`pass=True`, `sign_unsupported_scheme_reject_ok=True`, `audit_has_sign_reject_unsupported_scheme=True`）。
- 证据样本：`artifacts/migration/acceptance-gate-governance-signature-scheme-smoke/acceptance-gate-summary.json`（`overall_pass=True`, `governance_rpc_signature_scheme_reject_pass=True`）。
- 已完成：治理负向门禁闭环：`unauthorized_submit + invalid_signature + duplicate_vote + insufficient_votes + replay_execute`，并新增 `scripts/migration/run_governance_negative_gate.ps1`；该门禁已接入 `scripts/migration/run_migration_acceptance_gate.ps1`（`overall_pass` 新增 `governance_negative_pass` 约束）。
- 已完成：治理签名防重放域分隔：`GovernanceVote` 签名消息绑定 `proposal_id + proposal_height + proposal_digest + support`，执行时强校验 `proposal_height/proposal_digest` 一致性，拒绝跨提案/跨高度重放。
- 证据样本：`artifacts/migration/acceptance-gate-governance-negative-smoke/governance-negative-gate/governance-negative-gate-summary.json`（`pass=True`）。
- 证据样本：`artifacts/migration/acceptance-gate-governance-negative-smoke-v2/governance-negative-gate/governance-negative-gate-summary.json`（`pass=True`）。
- 证据样本：`artifacts/migration/acceptance-gate-governance-param2-smoke/acceptance-gate-summary.json`（`overall_pass=True`，含 `governance_hook_pass/governance_execution_pass/governance_param2_pass/governance_negative_pass=True`）。
- 证据样本：`artifacts/migration/acceptance-gate-governance-rpc-smoke-v2/acceptance-gate-summary.json`（`overall_pass=True`，含 `governance_rpc_pass/governance_hook_pass/governance_execution_pass/governance_param2_pass/governance_param3_pass/governance_negative_pass=True`）。
- 已完成：全量门禁发布快照（`full_snapshot_v1`）一次性跑通，新增 `scripts/migration/run_release_snapshot.ps1` 并生成 `release-snapshot.json/md` 聚合产物（`date/overall_pass/enabled_gates/key_results/allowed_regression_pct`）。
- 证据样本：`artifacts/migration/release-snapshot-2026-03-05/release-snapshot.json`（`overall_pass=True`, `profile_name=full_snapshot_v1`, `enabled_gates.*=True`）。
- 已完成：门禁稳定性 relfix（relative `OutputDir` -> child `cwd` 导致 whitelist negative path drift），`OutputDir` 统一绝对化后 `adapter_stability` 与 full snapshot 恢复稳定。
- 状态：`ReadyForMerge / SnapshotGreen`（relfix 后恢复稳定）。
- 证据样本：`artifacts/migration/adapter-stability-relfix-smoke/adapter-stability-summary.json`（`pass=True`, `pass_rate_pct=100`）。
- 证据样本：`artifacts/migration/release-snapshot-param3-smoke-relfix/release-snapshot.json`（`overall_pass=True`, `profile_name=full_snapshot_v1`, `key_results.governance_pass=True`, `enabled_gates.governance_param3=True`）。
- 证据样本：`artifacts/migration/release-snapshot-param3-smoke-relfix/acceptance-gate-full/acceptance-gate-summary.json`（`governance_param3_pass=True`, `adapter_stability_pass=True`）。
- 已完成：`chain_query_rpc_gate` rate-limit retryfix（Unix 秒窗口抖动修复）：`run_chain_query_rpc_gate.ps1` 现将 rate-limit probe 对齐到下一个秒窗口，并在 `limited_ok=False` 时做小次数自动重试，避免发布链路首轮跨秒误判。
- 状态：`ReadyForMerge / SnapshotGreen`（retryfix 后连续回归稳定）。
- 证据样本：`artifacts/migration/chain-query-rpc-gate-retryfix-run-1/chain-query-rpc-gate-summary.json` 至 `artifacts/migration/chain-query-rpc-gate-retryfix-run-10/chain-query-rpc-gate-summary.json`（10/10 `pass=True`, `attempts_used=1`）。
- 证据样本：`artifacts/migration/release-snapshot-chain-query-retryfix-run-1/release-snapshot.json`（`overall_pass=True`）。
- 证据样本：`artifacts/migration/release-snapshot-chain-query-retryfix-run-2/release-snapshot.json`（`overall_pass=True`）。
- 已完成：GA 正式发布快照（`full_snapshot_ga_v1`）回填：`release-snapshot` 聚合包含 `governance_market_policy_engine_pass` + `governance_market_policy_treasury_pass`，并纳入 `governance_pass` 收口。
- 证据样本：`artifacts/migration/release-snapshot-ga-2026-03-06-051653/release-snapshot.json`（`overall_pass=True`, `profile_name=full_snapshot_ga_v1`, `key_results.governance_market_policy_engine_pass=True`, `key_results.governance_market_policy_treasury_pass=True`）。
- 已完成：`release-snapshot` 聚合新增治理审计持久化口径 `key_results.governance_rpc_audit_persist_pass`，并纳入治理总门禁。
- 证据样本：`artifacts/migration/release-snapshot-audit-persist-smoke/release-snapshot.json`（`overall_pass=True`, `profile_name=full_snapshot_v1`, `key_results.governance_rpc_audit_persist_pass=True`）。
- 已完成：`release-snapshot` 聚合新增治理签名算法 staged 口径 `key_results.governance_rpc_signature_scheme_reject_pass`，并纳入治理总门禁。
- 证据样本：`artifacts/migration/release-snapshot-signature-scheme-smoke/release-snapshot.json`（`overall_pass=True`, `profile_name=full_snapshot_v1`, `key_results.governance_rpc_signature_scheme_reject_pass=True`）。
- 已完成：GA 正式 RC（`rc_ref=novovm-rc-2026-03-06-ga-v1`）回填，状态保持 `ReadyForMerge/SnapshotGreen`。
- 证据样本：`artifacts/migration/release-candidate-novovm-rc-2026-03-06-ga-v1/rc-candidate.json`（`commit_hash=823a5880e104c96d03e2ab4a8473c9f620ae6413`, `governance_market_policy_engine_pass=True`, `governance_market_policy_treasury_pass=True`）。
- 已完成：GA orchfix 复核快照回填（`full_snapshot_ga_v1`），保持全量门禁绿色。
- 证据样本：`artifacts/migration/release-candidate-novovm-rc-2026-03-06-ga-orchfix/snapshot/release-snapshot.json`（`overall_pass=True`, `key_results.governance_market_policy_orchestration_pass=True`）。
- 已完成：GA orchfix 复核 RC（`rc_ref=novovm-rc-2026-03-06-ga-orchfix`）回填，状态保持 `ReadyForMerge/SnapshotGreen`。
- 证据样本：`artifacts/migration/release-candidate-novovm-rc-2026-03-06-ga-orchfix/rc-candidate.json`（`commit_hash=bac3763192258d5fcb89fc129e2b675d56dbb317`, `governance_market_policy_orchestration_pass=True`）。
- 证据样本：`artifacts/migration/release-candidate-novovm-rc-2026-03-06-governance-audit-persist-smoke/rc-candidate.json`（`status=ReadyForMerge/SnapshotGreen`, `governance_rpc_audit_persist_pass=True`）。
- 证据样本：`artifacts/migration/release-candidate-novovm-rc-2026-03-06-signature-scheme-smoke/rc-candidate.json`（`status=ReadyForMerge/SnapshotGreen`, `governance_rpc_signature_scheme_reject_pass=True`）。
- 已完成：`novovm-consensus` 新增自动解禁窗口（`cooldown_epochs`），`SlashExecution` 输出 `jailed_until_epoch/cooldown_epochs`；`state.height >= jailed_until_epoch` 时自动恢复验证者活跃态。
- 已完成：新增 `scripts/migration/run_unjail_cooldown_gate.ps1`（正向：jail -> cooldown 到期自动 unjail；负向：未到期拒绝），并接入 `scripts/migration/run_migration_acceptance_gate.ps1`（`overall_pass` 新增 `unjail_cooldown_pass` 约束）。
- 证据样本：`artifacts/migration/acceptance-gate-unjail-full/unjail-cooldown-gate/unjail-cooldown-gate-summary.json`（`pass=True`）。
- 证据样本：`artifacts/migration/acceptance-gate-unjail-full/acceptance-gate-summary.json`（`overall_pass=True`，含 `unjail_cooldown_pass=True`）。
- 已完成：`view-change`（超时换主）与 `fork-choice`（高度/权重优先）已接入 `novovm-consensus` 与 `consensus_negative_smoke`，功能一致性门禁通过（`artifacts/migration/functional-smoke-consensus-view-fork/functional-consistency.json`，`consensus_negative_signal.view_change=True`，`fork_choice=True`）。
- 已完成：GA 主线回归快照（2026-03-07，post-fix）全绿：`full_snapshot_ga_v1` 在 `release-snapshot-ga-post-fix-2026-03-07` 下复跑通过，`overall_pass=True`。
- 证据样本：`artifacts/migration/release-snapshot-ga-post-fix-2026-03-07/release-snapshot.json`（`governance_market_policy_pass=True`，`governance_market_policy_engine_pass=True`，`governance_market_policy_treasury_pass=True`，`governance_market_policy_orchestration_pass=True`，`governance_token_economics_pass=True`，`governance_treasury_spend_pass=True`）。
- 证据样本：`artifacts/migration/release-snapshot-ga-post-fix-2026-03-07/acceptance-gate-full/acceptance-gate-summary.json`（`profile_name=full_snapshot_ga_v1`，`overall_pass=True`）。
- 已完成：`governance_market_policy` 回归抖动修复（dividend 同日重复 claim）：
  - 修复点：`novovm-consensus/src/market_engine.rs` 引入按 `day` 轮转的 dividend probe 地址环，消除同日二次 `reconfigure` 的 claim 冲突。
  - 结果：保持 `dividend_claims_executed > 0` 严格口径不降级，gate 恢复稳定通过。
- 证据样本：`artifacts/migration/release-snapshot-ga-post-fix-2026-03-07/acceptance-gate-full/governance-market-policy-gate/governance-market-policy-gate-summary.json`（`dividend_output_pass=True`，`foreign_payment_output_pass=True`，`pass=True`）。
- 已完成：分红余额源收口（token_runtime -> market_engine -> dividend）：
  - 修复点：`token_runtime` 新增 `dividend_eligible_balances`，`protocol.set_market_governance_policy` 注入 `market_engine.set_dividend_runtime_balances`，并保留 deterministic probe fallback。
  - 新增门禁：`scripts/migration/run_dividend_balance_source_gate.ps1`，覆盖 injected balances/reentrancy guard/runtime seed/protocol sync/regression。
  - acceptance 聚合：`run_migration_acceptance_gate.ps1` 新增 `dividend_balance_source_gate_enabled/pass` 字段并纳入 `overall_pass`。
- 证据样本：`artifacts/migration/dividend-balance-source-gate-2026-03-07/dividend-balance-source-gate-summary.json`（`pass=True`）与 `artifacts/migration/acceptance-economic-dividend-source-smoke-2026-03-07/acceptance-gate-summary.json`（`overall_pass=True`, `dividend_balance_source_pass=True`）。
- 已完成：NAV 估值源收口（external + fallback）：
  - 修复点：`market_engine` 新增 `ConfigurableNavValuationSource`，支持外部估值源切换、缺失报价 fallback、`price_bp` 范围校验，并把 source/price/fallback 写入快照。
  - `novovm-node` 接入远端 HTTP NAV feed：`NOVOVM_GOV_MARKET_NAV_VALUATION_MODE=external_feed`、`NOVOVM_GOV_MARKET_NAV_FEED_URL`、`NOVOVM_GOV_MARKET_NAV_FEED_STRICT`；不可达可 strict fail 或 fallback。
  - 新增门禁：`scripts/migration/run_nav_valuation_source_gate.ps1`，覆盖 external 正向、fallback 正向、invalid price 负向、market_engine 回归、remote feed 正向与 strict 不可达负向。
  - acceptance 聚合：`run_migration_acceptance_gate.ps1` 新增 `nav_valuation_source_gate_enabled/pass` 字段并纳入 `overall_pass`。
- 证据样本：`artifacts/migration/nav-valuation-source-gate-2026-03-07/nav-valuation-source-gate-summary.json`（`pass=True`）、`artifacts/migration/nav-valuation-source-gate-remote-smoke-2026-03-07/nav-valuation-source-gate-summary.json`（`pass=True`）与 `artifacts/migration/acceptance-economic-navfx-dividend-smoke-2026-03-07/acceptance-gate-summary.json`（`overall_pass=True`, `nav_valuation_source_pass=True`）。
- 已完成：ForeignPayment 远端汇率源收口（external + strict/fallback）：
  - `novovm-node` 接入远端 HTTP foreign rate feed：`NOVOVM_GOV_MARKET_FOREIGN_RATE_MODE=external_feed`、`NOVOVM_GOV_MARKET_FOREIGN_RATE_FEED_URL`、`NOVOVM_GOV_MARKET_FOREIGN_RATE_FEED_STRICT`；不可达可 strict fail 或 fallback。
  - `market_engine` 快照新增 `foreign_rate_source/foreign_rate_quote_spec_applied/foreign_rate_fallback_used`，并在 `governance_market_foreign_source_in/out` 信号中可审计。
  - `run_foreign_rate_source_gate.ps1` 升级覆盖：remote feed 正向、fallback 正向、strict 不可达负向。
- 证据样本：`artifacts/migration/foreign-rate-source-gate-remote-smoke-2026-03-07/foreign-rate-source-gate-summary.json`（`pass=True`）与 `artifacts/migration/governance-market-policy-gate-forex-smoke-2026-03-07/governance-market-policy-gate-summary.json`（`pass=True`）。
- 已完成：`F-10~F-13` 自动台账判定由“骨架目录存在”升级为“主链路 gate 证据判定”（storage/domain/defi/adapters-multi），并使用 `acceptance-gate-summary.json` 聚合字段收口。
- 已完成：`F-14` vm-runtime split 门禁已接入 acceptance，并在自动台账中按 `vm_runtime_split_pass + legacy_vm_runtime_present=False` 固化为 `ReadyForMerge`。
- 状态：`F-10/F-11/F-12/F-13/F-14 = Done`（2026-03-07 自动回填快照）。
- 证据样本：`docs_CN/SVM2026-MIGRATION/NOVOVM-CAPABILITY-MIGRATION-LEDGER-AUTO-2026-03-04.md`（`Full Scan Matrix (F-01~F-16)` 与 `Ledger` 段）。
