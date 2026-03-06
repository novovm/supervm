# NOVOVM 共识可发布规则深度审计与使用规则说明（2026-03-05）

## 0. 审计目标与结论速览

本审计回答 3 个问题：

1. 当前 NOVOVM 已迁移完成并可发布的共识规则有哪些。
2. 哪些规则属于 SVM2026 已有“可继承发布口径”但尚未迁移到 NOVOVM 主链路。
3. 发币、验证、交易、投票的使用规则应如何执行，哪些可立即用，哪些必须先迁移。

结论（可直接用于决策）：

- `可立即发布（MVP 范围）`：
  - 共识主链路：`weighted quorum + QC anti-tamper + equivocation/slash + view-change + fork-choice + slash policy + unjail cooldown`。
  - 治理最小闭环（受限范围）：`UpdateSlashPolicy`、`UpdateMempoolFeeFloor`、`UpdateNetworkDosPolicy`、`UpdateTokenEconomicsPolicy`、`UpdateMarketGovernancePolicy`、`TreasurySpend`、`UpdateGovernanceAccessPolicy`、`UpdateGovernanceCouncilPolicy` 的 `proposal -> vote(signature) -> quorum -> apply`。
  - 治理防重放域分隔：治理签名消息绑定 `proposal_id + proposal_height + proposal_digest + support`。
  - 经济主链路（最小可发布）：`mint/burn + gas/service fee split + treasury spend(governance)`。
  - 交易闭环：`tx wire -> mempool admission -> tx metadata -> block/commit output`。
  - 读查询：`getBlock/getTransaction/getReceipt/getBalance` + RPC rate limit。
  - 网络与活性：`headers-first`、`fast/state sync`（含负向篡改拒绝）、`peer-score/ban`、`pacemaker failover`。
- `继承可发布但尚未完全迁移到 NOVOVM 主链路`：
  - 完整经济域跨模块执行联动（预言机/清算引擎/NAV 实时结算与回购执行策略）仍未接入 NOVOVM 主链路。
  - 抗量子签名（ML-DSA）与链上治理权限模型。
- `治理入口当前状态`：
  - 已具备受限治理执行面：`UpdateSlashPolicy`、`UpdateMempoolFeeFloor`、`UpdateNetworkDosPolicy`、`UpdateTokenEconomicsPolicy`、`UpdateMarketGovernancePolicy`、`TreasurySpend`、`UpdateGovernanceAccessPolicy`、`UpdateGovernanceCouncilPolicy` 可经签名投票 + quorum 生效。
- `发布边界`：
  - 当前可发布口径是“`MVP+（共识+交易+读查询+最小经济治理）`”，不是“完整主网经济治理版”。
- `发布策略（当前执行）`：
  - RC 仅内部里程碑，不对外发布；对外仅 GA。
  - GA 前置条件：最小经济治理门禁（`governance_token_economics`、`governance_treasury_spend`）纳入 acceptance 全量通过。

---

## 1. 证据基线

### 1.1 NOVOVM（SUPERVM）代码与门禁

- 共识实现：
  - `crates/novovm-consensus/src/types.rs`
  - `crates/novovm-consensus/src/protocol.rs`
  - `crates/novovm-consensus/src/quorum_cert.rs`
  - `crates/novovm-consensus/src/bft_engine.rs`
- 节点实现：
  - `crates/novovm-node/src/main.rs`
  - `config/novovm-consensus-policy.json`
- 门禁聚合：
  - `scripts/migration/run_migration_acceptance_gate.ps1`
- 验收产物（本轮基线）：
  - `artifacts/migration/acceptance-gate-unjail-full/acceptance-gate-summary.json`
  - `artifacts/migration/acceptance-gate-unjail-full/functional/functional-consistency.json`
  - `artifacts/migration/acceptance-gate-unjail-full/*-gate/*-summary.json`

### 1.2 迁移状态台账（NOVOVM）

- `docs_CN/SVM2026-MIGRATION/NOVOVM-CAPABILITY-MIGRATION-LEDGER-2026-03-03.md`
- `docs_CN/SVM2026-MIGRATION/NOVOVM-CAPABILITY-MIGRATION-LEDGER-AUTO-2026-03-04.md`
- `docs_CN/SVM2026-MIGRATION/NOVOVM-MVP-MAINNET-GAP-AUDIT-2026-03-04.md`

### 1.3 可继承来源（SVM2026）

- 发币与经济：
  - `contracts/web30/core/src/mainnet_token.rs`
  - `contracts/web30/core/src/mainnet_token_impl.rs`
  - `src/vm-runtime/src/economic_context.rs`
- 治理投票：
  - `contracts/web30/core/src/governance.rs`
  - `src/vm-runtime/src/governance.rs`
  - `supervm-node-core/src/rpc/governance_api.rs`

---

## 2. 迁移完成且可发布的规则（NOVOVM 当前）

以下规则满足“代码存在 + 门禁证据通过”。

## 2.1 共识规则（C 系列）

