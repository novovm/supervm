# NOVOVM MVP 能力与主网差距审计（对 ChatGPT 结论复核）- 2026-03-04

## 0. 审计范围与基线

- 审计对象：
  - `NOVOVM` 当前迁移态（以 2026-03-04 自动台账为准）
  - `SVM2026` 现有实现与路线图
- 复核目标：
  - A. 「已经具备的能力」是否成立
  - B. 「生产主网扩展项」是否仍缺
  - C. 「下一阶段自然演进」是否合理
- 关键证据：
  - `SUPERVM/docs_CN/SVM2026-MIGRATION/NOVOVM-CAPABILITY-MIGRATION-LEDGER-AUTO-2026-03-04.md`
  - `SUPERVM/docs_CN/SVM2026-MIGRATION/NOVOVM-FUNCTION-CATALOG-2026-03-03.md`
  - `SVM2026/supervm-node/src/main.rs`
  - `SVM2026/supervm-node-core/src/rpc/*`
  - `SVM2026/supervm-consensus/src/*`
  - `SVM2026/src/vm-runtime/src/economic_context.rs`
  - `SVM2026/ROADMAP.md`

---

## 1. 对 ChatGPT 结论的审计结论

## A. 「已经具备的能力」

| 子项 | 审计结论 | 说明 |
|---|---|---|
| 可跑交易/执行/出块/传播 | 成立（MVP 口径） | `D2/D3=Done`，`F-03~F-08=ReadyForMerge`；功能门禁含 `tx_codec/mempool/block/commit/network/consensus`。 |
| 可做标准链查询（`getBlock/getTransaction/getReceipt/getBalance`） | 成立（MVP + 读 RPC 服务口径） | `novovm-node` 已新增 `chain_query` + `rpc_server` 模式：提交后落盘 `query-state`，并通过 JSON-RPC 风格接口（`/`/`/rpc`）暴露四个查询方法；已纳入 `run_chain_query_rpc_gate.ps1` 门禁。 |
| 可发币/资产发行 | 成立（最小经济治理口径） | `NOVOVM` 已复用 `SVM2026 web30-core` 的 `MainnetTokenImpl`，并通过治理 `UpdateTokenEconomicsPolicy` 与 `TreasurySpend` 完成最小经济主链路门禁闭环。 |

**A 总结**：`交易闭环 + 最小查询闭环 + 读 RPC 服务化 + 最小经济治理闭环` 已成立；后续主要是“完整经济治理域”的生产化收口。

## B. 「生产主网还需要扩展项」

| 子项 | 审计结论 | 说明 |
|---|---|---|
| 完整共识（validator/stake/slash/view-change/fork choice） | 基本正确（主闭环已具备） | 已具备 `ValidatorSet + QC + 投票`，并新增 stake-weighted quorum、equivocation slash-evidence、slash execution、view-change、fork-choice；`slash execution` 已参数化为 `SlashPolicy(mode/threshold/min_active/cooldown_epochs)`，默认可由 `config/novovm-consensus-policy.json` 外置输入并通过 `slash_governance_gate + slash_policy_external_gate + unjail_cooldown_gate` 门禁；网络级 `view_sync/new_view` pacemaker 已从“消息闭环”升级为 `pacemaker_failover_gate`（leader 超时失效 -> 换主 -> 提交）硬门禁，后续主要是生产参数化与治理策略收口。 |
| 完整同步（header/fast/state sync） | 部分正确（仍缺） | 已新增 `header_sync_gate`（headers-first）与 `fast_state_sync_gate`（fast headers + state snapshot verify）并接入 acceptance gate，且均含篡改负向拒绝；`fast/state sync` 的生产多 peer/持久化恢复链路仍缺。 |
| DoS 防护（tx spam/peer flood/invalid block storm） | 部分正确（仍缺） | 已有 RPC ingress rate-limit（429 / `-32029`）门禁，且新增网络级 `peer-score/ban + invalid-block-storm` 门禁（`network_dos_gate`）；仍缺生产级持久化惩罚、跨节点信誉传播与灰度恢复策略。 |
| 经济参数（gas pricing/burn/inflation/treasury） | 部分正确（主链路已迁移，生产化策略待补） | `NOVOVM` 已迁入 `mint/burn/gas-service split/treasury spend`，并补齐经济跨模块主链路（预言机价格驱动/CDP 清算/NAV 结算/回购编排）门禁化；剩余缺口主要在长期运维与策略持久化。 |
| ZK runtime（F-15） | 正确（能力已就绪） | 最新能力快照已回填 `zk_runtime_ready=True`、`zkvm_prove=True`、`zkvm_verify=True`。 |

