# NOVOVM 共识可发布规则深度审计与使用规则说明（2026-03-05）

## 0. 审计目标与结论速览

本审计回答 3 个问题：

1. 当前 NOVOVM 已迁移完成并可发布的共识规则有哪些。
2. 哪些规则属于 SVM2026 已有“可继承发布口径”但尚未迁移到 NOVOVM 主链路。
3. 发币、验证、交易、投票的使用规则应如何执行，哪些可立即用，哪些必须先迁移。

结论（可直接用于决策）：

- `可立即发布（MVP 范围）`：
  - 共识主链路：`weighted quorum + QC anti-tamper + equivocation/slash + view-change + fork-choice + slash policy + unjail cooldown`。
  - 治理最小闭环（受限范围）：`UpdateSlashPolicy`、`UpdateMempoolFeeFloor` 与 `UpdateNetworkDosPolicy` 的 `proposal -> vote(signature) -> quorum -> apply`。
  - 治理防重放域分隔：治理签名消息绑定 `proposal_id + proposal_height + proposal_digest + support`。
  - 交易闭环：`tx wire -> mempool admission -> tx metadata -> block/commit output`。
  - 读查询：`getBlock/getTransaction/getReceipt/getBalance` + RPC rate limit。
  - 网络与活性：`headers-first`、`fast/state sync`（含负向篡改拒绝）、`peer-score/ban`、`pacemaker failover`。
- `继承可发布但尚未迁移到 NOVOVM 主链路`：
  - 发币/经济参数（mint/burn、gas/service fee 分流、treasury）。
  - 治理投票完整面（proposal/vote/signature/执行闭环）。
- `治理入口当前状态`：
  - 已具备受限治理执行面：`UpdateSlashPolicy`、`UpdateMempoolFeeFloor`、`UpdateNetworkDosPolicy` 可经签名投票 + quorum 生效。
- `发布边界`：
  - 当前可发布口径是“`MVP 共识+交易+读查询`”，不是“完整主网经济治理版”。

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
| C-14 | 治理权限与审计：`proposer/executor allowlist` + 审计事件流（含 reject 事件） | `main.rs` (`NOVOVM_GOVERNANCE_*_ALLOWLIST`, `GovernanceRpcAuditEvent`) + `run_governance_rpc_gate.ps1` | `unauthorized_submit_reject_ok=true`, `audit_ok=true` | 已迁移（受限范围） |
| C-15 | 第三类治理参数：`UpdateNetworkDosPolicy` 经治理投票生效 | `types.rs` (`GovernanceOp::UpdateNetworkDosPolicy`, `NetworkDosPolicy`) + `protocol.rs` (`governance_network_dos_policy`) + `main.rs` (`governance_param3_probe`) | `governance_param3_gate.pass=true` | 已迁移（受限范围） |
| C-16 | RPC 暴露安全铁律：public/gov 端口分离、`gov rpc` 默认关闭、public 不暴露 `governance_*`、非回环治理端口需 allowlist | `main.rs` (`run_chain_query_rpc_server_mode`, `NOVOVM_ENABLE_GOV_RPC`, `NOVOVM_GOV_RPC_BIND`, `NOVOVM_GOV_RPC_ALLOWLIST`) + `run_rpc_exposure_gate.ps1` | `rpc_exposure_gate.pass=true` | 已迁移（可发布安全默认） |

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
- 治理负向 gate：`run_governance_negative_gate.ps1`，验证 `unauthorized_submit/invalid_signature/duplicate_vote/insufficient_votes/replay_execute` 全部拒绝。
- 投票域分隔校验：执行时强校验 `proposal_height/proposal_digest` 与提案一致，不一致直接拒绝。
- 治理 RPC gate：`run_governance_rpc_gate.ps1`，验证 `submit -> sign -> vote -> execute -> getPolicy` 正向闭环、`unauthorized proposer` 拒绝、重复投票拒绝、`listAuditEvents` 审计可观测；已接入 acceptance gate 的 `governance_rpc_pass`。
- RPC 暴露 gate：`run_rpc_exposure_gate.ps1`，验证默认安全态（public 拒绝 `governance_*` + gov 端口关闭）与受控开启态（gov 本地端口可用、public 仍拒绝）。
- 治理权限入口（RPC 进程级）：`NOVOVM_GOVERNANCE_PROPOSER_ALLOWLIST`、`NOVOVM_GOVERNANCE_EXECUTOR_ALLOWLIST`。
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

## 3. 继承可发布但尚未迁移完成的规则（SVM2026 来源）

本节是“可继承发布口径”，不是“已进入 NOVOVM 主链路”。

## 3.1 发币/经济规则（I-TOKEN）