| 规则ID | 规则 | 实现证据 | 门禁证据 | 结论 |
|---|---|---|---|---|
| C-01 | 验证者集合与加权法定人数：`quorum = ceil(2/3 * total_weight)` | `types.rs` (`ValidatorSet::quorum_size`) | `consensus_negative_ext.weighted_quorum=true` | 已迁移可发布 |
| C-02 | 提案合法性检查：`height/epoch/view/leader/prev_qc` 必须一致 | `protocol.rs` (`validate_proposal`) | `wrong_epoch=true` + fail path | 已迁移可发布 |
| C-03 | 投票规则：签名校验、同提案唯一投票；同高双签触发 equivocation | `quorum_cert.rs` + `protocol.rs` (`collect_vote`) | `invalid_signature=true`, `duplicate_vote=true`, `equivocation=true` | 已迁移可发布 |
| C-04 | QC 防篡改：重算观测权重，不信任声明 `total_weight` | `quorum_cert.rs` (`validate_votes_and_weight`) | `weighted_quorum=true` 且负向 tamper 受测 | 已迁移可发布 |
| C-05 | 罚没策略：`mode/enforce|observe_only` + `threshold` + `min_active_validators` | `types.rs` (`SlashPolicy`) + `protocol.rs` (`execute_slash`) | `slash_threshold=true`, `slash_observe_only=true`, `slash_execution=true` | 已迁移可发布 |
| C-06 | 自动解禁：`cooldown_epochs` 到期自动 unjail | `protocol.rs` (`jailed_until_epoch`, `validator_jailed_until_epoch`) | `unjail_out: ... unjailed=true` | 已迁移可发布 |
| C-07 | 活性恢复：`view-change` | `protocol.rs` (`trigger_view_change`) | `view_change=true` + failover gate pass | 已迁移可发布 |
| C-08 | 分叉选择：`高度优先 -> 权重优先 -> hash` | `protocol.rs` (`select_fork_choice`) | `fork_choice=true` | 已迁移可发布 |
| C-09 | 治理挂点预留：`GovernanceOp::UpdateSlashPolicy`（staged-only） | `types.rs` + `protocol.rs` (`stage_governance_op`) + `main.rs` (`governance_hook_probe`) | `governance_hook_gate.pass=true` | 已迁移（占位，不执行） |
| C-10 | 治理最小执行：`submit proposal + governance vote(signature) + quorum apply`（仅 `UpdateSlashPolicy`） | `types.rs` (`GovernanceProposal/GovernanceVote`) + `protocol.rs` (`submit_governance_proposal`, `execute_governance_proposal`) + `main.rs` (`governance_execute_probe`) | `governance_execution_gate.pass=true` | 已迁移（受限范围） |
| C-11 | 治理负向规则：非验证者提案拒绝、签名错误拒绝、重复投票拒绝、票数不足拒绝、重复执行拒绝 + 投票域分隔（height/digest mismatch 拒绝） | `protocol.rs` (`submit_governance_proposal`, `execute_governance_proposal`) + `main.rs` (`governance_negative_probe`) | `governance_negative_gate.pass=true` | 已迁移（受限范围） |
| C-12 | 第二类治理参数：`UpdateMempoolFeeFloor` 经治理投票生效 | `types.rs` (`GovernanceOp::UpdateMempoolFeeFloor`) + `protocol.rs` (`governance_mempool_fee_floor`) + `main.rs` (`governance_param2_probe`) | `governance_param2_gate.pass=true` | 已迁移（受限范围） |
| C-13 | 治理 RPC 执行面：`submit/sign/vote/execute/getProposal/listProposals/listAuditEvents/getPolicy`（受限执行面） | `main.rs` (`run_governance_rpc`, `run_chain_query_rpc_server_mode`) + `run_governance_rpc_gate.ps1` | `governance_rpc_gate.pass=true` | 已迁移（受限范围） |
| C-14 | 治理权限与审计：`proposer/executor allowlist` + 审计事件流（含 reject 事件）+ 审计持久化索引（`NOVOVM_GOVERNANCE_AUDIT_DB`） | `main.rs` (`NOVOVM_GOVERNANCE_*_ALLOWLIST`, `GovernanceRpcAuditEvent`, `GovernanceRpcAuditStore`) + `run_governance_rpc_gate.ps1` | `unauthorized_submit_reject_ok=true`, `audit_ok=true`, `audit_persist_ok=true` | 已迁移（受限范围） |
| C-15 | 第三类治理参数：`UpdateNetworkDosPolicy` 经治理投票生效 | `types.rs` (`GovernanceOp::UpdateNetworkDosPolicy`, `NetworkDosPolicy`) + `protocol.rs` (`governance_network_dos_policy`) + `main.rs` (`governance_param3_probe`) | `governance_param3_gate.pass=true` | 已迁移（受限范围） |
| C-16 | RPC 暴露安全铁律：public/gov 端口分离、`gov rpc` 默认关闭、public 不暴露 `governance_*`、非回环治理端口需 allowlist | `main.rs` (`run_chain_query_rpc_server_mode`, `NOVOVM_ENABLE_GOV_RPC`, `NOVOVM_GOV_RPC_BIND`, `NOVOVM_GOV_RPC_ALLOWLIST`) + `run_rpc_exposure_gate.ps1` | `rpc_exposure_gate.pass=true` | 已迁移（可发布安全默认） |
| C-17 | 国库治理执行：`TreasurySpend` 经治理投票生效，支持超额支出拒绝 | `types.rs` (`GovernanceOp::TreasurySpend`) + `protocol.rs` (`spend_treasury_tokens`) + `main.rs` (`governance_treasury_spend_probe`) | `governance_treasury_spend_gate.pass=true` | 已迁移（受限范围） |
| C-18 | 链上治理访问策略：`proposer/executor committee + threshold + timelock`（治理权限由 `GovernanceOp::UpdateGovernanceAccessPolicy` 下发） | `types.rs` (`GovernanceAccessPolicy`, `GovernanceOp::UpdateGovernanceAccessPolicy`) + `protocol.rs` (`submit/execute *_with_approvals`) + `main.rs` (`governance_access_policy_probe`) | `governance_access_policy_gate.pass=true` | 已迁移（受限范围，链上权限模型初版） |
| C-19 | 九席位治理权重策略：`Founder/TopHolder(0-4)/Team(0-1)/Independent`，按提案类别阈值（`Parameter/Treasury/Protocol/Emergency`）执行治理 | `types.rs` (`GovernanceCouncilPolicy`, `GovernanceCouncilSeat`, `GovernanceOp::UpdateGovernanceCouncilPolicy`) + `protocol.rs` (`execute_governance_proposal_with_executor_approvals`) + `main.rs` (`governance_council_policy_probe`) | `governance_council_policy_gate.pass=true` | 已迁移（受限范围，I-GOV-01 主链路） |
| C-20 | 经济治理参数族热更新：`AMM/CDP/Bond/Reserve/NAV/Buyback` 统一由 `UpdateMarketGovernancePolicy` 治理下发，并输出 `market_engine + treasury` 执行证据 | `types.rs` (`MarketGovernancePolicy`, `GovernanceOp::UpdateMarketGovernancePolicy`) + `protocol.rs` (`set/governance_market_policy`) + `market_engine.rs` (`Web30MarketEngine`) + `main.rs` (`governance_market_policy_probe`) | `governance_market_policy_gate.pass=true` + `engine_output_pass=true` + `treasury_output_pass=true` | 已迁移（受限范围，I-GOV-02 主链路） |
| C-21 | 治理签名算法 staged 抽象：RPC 层 `signature_scheme` + 共识执行层 `governance_vote_verifier` 钩子均已接线；`novovm-node` 启动支持 `NOVOVM_GOVERNANCE_VOTE_VERIFIER`（默认 `ed25519`）；`mldsa87` 明确拒绝并落审计（不启 ML-DSA 验签执行） | `main.rs` (`parse_governance_signature_scheme`, `configure_governance_vote_verifier`, `ensure_governance_signature_scheme_supported`) + `governance_verifier.rs` (`GovernanceVoteVerifier`, `GovernanceVoteVerifierScheme`, `build_governance_vote_verifier`) + `protocol.rs` (`governance_vote_verifier_scheme`) + `bft_engine.rs` (`set_governance_vote_verifier_by_scheme`, `governance_vote_verifier_scheme`, `governance_signature_scheme_supported`) + `run_governance_rpc_gate.ps1` | `sign_unsupported_scheme_reject_ok=true` + `vote_verifier_startup_ok=true` + `vote_verifier_staged_reject_ok=true` + `governance_rpc_signature_scheme_reject_pass=true` + `governance_rpc_vote_verifier_startup_pass=true` + `governance_rpc_vote_verifier_staged_reject_pass=true` + `test_governance_execute_uses_configurable_vote_verifier_hook` + `build_governance_vote_verifier_rejects_mldsa87_staged_only` | 已迁移（staged-only，I-GOV-04 预接线） |