## C. 「下一阶段（自然演进）」

该方向判断**正确**，且与当前状态匹配：

| 阶段项 | 现状判定 |
|---|---|
| 钱包 / RPC / SDK | 部分完成（SVM2026 有局部 SDK/CLI/RPC，NOVOVM 已完成本地查询闭环 + 读 RPC 服务化门禁） |
| Genesis + Tokenomics | 部分完成（最小主链路完成，完整域待补） |
| Validator 网络 | 部分完成（有示例/骨架，不是生产验证者网络） |
| Testnet 启动 | 规划态/建议态为主，未见统一“已正式启动”证据闭环 |
| ZK Runtime | 受 AOEM runtime 就绪状态阻塞 |
| 性能调优 | 完成度高（SVM2026 侧有大量基准与优化报告） |

---

## 2. 进度看板（你可直接据此决策）

## 2.1 生产主网扩展项（B）进度

| 项目 | NOVOVM（2026-03-04） | SVM2026 原实现 | 进度判定 |
|---|---|---|---|
| 完整共识机制 | MVP 通过（~80%核验口径） | 已有验证者集合/QC + stake-weighted quorum + equivocation slash-evidence + slash execution + slash-policy(threshold/observe-only/min-active) + view-change + fork-choice + 网络级 view-sync/new-view pacemaker + failover（超时换主后继续出块提交）；剩余是生产参数化罚没/治理策略 | 部分完成 |
| 完整同步 | MVP 网络闭环通过 | 已补 headers-first + fast/state 最小门禁闭环；仍缺 fast/state sync 生产闭环 | 部分完成 |
| DoS 防护体系 | 已形成入口+网络最小门禁项 | RPC ingress 侧 rate-limit 已门禁化；网络侧 `peer-score/ban + invalid-block-storm` 已门禁化，生产级长期信誉治理仍待补 | 部分完成 |
| 经济参数闭环 | 主链路已完成，生产策略待补 | 已完成 `UpdateTokenEconomicsPolicy + TreasurySpend + UpdateMarketGovernancePolicy`，并新增跨模块编排门禁（oracle/liquidation/nav/buyback）；生产策略与长期运营治理仍在后续计划 | 部分完成 |
| ZK runtime ready | `False`（台账明确） | 原仓有 ZK 能力沉淀，但 NOVOVM 运行态受 AOEM 约束 | 阻塞中 |

**B 汇总**：`0 项完全完成 / 4 项部分完成 / 1 项阻塞`。

## 2.2 自然演进项（C）进度

| 项目 | 进度判定 | 备注 |
|---|---|---|
| 钱包/RPC/SDK | 部分完成 | 读查询 RPC 已服务化并接入门禁；钱包/SDK/写接口仍待推进。 |
| Genesis+Tokenomics | 部分完成 | 迁移闭环已形成（token economics + treasury spend + market governance orchestration），仍待补生产参数编排与发布运营策略。 |
| Validator 网络 | 部分完成 | 有 demo/harness，不等于生产网络。 |
| Testnet 启动 | 未完成 | 多为计划/建议部署测试网。 |
| ZK Runtime | 阻塞中 | 等 AOEM 1.0 后切 runtime-ready。 |
| 性能调优 | 完成度高 | 可作为迁移期基线优势。 |

**C 汇总**：`1 高完成 / 3 部分完成 / 1 未完成 / 1 阻塞`。

---

## 2.3 发布快照（2026-03-05，全量门禁）