| 规则ID | 规则 | 来源 | 当前 NOVOVM 状态 |
|---|---|---|---|
| I-TOKEN-01 | Token trait 定义 mint/burn/fee-routing 标准接口 | `contracts/web30/core/src/mainnet_token.rs` | 未迁入主链路 |
| I-TOKEN-02 | mint 约束：`amount>0`、不超过 `locked_supply`、不突破 `max_supply` | `mainnet_token_impl.rs` (`mint`) | 未迁入主链路 |
| I-TOKEN-03 | burn 约束：先扣余额再销毁总量 | `mainnet_token_impl.rs` (`burn`) | 未迁入主链路 |
| I-TOKEN-04 | Gas 费路由（示例 20/30/50）与国库入账 | `economic_context.rs` (`charge_gas`) | 未迁入主链路 |
| I-TOKEN-05 | Service fee 路由（provider/treasury/burn） | `mainnet_token_impl.rs` (`on_service_fee_paid`) | 未迁入主链路 |

补充：迁移文档已明确“发币能力在 NOVOVM 侧为条件成立，但主链路未完成迁移”，且经济模块迁移仍在后续计划项。

## 3.2 治理与投票规则（I-GOV）

| 规则ID | 规则 | 来源 | 当前 NOVOVM 状态 |
|---|---|---|---|
| I-GOV-01 | 九席位加权治理模型与提案阈值 | `contracts/web30/core/src/governance.rs` | 未迁入主链路 |
| I-GOV-02 | 参数治理热更新（AMM/CDP/Bond/Gov） | `src/vm-runtime/src/governance.rs` | 未迁入主链路 |
| I-GOV-03 | RPC 治理接口：submit/vote/sign/list | `supervm-node-core/src/rpc/governance_api.rs` | NOVOVM 已接入受限最小面（`submit/sign/vote/execute/get/list/getPolicy/listAuditEvents`），权限/审计为进程级实现 |
| I-GOV-04 | 投票/签名消息校验与抗量子签名验证（ML-DSA） | 同上 | NOVOVM 未接入 |

注：NOVOVM 已具备治理挂点占位、最小执行闭环、第二类参数与受限 RPC 执行面；但完整治理域（多类型提案全量、链上权限治理、链上审计索引）仍未接入主链路。

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

1. 仅支持当前最小提案族（`UpdateSlashPolicy`、`UpdateMempoolFeeFloor`、`UpdateNetworkDosPolicy`）。
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
4. 现阶段 NOVOVM 支持受限执行路径：`UpdateSlashPolicy`、`UpdateMempoolFeeFloor`、`UpdateNetworkDosPolicy` 可经签名投票 + quorum 生效；并且签名消息已做域分隔（`proposal_id + proposal_height + proposal_digest + support`）；其余治理类型仍未接线。

---

## 5. 可发布范围建议

## 5.1 现在可对外宣布的范围

- NOVOVM 已具备 `MVP 共识与交易可运行闭环`：
  - 交易准入、投票收敛、出块提交、读查询、同步与网络活性/DoS 最小闭环均有门禁证据。

## 5.2 现在不应对外宣布“已完成”的范围

- 发币主链路（mint/burn/treasury/gas economics）已迁移完成。
- 治理投票完整主链路（`sign`、权限、审计、全参数族）已迁移完成。

## 5.3 从 MVP 走向完整主网的最短补齐项

1. 把 SVM2026 的 `MainnetToken + EconomicContext` 迁入 NOVOVM 主链路并新增门禁（mint/burn/gas split/treasury）。
2. 在已完成的 `UpdateSlashPolicy` 最小治理闭环基础上，扩展到完整治理提案族（参数域、权限域、经济域），并把签名错误/重放/越权纳入负向门禁。
3. 在当前已通过门禁基础上增加生产参数治理与长压口径（尤其是同步与信誉惩罚持久化）。

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
- 增量快照（治理 RPC 最小面接线）：`artifacts/migration/acceptance-gate-governance-rpc-smoke-v2/acceptance-gate-summary.json`
  - `overall_pass=true`
  - `governance_rpc_pass=true`
  - `governance-rpc-gate` 中 `sign1_ok=true`, `unauthorized_submit_reject_ok=true`, `audit_ok=true`
  - `governance_hook_pass=true`
  - `governance_execution_pass=true`
  - `governance_param2_pass=true`
  - `governance_param3_pass=true`
  - `governance_negative_pass=true`
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
- relfix 证据路径：
  - `artifacts/migration/adapter-stability-relfix-smoke/adapter-stability-summary.json`
  - `artifacts/migration/release-snapshot-param3-smoke-relfix/release-snapshot.json`
  - `artifacts/migration/release-snapshot-param3-smoke-relfix/acceptance-gate-full/acceptance-gate-summary.json`

该快照对应结论：当前 NOVOVM “共识+交易+读查询+网络活性防护”的 MVP 规则链路处于可发布状态；发币与治理仍是继承规则，待迁移落地。