### C 系列关键观察

- `SlashPolicy` 已支持外置文件加载，默认路径 `config/novovm-consensus-policy.json`。
- 当前默认策略：
  - `mode=enforce`
  - `equivocation_threshold=1`
  - `min_active_validators=2`
  - `cooldown_epochs=0`（语义为不自动解禁；使用外置策略可开启窗口解禁）
- 相关 gate 已入总验收：`slash_governance`, `slash_policy_external`, `unjail_cooldown`。
- 治理挂点 gate：`run_governance_hook_gate.ps1`，验证 `staged=true`、`executed=false`、`reason_code=governance_not_enabled`。
- 治理执行 gate：`run_governance_execution_gate.ps1`，验证 `executed=true`、`reason_code=ok`、`policy_applied=true`。
- 治理参数扩展 gate：`run_governance_param2_gate.ps1`，验证 `UpdateMempoolFeeFloor` 的提案/投票/生效闭环。
- 治理参数扩展 gate：`run_governance_param3_gate.ps1`，验证 `UpdateNetworkDosPolicy` 的提案/投票/生效闭环。
- 治理参数扩展 gate：`run_governance_market_policy_gate.ps1`，验证 `UpdateMarketGovernancePolicy`（AMM/CDP/Bond/Reserve/NAV/Buyback）的提案/投票/生效闭环。
- 治理席位权重 gate：`run_governance_council_policy_gate.ps1`，验证 `UpdateGovernanceCouncilPolicy` 的提案/投票/生效闭环，以及 `Parameter/ProtocolUpgrade` 分级阈值拒绝与通过路径。
- 治理权限模型 gate：`run_governance_access_policy_gate.ps1`，验证 `committee/threshold/timelock` 正向与负向闭环（提案阈值不足拒绝、未到 timelock 拒绝、执行阈值不足拒绝）。
- 治理参数扩展 gate：`run_governance_token_economics_gate.ps1`，验证 `UpdateTokenEconomicsPolicy` + `mint/burn/fee split` 会计闭环。
- 治理执行 gate：`run_governance_treasury_spend_gate.ps1`，验证 `TreasurySpend` 的提案/投票/生效闭环与 `overspend` 拒绝。
- 治理负向 gate：`run_governance_negative_gate.ps1`，验证 `unauthorized_submit/invalid_signature/duplicate_vote/insufficient_votes/replay_execute` 全部拒绝。
- 投票域分隔校验：执行时强校验 `proposal_height/proposal_digest` 与提案一致，不一致直接拒绝。
- 治理验签执行钩子：`novovm-consensus` 已提供 `GovernanceVoteVerifier`（默认 `ed25519`），治理执行路径改为通过 verifier 调用，后续 ML-DSA 只需注入实现，不需要重写执行主链路。
- 启动配置入口：`novovm-node` 支持 `NOVOVM_GOVERNANCE_VOTE_VERIFIER`（默认 `ed25519`）；若配置为 `mldsa87` 当前会在启动阶段拒绝（staged-only 保护），避免误以为已启用 ML-DSA 验签。
- 结构下沉完成：`scheme parse + verifier factory + staged reject` 已下沉到 `novovm-consensus::governance_verifier`；节点层只保留配置读取与调用 `BFTEngine::set_governance_vote_verifier_by_scheme`。
- 继续下沉完成：`governance_sign/governance_vote` 对 `signature_scheme` 的支持判定已改为调用共识层 `BFTEngine::governance_signature_scheme_supported`（并通过 `governance_vote_verifier_scheme` 返回 active scheme），节点层不再硬编码 `ed25519`。
- CI 强门禁：`.github/workflows/ci.yml` 新增 `governance_rpc_gate` job，要求 `vote_verifier_startup_ok=true` 且 `vote_verifier_staged_reject_ok=true`，否则 PR 直接失败。
- 分支保护自动化脚本：`scripts/migration/set_branch_protection_required_checks.ps1`（将 `Rust checks` + `Governance RPC gate (vote verifier)` 设为 `main` 的 required checks，支持 `-DryRun` 预览）。
- 治理 RPC gate：`run_governance_rpc_gate.ps1`，验证 `submit -> sign -> vote -> execute -> getPolicy` 正向闭环、`unauthorized proposer` 拒绝、重复投票拒绝、`listAuditEvents` 审计可观测、`signature_scheme=mldsa87` 拒绝（staged-only）；已接入 acceptance gate 的 `governance_rpc_pass`。
- 新增证据（下沉后回归）：`artifacts/migration/governance-rpc-gate-downsink-scheme-smoke/governance-rpc-gate-summary.json`（`pass=true`, `sign_unsupported_scheme_reject_ok=true`）。
- RPC 暴露 gate：`run_rpc_exposure_gate.ps1`，验证默认安全态（public 拒绝 `governance_*` + gov 端口关闭）与受控开启态（gov 本地端口可用、public 仍拒绝）。
- 治理权限入口（RPC 进程级）：`NOVOVM_GOVERNANCE_PROPOSER_ALLOWLIST`、`NOVOVM_GOVERNANCE_EXECUTOR_ALLOWLIST`。
- 治理权限入口（链上策略初版）：`NOVOVM_GOV_ACCESS_PROPOSER_COMMITTEE`、`NOVOVM_GOV_ACCESS_PROPOSER_THRESHOLD`、`NOVOVM_GOV_ACCESS_EXECUTOR_COMMITTEE`、`NOVOVM_GOV_ACCESS_EXECUTOR_THRESHOLD`、`NOVOVM_GOV_ACCESS_TIMELOCK_EPOCHS`。
- 治理 RPC 安全入口：`NOVOVM_ENABLE_PUBLIC_RPC`、`NOVOVM_ENABLE_GOV_RPC`、`NOVOVM_PUBLIC_RPC_BIND`、`NOVOVM_GOV_RPC_BIND`、`NOVOVM_GOV_RPC_ALLOWLIST`。

