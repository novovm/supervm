# NOVOVM 治理用户入口封盘（2026-04-18）

Status: SEALED（Authoritative）  
Scope: `novovm-node` 真实产物入口上的治理用户面（`governance_getPolicy` / `governance_getProposal` / `governance_listProposals` / `governance_listAuditEvents` / `governance_listChainAuditEvents` / `governance_submitProposal` / `governance_sign` / `governance_vote` / `governance_execute`）

## 目的

本文件用于封盘一条已经完成的关键链路：

`真实 novovm-node 入口 -> mainline_query -> governance_surface -> consensus protocol`

本次封盘的意义不是“新增治理能力”，而是把已存在的治理读面、写面、执行面与签名面正式统一接入真实可运行产物入口，并用真实产物级门禁验证其未破坏主线。

## 最终结论

治理读面 + 写面 + 执行面 + `governance_sign`，已经统一进入真实 `novovm-node` 主线产物。`mldsa87 external vote` 的后续封盘见 `docs_CN/NOVOVM-NETWORK/NOVOVM-GOVERNANCE-MLDSA87-EXTERNAL-VOTE-SEAL-2026-04-18.md`。

当前可成立的项目状态：

- `经济用户入口`：已接入真实 `novovm-node`
- `治理用户入口`：已接入真实 `novovm-node`
- `governance_sign`：已接入真实 `novovm-node`
- `真实产物级门禁`：已通过
- `入口体系`：已统一到主线
- `P3`：Decision Only / Not Enabled

## 已封盘范围（代码事实）

### 1）真实入口识别已成立

`src/bin/novovm-node.rs` 与 `src/bin/supervm-mainline-query.rs` 已能识别 `governance_*` 方法，不再把治理方法提前误判为 unsupported canonical query。

关键接线点：

- `crates/novovm-node/src/bin/novovm-node.rs`
- `crates/novovm-node/src/bin/supervm-mainline-query.rs`
- `crates/novovm-node/src/mainline_query.rs`
- `crates/novovm-node/src/governance_surface.rs`

### 2）真实治理用户入口已成立

已进入真实产物入口的方法：

- `governance_getPolicy`
- `governance_getProposal`
- `governance_listProposals`
- `governance_listAuditEvents`
- `governance_listChainAuditEvents`
- `governance_submitProposal`
- `governance_sign`
- `governance_vote`
- `governance_execute`

这些方法已经统一由 `crates/novovm-node/src/mainline_query.rs` 路由到 `crates/novovm-node/src/governance_surface.rs`，不再依赖 dead `main.rs` 或旧 gov RPC 面。

### 3）治理入口只做统一接线，不复制治理逻辑

`mainline_query` / `governance_surface` 层只负责：

- 识别方法
- 做最小参数装配
- 统一进入当前治理 surface
- 把治理执行语义交回 consensus protocol

不成立第二套治理内核，不引入旁路，不绕开主线校验。

### 4）治理状态闭环已成立

治理入口不只是“能调”，还具备：

- governance store 持久化
- 签名缓存持久化
- chain audit / rpc audit 查询面
- consensus snapshot / recover

这意味着当前治理入口已经具备：

`可落盘 + 可恢复 + 可审计`

而不是一次性、临时性的调用器。

## 冻结的入口合同

本次冻结的治理入口合同如下：

| 方法 | 入口语义 | 状态 |
| --- | --- | --- |
| `governance_getPolicy` | 查询当前治理策略与阈值配置 | 已封盘 |
| `governance_getProposal` | 查询单个提案状态 | 已封盘 |
| `governance_listProposals` | 查询当前提案列表 | 已封盘 |
| `governance_listAuditEvents` | 查询治理审计事件流 | 已封盘 |
| `governance_listChainAuditEvents` | 查询链内治理审计事件流 | 已封盘 |
| `governance_submitProposal` | 提交治理提案 | 已封盘 |
| `governance_sign` | 生成并缓存投票签名（当前仅本地 `ed25519`） | 已封盘 |
| `governance_vote` | 提交投票；未显式传签名时可消费缓存签名 | 已封盘 |
| `governance_execute` | 执行已满足条件的提案 | 已封盘 |

## 已成立的签名缓存链路

本次封盘明确成立如下链路：

`submitProposal -> governance_sign -> governance_vote -> execute`

其中：

- `governance_sign` 生成的签名会落入 governance store
- `governance_vote` 在未显式传 `signature` 时可消费缓存签名
- 缓存消费完成后，不保留悬空签名状态

这意味着 `governance_sign` 已经不是单纯的辅助接口，而是治理真实用户面的一部分。

## 保留不变的治理本质约束

本次封盘不修改治理规则本身。以下约束继续由 consensus 内核负责：

- `committee`
- `threshold`
- `timelock`
- active validator 校验
- proposal / vote / execute 的主线治理语义

本次封盘做的是“统一入口”，不是“放宽治理规则”。

## 验收基线（2026-04-19 本地实际执行）

本次封盘基于以下真实执行结果：

- `cargo fmt --all`
- `cargo test -p novovm-node`
- `cargo clippy -p novovm-node --all-targets -- -D warnings`
- 真实 `novovm-node` smoke：
  - `submitProposal -> governance_sign -> governance_vote -> execute`
  - 本地 smoke 中 `mempool_fee_floor` 已成功更新
- 真实产物级门禁：
  - `cargo run -p novovm-node --bin supervm-mainline-gate`
  - 结果：`supervm mainline gate passed`
  - 结果：`L1=100% L2=100% L3=100% L4=100% Overall=100%`

## 本文件未覆盖的后续扩展

本文件封盘的是治理基础入口统一，不单独覆盖以下后续扩展：

- `mldsa87 external vote`

对应后续状态见：

- `docs_CN/NOVOVM-NETWORK/NOVOVM-GOVERNANCE-MLDSA87-EXTERNAL-VOTE-SEAL-2026-04-18.md`

这意味着本文件当前只宣称：

`治理基础用户面已经统一进入真实主线产物`

## 与经济用户入口封盘的关系

本文件与以下文档构成当前治理入口体系的里程碑：

- `docs_CN/NOVOVM-NETWORK/NOVOVM-NATIVE-ECONOMIC-USER-SURFACE-SEAL-2026-04-18.md`
- `docs_CN/NOVOVM-NETWORK/NOVOVM-GOVERNANCE-USER-SURFACE-SEAL-2026-04-18.md`
- `docs_CN/NOVOVM-NETWORK/NOVOVM-GOVERNANCE-MLDSA87-EXTERNAL-VOTE-SEAL-2026-04-18.md`

统一后的项目读取方式应为：

- 经济面：真实主线用户入口已可用
- 治理面：真实主线用户入口已可用
- `governance_sign`：真实主线用户入口已可用
- `mldsa87 external vote`：真实主线扩展验签路径已可用

## 建议对外口径

`NOVOVM 的真实用户入口体系已经统一完成到“经济面 + 治理面 + governance_sign + mldsa87 external vote”；当前治理边界为单 active verifier，不支持 mixed verifier 与本地 mldsa87 governance_sign。`