- 快照脚本：`scripts/migration/run_release_snapshot.ps1`
- 一键全开 profile：`run_migration_acceptance_gate.ps1 -FullSnapshotProfile`（`profile_name=full_snapshot_v1`）
- 快照产物：
  - `artifacts/migration/release-snapshot-2026-03-05/release-snapshot.json`
  - `artifacts/migration/release-snapshot-2026-03-05/release-snapshot.md`
- 快照结论：
  - `overall_pass=True`
  - `enabled_gates` 全部为 `true`（chain_query_rpc / governance_rpc / header_sync / fast_state_sync / network_dos / pacemaker_failover / slash_governance / slash_policy_external / governance_hook / governance_execution / governance_param2 / governance_param3 / governance_negative / unjail_cooldown / adapter_stability）
  - `allowed_regression_pct=-5.0`
  - `key_results`：
    - `tps_p50.core/cpu_batch_stress=24607691.87`
    - `tps_p50.core/cpu_parity=5947527.35`
    - `rpc_pass/governance_pass/sync_pass/adapter_pass/dos_pass/consensus_pass=True`
- relfix 快照（同日回归）：
  - `artifacts/migration/release-snapshot-param3-smoke-relfix/release-snapshot.json`
  - `profile_name=full_snapshot_v1`，`overall_pass=True`，`governance_param3_pass=True`，`adapter_stability_pass=True`
- GA 经济治理快照（2026-03-06）：
  - `artifacts/migration/release-snapshot-ga-smoke-treasury/release-snapshot.json`
  - `profile_name=full_snapshot_ga_v1`，`overall_pass=True`
  - `governance_access_policy_pass=True`，`governance_token_economics_pass=True`，`governance_treasury_spend_pass=True`
- GA RC 候选快照（2026-03-06）：
  - `artifacts/migration/release-candidate-novovm-rc-2026-03-06-ga-v1-retryfix/rc-candidate.json`
  - `status=ReadyForMerge/SnapshotGreen`
  - `governance_access_policy_pass=True`，`governance_token_economics_pass=True`，`governance_treasury_spend_pass=True`
  - `adapter_stability` 已加入定向单次重试稳态修复（registry negative hash-mismatch 抖动场景）
- GA 正式快照（2026-03-06）：
  - `artifacts/migration/release-snapshot-ga-2026-03-06-051653/release-snapshot.json`
  - `profile_name=full_snapshot_ga_v1`，`overall_pass=True`
  - `governance_market_policy_engine_pass=True`，`governance_market_policy_treasury_pass=True`
- GA 正式 RC（2026-03-06）：
  - `artifacts/migration/release-candidate-novovm-rc-2026-03-06-ga-v1/rc-candidate.json`
  - `status=ReadyForMerge/SnapshotGreen`
  - `commit_hash=823a5880e104c96d03e2ab4a8473c9f620ae6413`
- GA orchfix 复核快照（2026-03-06）：
  - `artifacts/migration/release-candidate-novovm-rc-2026-03-06-ga-orchfix/snapshot/release-snapshot.json`
  - `profile_name=full_snapshot_ga_v1`，`overall_pass=True`
  - `governance_market_policy_orchestration_pass=True`
- GA orchfix 复核 RC（2026-03-06）：
  - `artifacts/migration/release-candidate-novovm-rc-2026-03-06-ga-orchfix/rc-candidate.json`
  - `status=ReadyForMerge/SnapshotGreen`
  - `commit_hash=bac3763192258d5fcb89fc129e2b675d56dbb317`
- GA 多源签名回归快照（2026-03-07）：
  - `artifacts/migration/release-snapshot-ga-multisig-2026-03-07/release-snapshot.json`
  - `profile_name=full_snapshot_ga_v1`，`overall_pass=True`
  - `governance_market_policy_dividend_pass=True`，`governance_market_policy_foreign_payment_pass=True`
  - `economic_pass=True`，`economic_infra_dedicated_pass=True`，`market_engine_treasury_negative_pass=True`，`foreign_rate_source_pass=True`，`nav_valuation_source_pass=True`，`dividend_balance_source_pass=True`