## 2.2 交易与出块规则（T 系列）

| 规则ID | 规则 | 实现证据 | 门禁证据 | 结论 |
|---|---|---|---|---|
| T-01 | 交易编解码闭环：`novovm_local_tx_wire_v1` | `main.rs` (`roundtrip_local_tx_codec_v1`) | `tx_codec_signal.pass=true` | 已迁移可发布 |
| T-02 | mempool 准入：`fee_floor` + 签名正确 + nonce 连续 | `main.rs` (`admit_mempool_basic`) | `mempool_admission_signal.pass=true` | 已迁移可发布 |
| T-03 | 交易元数据强校验：`fee>0`、签名、nonce 序列 | `main.rs` (`validate_and_summarize_txs`) | `tx_metadata_signal.pass=true` | 已迁移可发布 |
| T-04 | 执行到共识闭环观测：`block_output/commit_output` | `main.rs`（Batch A 闭环） | `block_output_signal.pass=true`, `commit_output_signal.pass=true` | 已迁移可发布（MVP） |

### T 系列关键观察

- 当前 nonce 规则是“按 account 从 0 开始连续”，适配 demo/migration 基线；生产环境建议增加持久化 nonce 读取与重放保护。
- `batch_a_closure` 在功能报告中仍是观测项，不是硬门禁项（报告 notes 已注明）。

## 2.3 查询、同步、网络防护规则（N/Q 系列）

| 规则ID | 规则 | 实现证据 | 门禁证据 | 结论 |
|---|---|---|---|---|
| Q-01 | 读查询方法：`getBlock/getTransaction/getReceipt/getBalance` | `main.rs` (`run_chain_query`) | `chain-query-rpc-gate.pass=true` | 已迁移可发布 |
| Q-02 | 未知方法拒绝：返回 `-32602` | `main.rs` default branch | gate 请求 `getUnknown` 命中 | 已迁移可发布 |
| Q-03 | RPC 限流：429 + `-32029` | `main.rs` (`is_rate_limited`) | `rate_limit_signal.pass=true` | 已迁移可发布 |
| N-01 | headers-first 同步 + 篡改父哈希拒绝 | `NOVOVM_NODE_MODE=header_sync_probe` | `header_sync_pass=true` + negative pass | 已迁移可发布 |
| N-02 | fast/state sync + snapshot 篡改拒绝 | `NOVOVM_NODE_MODE=fast_state_sync_probe` | `fast_state_sync_pass=true` + negative pass | 已迁移可发布 |
| N-03 | DoS 防护：peer-score/ban + invalid storm reject | `NOVOVM_NODE_MODE=network_dos_probe` | `network_dos_pass=true` | 已迁移可发布 |
| N-04 | 网络级 pacemaker failover | `NOVOVM_NODE_MODE=pacemaker_failover_probe` | `pacemaker_failover_pass=true` | 已迁移可发布 |

---

## 3. 继承规则迁移状态（SVM2026 来源）

本节区分“已进入 NOVOVM 主链路”的继承规则与“仍待迁移”的继承规则。

## 3.1 发币/经济规则（I-TOKEN）

| 规则ID | 规则 | 来源 | 当前 NOVOVM 状态 |
|---|---|---|---|
| I-TOKEN-01 | Token trait 定义 mint/burn/fee-routing 标准接口 | `contracts/web30/core/src/mainnet_token.rs` | 已迁入主链路（`Web30TokenRuntime` 复用） |
| I-TOKEN-02 | mint 约束：`amount>0`、不超过 `locked_supply`、不突破 `max_supply` | `mainnet_token_impl.rs` (`mint`) | 已迁入主链路（门禁通过） |
| I-TOKEN-03 | burn 约束：先扣余额再销毁总量 | `mainnet_token_impl.rs` (`burn`) | 已迁入主链路（门禁通过） |
| I-TOKEN-04 | Gas 费路由（示例 20/30/50）与国库入账 | `mainnet_token_impl.rs` (`on_gas_fee_paid`) | 已迁入主链路（门禁通过） |
| I-TOKEN-05 | Service fee 路由（provider/treasury/burn） | `mainnet_token_impl.rs` (`on_service_fee_paid`) | 已迁入主链路（门禁通过） |
| I-TOKEN-06 | 国库治理支出（`TreasurySpend`）与超额拒绝 | `mainnet_token.rs::transfer` + `protocol.rs::spend_treasury_tokens` | 已迁入受限治理主链路（门禁通过） |

补充：NOVOVM 当前已完成 `I-TOKEN` 最小可发布闭环，并已迁入经济治理参数族（AMM/CDP/Bond/Reserve/NAV/Buyback）热更新入口；但完整跨模块执行联动（清算/预言机/NAV 实时结算）仍待后续迁移。

## 3.2 治理与投票规则（I-GOV）

| 规则ID | 规则 | 来源 | 当前 NOVOVM 状态 |
|---|---|---|---|
| I-GOV-01 | 九席位加权治理模型与提案阈值 | `contracts/web30/core/src/governance.rs` | 已迁入受限主链路：`UpdateGovernanceCouncilPolicy` + 九席位固定结构 + 分类阈值（Parameter/Treasury/Protocol/Emergency） |
| I-GOV-02 | 参数治理热更新（AMM/CDP/Bond/Gov） | `src/vm-runtime/src/governance.rs` | 已迁入受限主链路：`UpdateMarketGovernancePolicy` 覆盖 `AMM/CDP/Bond/Reserve/NAV/Buyback` 参数治理；`Slash/Mempool/NetworkDos/TokenEconomics/Treasury/AccessPolicy/CouncilPolicy` 亦已可治理 |
| I-GOV-03 | RPC 治理接口：submit/vote/sign/list | `supervm-node-core/src/rpc/governance_api.rs` | NOVOVM 已接入受限最小面（`submit/sign/vote/execute/get/list/getPolicy/listAuditEvents`），并补齐链上权限模型初版（committee/threshold/timelock）；进程级 allowlist 仍可作为运维防线 |
| I-GOV-04 | 投票/签名消息校验与抗量子签名验证（ML-DSA） | 同上 | NOVOVM staged-only：已接入双层抽象（RPC `signature_scheme` + 共识执行 `governance_vote_verifier` hook），当前仅 `ed25519` 启用；`mldsa87` 明确拒绝并写入审计，ML-DSA 验签执行链路未启用 |

注：NOVOVM 已具备治理挂点占位、最小执行闭环、九席位权重阈值模型、第二/三类参数扩展、经济治理参数族扩展（I-GOV-02）与受限 RPC 执行面；ML-DSA 当前为 staged-only（算法标识接线与拒绝门禁已完成，验签执行链路未启用）；完整跨模块经济执行联动仍未接入主链路。

---

## 4. 使用规则说明（发币、验证、交易、投票）

## 4.1 交易规则（当前可执行）

### 规则

1. 交易必须满足：
   - `fee >= NOVOVM_MEMPOOL_FEE_FLOOR`（默认 1）
   - 签名与交易字段一致（本地域分隔签名）
   - `nonce` 按账户从 0 连续递增