- GA 多源签名正式 RC（2026-03-07）：
  - `artifacts/migration/release-candidate-novovm-rc-2026-03-07-ga-multisig/rc-candidate.json`
  - `status=ReadyForMerge/SnapshotGreen`
  - `snapshot_profile=full_snapshot_ga_v1`，`snapshot_overall_pass=True`
  - `commit_hash=b72fdd987cf1c61163830bda4d46e4dd34020ecf`
- 治理审计持久化回归快照（2026-03-06）：
  - `artifacts/migration/release-snapshot-audit-persist-smoke/release-snapshot.json`
  - `key_results.governance_rpc_audit_persist_pass=True`
- 治理审计持久化回归 RC（2026-03-06）：
  - `artifacts/migration/release-candidate-novovm-rc-2026-03-06-governance-audit-persist-smoke/rc-candidate.json`
  - `status=ReadyForMerge/SnapshotGreen`
  - `governance_rpc_audit_persist_pass=True`
- 治理签名算法 staged 回归快照（2026-03-06）：
  - `artifacts/migration/release-snapshot-signature-scheme-smoke/release-snapshot.json`
  - `key_results.governance_rpc_signature_scheme_reject_pass=True`
- 治理签名算法 staged 回归 RC（2026-03-06）：
  - `artifacts/migration/release-candidate-novovm-rc-2026-03-06-signature-scheme-smoke/rc-candidate.json`
  - `status=ReadyForMerge/SnapshotGreen`
  - `governance_rpc_signature_scheme_reject_pass=True`
- 治理链审计 root 区块路径锚定回归快照（2026-03-06）：
  - `artifacts/migration/release-snapshot-governance-chain-audit-root-smoke/release-snapshot.json`
  - `key_results.governance_chain_audit_root_parity_pass=True`
- 治理链审计 root 区块路径锚定回归 RC（2026-03-06）：
  - `artifacts/migration/release-candidate-novovm-rc-2026-03-06-governance-chain-audit-root-anchor-smoke/rc-candidate.json`
  - `status=ReadyForMerge/SnapshotGreen`
  - `governance_chain_audit_root_parity_pass=True`

---

## 3. 关键证据摘录（便于快速复核）

- NOVOVM 域级完成：
  - `D2=Done`、`D3=Done`，且 `F-03~F-08=ReadyForMerge`（自动台账）。
- NOVOVM 读 RPC 服务化门禁：
  - `novovm-node` 支持 `NOVOVM_NODE_MODE=rpc_server`；
  - `scripts/migration/run_chain_query_rpc_gate.ps1`（4 个正向查询 + 1 个负向方法 + `rate_limit_signal`(429/`-32029`)）已接入 acceptance gate（并已修复 PowerShell 兼容与 BOM 读取问题）。
- NOVOVM 同步门禁：
  - `novovm-node` 支持 `NOVOVM_NODE_MODE=header_sync_probe`；
  - `novovm-node` 支持 `NOVOVM_NODE_MODE=fast_state_sync_probe`；
  - `scripts/migration/run_header_sync_gate.ps1`（`header_sync_signal` + `header_sync_negative_signal`）与 `scripts/migration/run_fast_state_sync_gate.ps1`（`fast_state_sync_signal` + `fast_state_sync_negative_signal`）均已接入 acceptance gate。
- NOVOVM 抗 DoS 门禁：
  - `novovm-node` 支持 `NOVOVM_NODE_MODE=network_dos_probe`；
  - `scripts/migration/run_network_dos_gate.ps1`（`network_dos_signal`）已接入 acceptance gate，覆盖 `peer-score/ban + invalid-block-storm` 最小闭环。
- NOVOVM pacemaker failover 门禁：
  - `novovm-node` 支持 `NOVOVM_NODE_MODE=pacemaker_failover_probe`；
  - `scripts/migration/run_pacemaker_failover_gate.ps1` 已接入 acceptance gate，覆盖 `leader timeout -> view-change -> new leader commit` 活性闭环。