2. 交易元数据验证必须满足：
   - `fee > 0`
   - 签名有效
   - nonce 序列严格连续
3. 交易查询只支持 4 个方法：
   - `getBlock`
   - `getTransaction`
   - `getReceipt`
   - `getBalance`

### 失败语义（当前实现）

- 缺 `tx_hash/account/height` 等必需参数：返回 `-32602`（invalid params）。
- 未知方法：`unknown method: ... valid: getBlock|getTransaction|getReceipt|getBalance`。
- 超限请求：HTTP 429，错误码 `-32029`。

## 4.2 验证与投票规则（当前可执行）

### 验证者与法定人数

1. 使用加权验证者集合。
2. 法定人数按活跃权重计算：`ceil(2/3 * active_total_weight)`。
3. 被 jail 的验证者在 jail 期间不计入活跃权重。

### 投票与 QC

1. 投票必须绑定：
   - 提案 hash
   - height
   - 签名
2. 同一验证者在同一高度：
   - 对同提案重复投票：拒绝（duplicate vote）
   - 对不同提案投票：判定 equivocation，记录 slash evidence
3. QC 验证必须同时满足：
   - 投票者均属于 validator set
   - 无重复 voter
   - `observed_weight == declared total_weight`
   - `observed_weight >= quorum`
   - 全部签名有效

## 4.3 罚没与解禁规则（当前可执行）

### 策略字段

`config/novovm-consensus-policy.json`

- `mode`: `enforce` / `observe_only`
- `equivocation_threshold`: 达阈值才允许 jail
- `min_active_validators`: 保护下限，防止误伤导致无活性
- `cooldown_epochs`: 自动解禁窗口（`0` 表示不自动解禁）

### 执行语义

1. 仅当 `mode=enforce` 且达到阈值且不破坏最小活跃验证者下限时，才实际 jail。
2. `observe_only` 只记录证据与执行记录，不 jail。
3. 配置可外置注入，非法配置应触发 `policy_invalid/policy_parse_failed` 失败门禁。
4. 开启 cooldown 时：
   - 到期前：不得参与（premature rejected）
   - 到期后：自动 unjail

## 4.4 发币规则（当前为“继承可用，NOVOVM 未接线”）

建议按以下继承规则迁入 NOVOVM：

1. `mint`：
   - amount > 0
   - amount <= locked_supply
   - total_supply + amount <= max_supply
2. `burn`：
   - 先校验余额充足，再扣余额，再减少 total_supply
3. `gas/service fee`：
   - 路由到 node/provider、treasury、burn 三方
   - 费率参数需保证 basis points 不超过 100%

## 4.5 治理投票规则（当前为“受限可执行 + 继承扩展”）

当前 NOVOVM 已支持最小治理 RPC：

1. `governance_submitProposal`
2. `governance_sign`
3. `governance_vote`
4. `governance_execute`
5. `governance_getProposal`
6. `governance_listProposals`
7. `governance_listAuditEvents`
8. `governance_getPolicy`

当前限制：

1. 当前主链路已支持提案族：`UpdateSlashPolicy`、`UpdateMempoolFeeFloor`、`UpdateNetworkDosPolicy`、`UpdateTokenEconomicsPolicy`、`TreasurySpend`、`UpdateGovernanceAccessPolicy`、`UpdateGovernanceCouncilPolicy`。
2. 权限与审计当前为进程级（allowlist + 内存审计事件），尚未上链。

后续建议继续按继承规则补齐：

1. 治理 API：
   - `governance_submitProposal`
   - `governance_vote`
   - `governance_sign`
   - `governance_listProposals`
2. 投票签名消息规范（SVM2026 现行）：
   - vote: `vote:{proposal_id}:{support}`
   - sign: `sign:{proposal_id}`
3. 验签失败必须拒绝（当前 SVM2026 通过 `QuantumResistantVerifier` 实现）。
4. 现阶段 NOVOVM 支持受限执行路径：`UpdateSlashPolicy`、`UpdateMempoolFeeFloor`、`UpdateNetworkDosPolicy`、`UpdateTokenEconomicsPolicy`、`TreasurySpend`、`UpdateGovernanceAccessPolicy`、`UpdateGovernanceCouncilPolicy`、`UpdateMarketGovernancePolicy` 可经签名投票 + quorum 生效；签名消息已做域分隔（`proposal_id + proposal_height + proposal_digest + support`）；`AMM/CDP/Bond/Reserve/NAV/Buyback` 已接线并具备 `market_engine + treasury` 门禁证据，当前剩余缺口主要是完整经济治理编排与长期运维治理。

---

## 5. 可发布范围建议

## 5.1 现在可对外宣布的范围

- NOVOVM 已具备 `MVP 共识与交易可运行闭环`：
  - 交易准入、投票收敛、出块提交、读查询、同步与网络活性/DoS 最小闭环均有门禁证据。

## 5.2 现在不应对外宣布“已完成”的范围

- 完整主网经济治理版（跨模块经济参数编排、经济策略持久化审计与恢复、长压运营口径）。
- 抗量子签名治理面（ML-DSA）与链上权限治理模型高级能力（成员轮换、链上审计索引、失效恢复）。

## 5.3 从 MVP 走向完整主网的最短补齐项

1. 从“参数可治理”走向“经济策略可发布”：补齐跨模块经济执行编排（参数 -> 执行 -> 结算 -> 审计）与回滚策略。
2. 在已上线链上权限模型初版（多签/委员会/时间锁）的基础上，从“节点侧审计持久化”升级到“链上持久化审计索引”，并逐步降级进程级 allowlist 为运维兜底。
3. 完成长期运行与生产运维口径（持久化 peer-score/ban、长压回归、参数变更审计追踪）。

---

## 6. 审计快照（本轮）

- 验收快照（主线）：`artifacts/migration/acceptance-gate-unjail-full/acceptance-gate-summary.json`
  - `overall_pass=true`
  - `functional_pass=true`
  - `performance_pass=true`
  - `chain_query_rpc_pass=true`
  - `header_sync_pass=true`
  - `fast_state_sync_pass=true`
  - `network_dos_pass=true`
  - `pacemaker_failover_pass=true`
  - `slash_governance_pass=true`
  - `slash_policy_external_pass=true`
  - `unjail_cooldown_pass=true`
  - `adapter_stability_pass=true`
- 增量快照（治理挂点）：`artifacts/migration/acceptance-gate-governance-smoke/acceptance-gate-summary.json`
  - `overall_pass=true`
  - `governance_hook_pass=true`
- 增量快照（治理执行最小闭环）：`artifacts/migration/acceptance-gate-governance-exec-smoke/acceptance-gate-summary.json`
  - `overall_pass=true`
  - `governance_hook_pass=true`
  - `governance_execution_pass=true`
- 增量快照（治理顺序推进闭环：2->1）：`artifacts/migration/acceptance-gate-governance-param2-smoke/acceptance-gate-summary.json`
  - `overall_pass=true`
  - `governance_hook_pass=true`
  - `governance_execution_pass=true`
  - `governance_param2_pass=true`
  - `governance_negative_pass=true`
- 增量快照（第三类治理参数 gate）：`artifacts/migration/governance-param3-gate-smoke/governance-param3-gate-summary.json`
  - `pass=true`
  - `parse_pass=true`
  - `input_pass=true`
  - `output_pass=true`
- 增量快照（九席位治理阈值 gate）：`artifacts/migration/governance-council-policy-gate-local/governance-council-policy-gate-summary.json`
  - `pass=true`
  - `parse_pass=true`
  - `input_pass=true`
  - `output_pass=true`
- 增量快照（经济治理参数族 gate）：`artifacts/migration/governance-market-policy-gate-local/governance-market-policy-gate-summary.json`
  - `pass=true`
  - `parse_pass=true`
  - `input_pass=true`
  - `output_pass=true`
- 增量快照（经济治理参数族 gate，engine/treasury 硬门禁）：`artifacts/migration/acceptance-gate-market-engine-smoke/acceptance-gate-summary.json`
  - `overall_pass=true`
  - `governance_market_policy_pass=true`
  - `governance_market_policy_engine_pass=true`
  - `governance_market_policy_treasury_pass=true`
- 全量快照（GA + 九席位治理）：`artifacts/migration/release-snapshot-ga-council-local/release-snapshot.json`
  - `overall_pass=true`
  - `profile_name=full_snapshot_ga_v1`
  - `enabled_gates.governance_council_policy=true`
  - `key_results.governance_council_policy_pass=true`
- RC 产物（GA + 九席位治理）：`artifacts/migration/release-candidate-novovm-rc-2026-03-06-ga-council-local/rc-candidate.json`
  - `status=ReadyForMerge/SnapshotGreen`
  - `governance_council_policy_pass=true`
- 全量快照（GA + market policy）：`artifacts/migration/release-snapshot-ga-market-local/release-snapshot.json`
  - `overall_pass=true`
  - `profile_name=full_snapshot_ga_v1`
  - `enabled_gates.governance_market_policy=true`
  - `key_results.governance_market_policy_pass=true`
- RC 产物（GA + market policy）：`artifacts/migration/release-candidate-novovm-rc-2026-03-06-ga-market-local/rc-candidate.json`
  - `status=ReadyForMerge/SnapshotGreen`
  - `governance_market_policy_pass=true`
- 增量快照（治理 RPC 最小面接线）：`artifacts/migration/acceptance-gate-governance-rpc-smoke-v2/acceptance-gate-summary.json`
  - `overall_pass=true`
  - `governance_rpc_pass=true`
  - `governance-rpc-gate` 中 `sign1_ok=true`, `unauthorized_submit_reject_ok=true`, `audit_ok=true`
  - `governance_hook_pass=true`
  - `governance_execution_pass=true`
  - `governance_param2_pass=true`
  - `governance_param3_pass=true`
  - `governance_negative_pass=true`
- 增量快照（治理审计持久化索引）：`artifacts/migration/governance-rpc-gate-audit-persist-smoke/governance-rpc-gate-summary.json`
  - `pass=true`
  - `audit_ok=true`
  - `audit_persist_ok=true`
  - `audit_persist_count=10`
- 增量快照（治理审计持久化接入 acceptance）：`artifacts/migration/acceptance-gate-governance-audit-persist-smoke/acceptance-gate-summary.json`
  - `overall_pass=true`
  - `governance_rpc_pass=true`
  - `governance_rpc_audit_persist_pass=true`
- 增量快照（治理签名算法 staged 门禁）：`artifacts/migration/governance-rpc-gate-signature-scheme-smoke/governance-rpc-gate-summary.json`
  - `pass=true`
  - `sign_unsupported_scheme_reject_ok=true`
  - `audit_has_sign_reject_unsupported_scheme=true`