- NOVOVM 共识主网化增量门禁：
  - `novovm-consensus` 已启用 stake-weighted quorum 收敛（不再按票数个数收敛）；
  - `QC` 校验新增声明权重防伪（`total_weight` 与观测权重不一致会拒绝）；
  - 同高度双签（equivocation）会返回 `EquivocationDetected` 并落 slash evidence；
  - `view-change`（超时换主）与 `fork-choice`（高度/权重优先）已接入协议层与门禁；
  - `consensus_negative_smoke` 的 `pass` 已绑定上述能力（`weighted_quorum + equivocation + slash_execution + slash_threshold + slash_observe_only + view_change + fork_choice`）。
- NOVOVM Slash 治理门禁：
  - `scripts/migration/run_slash_governance_gate.ps1` 已接入 acceptance gate；
  - 覆盖 `SlashPolicy(mode=enforce/observe_only, threshold, min_active_validators)` 的行为断言。
- NOVOVM Slash 策略外置化门禁：
  - `config/novovm-consensus-policy.json` 作为默认外置入口；
  - `novovm-node` 支持 `NOVOVM_NODE_MODE=slash_policy_probe`，输出 `slash_policy_in` 与 `slash_policy_probe_out`；
  - `scripts/migration/run_slash_policy_external_gate.ps1` 已接入 acceptance gate，覆盖正向注入与负向 `policy_invalid/policy_parse_failed` 断言。
- NOVOVM Unjail/Cooldown 门禁：
  - `novovm-consensus` 已接入 `cooldown_epochs` 自动解禁窗口（`SlashExecution` 含 `jailed_until_epoch/cooldown_epochs`）；
  - `scripts/migration/run_unjail_cooldown_gate.ps1` 已接入 acceptance gate，覆盖“未到期拒绝 + 到期自动解禁”。
- NOVOVM 未完与阻塞：
  - 批次表：`E | RPC/CLI/DevEx | NotStarted`
- SVM2026 共识现状：
  - `supervm-node` 注释明确为 `Single-validator BFT engine for smoke / wiring demo`
  - `supervm-consensus` 有 `ValidatorSet/QC`，但未检出 `slash/view-change/fork_choice` 主链路实现。
- SVM2026 RPC 暴露范围：
  - `server.rs` 注册 `governance + metrics`；
  - 未检出 `getBlock/getTransaction/getReceipt/getBalance`。
- SVM2026 经济模块：
  - `economic_context.rs` 包含 `charge_gas`、`burn/treasury` 路由与测试断言（20/30/50 分配）。
  - `financial_metrics.rs` 仍存在 `TODO: Query from treasury`。
- SVM2026 路线图中的未完信号：
  - `L3.3 开发者工具 v0 = 0%`
  - `SDK进行中`
  - `headers-first + fast/state` 已新增最小门禁闭环，生产级多 peer/持久化恢复语义待办项仍在。

---

## 4. 建议的下一步（按优先级）

1. （已完成，2026-03-05）`NOVOVM Batch E` 第 2 步（RPC 服务化暴露）  
结果：`getBlock/getTransaction/getReceipt/getBalance` 已形成服务接口并纳入门禁（`chain_query_rpc_pass`）。

2. 共识主网化收口（剩余：参数化罚没与治理策略）  
目标：在已完成 stake/quorum/slash-evidence/slash-execution/view-change/fork-choice + 网络级 pacemaker 门禁基础上，补齐生产参数化与治理闭环。

3. 同步与抗 DoS 并行推进  
目标：在已完成 `headers-first + fast/state` 最小门禁基础上，补齐生产级 `fast/state sync`（多 peer + 持久化恢复）与 `rate-limit/peer-score/ban`，避免只在理想网络可用。

4. 经济模块迁移收口（基于 SVM2026 已有实现）  
目标：把 SVM2026 的经济参数能力搬到 NOVOVM 主链路并加门禁。

5. ZK runtime 继续按你当前策略冻结  
目标：等待 AOEM 1.0 后再切 `runtime-ready`，避免反复返工。