- 增量快照（治理签名算法 staged 接入 acceptance）：`artifacts/migration/acceptance-gate-governance-signature-scheme-smoke/acceptance-gate-summary.json`
  - `overall_pass=true`
  - `governance_rpc_pass=true`
  - `governance_rpc_signature_scheme_reject_pass=true`
- 全量发布快照（full gates）：`artifacts/migration/release-snapshot-2026-03-05/release-snapshot.json`
  - `overall_pass=true`
  - `profile_name=full_snapshot_v1`
  - `enabled_gates.*=true`
  - `rpc_pass/governance_pass/sync_pass/adapter_pass/dos_pass/consensus_pass=true`
- 全量发布快照 relfix（param3 + adapter stability 回归）：`artifacts/migration/release-snapshot-param3-smoke-relfix/release-snapshot.json`
  - `overall_pass=true`
  - `profile_name=full_snapshot_v1`
  - `governance_pass=true`
  - `consensus_pass=true`
  - `governance_param3_pass=true`（见 `acceptance-gate-summary.json`）
  - `adapter_stability_pass=true`（见 `acceptance-gate-summary.json`）
- 门禁稳定性根因与修复（固定记录）：
  - 根因：relative `OutputDir` + child process `cwd` 变化导致 whitelist negative path drift
  - 修复：在 `scripts/migration/run_functional_consistency.ps1` 与 `scripts/migration/run_adapter_stability_gate.ps1` 中将 `OutputDir` 归一化为绝对路径
- 门禁稳定性修复（2026-03-06）：
  - 根因：`adapter_plugin_registry_negative.hash_mismatch` 负例偶发进程异常退出（`reason_match` 丢失）导致单轮误红
  - 修复：`scripts/migration/run_adapter_stability_gate.ps1` 对该已知抖动形态增加定向单次重试（仅命中该特征才重试，不掩盖其他失败）
- relfix 证据路径：
  - `artifacts/migration/adapter-stability-relfix-smoke/adapter-stability-summary.json`
  - `artifacts/migration/release-snapshot-param3-smoke-relfix/release-snapshot.json`
  - `artifacts/migration/release-snapshot-param3-smoke-relfix/acceptance-gate-full/acceptance-gate-summary.json`
- GA 全量快照（含 token economics + treasury spend）：
  - `artifacts/migration/release-snapshot-ga-smoke-treasury/release-snapshot.json`
  - `profile_name=full_snapshot_ga_v1`
  - `overall_pass=true`
  - `governance_access_policy_pass=true`
  - `governance_token_economics_pass=true`
  - `governance_treasury_spend_pass=true`
- GA 全量快照（正式产物，含 market_engine/treasury 硬门禁）：
  - `artifacts/migration/release-snapshot-ga-2026-03-06-051653/release-snapshot.json`
  - `profile_name=full_snapshot_ga_v1`
  - `overall_pass=true`
  - `key_results.governance_market_policy_engine_pass=true`
  - `key_results.governance_market_policy_treasury_pass=true`
- RC 全量快照（含 governance access policy + token economics + treasury spend）：
  - `artifacts/migration/release-candidate-novovm-rc-2026-03-06-ga-v1-retryfix/rc-candidate.json`
  - `status=ReadyForMerge/SnapshotGreen`
  - `governance_access_policy_pass=true`
  - `governance_token_economics_pass=true`
  - `governance_treasury_spend_pass=true`
- RC 全量快照（正式 `rc_ref`）：
  - `artifacts/migration/release-candidate-novovm-rc-2026-03-06-ga-v1/rc-candidate.json`
  - `status=ReadyForMerge/SnapshotGreen`
  - `commit_hash=823a5880e104c96d03e2ab4a8473c9f620ae6413`
  - `governance_market_policy_engine_pass=true`
  - `governance_market_policy_treasury_pass=true`
- 全量发布快照（治理审计持久化接线回归）：
  - `artifacts/migration/release-snapshot-audit-persist-smoke/release-snapshot.json`
  - `profile_name=full_snapshot_v1`
  - `overall_pass=true`
  - `key_results.governance_rpc_audit_persist_pass=true`
- RC 快照（治理审计持久化接线回归）：
  - `artifacts/migration/release-candidate-novovm-rc-2026-03-06-governance-audit-persist-smoke/rc-candidate.json`
  - `status=ReadyForMerge/SnapshotGreen`
  - `governance_rpc_audit_persist_pass=true`
- 全量发布快照（治理签名算法 staged 回归）：
  - `artifacts/migration/release-snapshot-signature-scheme-smoke/release-snapshot.json`
  - `profile_name=full_snapshot_v1`
  - `overall_pass=true`
  - `key_results.governance_rpc_signature_scheme_reject_pass=true`
- RC 快照（治理签名算法 staged 回归）：
  - `artifacts/migration/release-candidate-novovm-rc-2026-03-06-signature-scheme-smoke/rc-candidate.json`
  - `status=ReadyForMerge/SnapshotGreen`
  - `governance_rpc_signature_scheme_reject_pass=true`

该快照对应结论：当前 NOVOVM “共识+交易+读查询+网络活性防护+最小经济治理（token economics + treasury spend）”链路处于可发布状态；后续缺口集中在完整经济治理域与治理权限模型。
