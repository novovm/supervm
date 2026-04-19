# NOVOVM 治理 MLDSA87 External Vote 封盘（2026-04-18）

Status: SEALED（Authoritative）  
Scope: 真实 `novovm-node` 主线产物上的 `mldsa87 external vote`

## 目的

本文件用于封盘一条已经完成的治理扩展链路：

`真实 novovm-node 入口 -> mainline_query -> governance_surface -> GovernanceVoteVerifier -> AOEM external verify -> consensus execute`

本次封盘的意义不是“重做治理逻辑”，而是在不破坏现有 `ed25519` 主线的前提下，把 `mldsa87 external vote` 正式接入真实治理入口，并通过真实产物级门禁验证其未破坏主线。

## 最终结论

`mldsa87 external vote` 已进入真实治理主线，并保持单 active verifier 边界。

当前可成立的项目状态：

- 默认治理验签：`ed25519`
- 扩展治理验签：`mldsa87 external vote`
- 治理状态机：未改动
- 真实产物级门禁：已通过
- 当前 verifier 边界：单 active verifier
- `P3`：Decision Only / Not Enabled

## 已封盘范围（代码事实）

### 1）验签扩展层已成立

当前实现复用了既有 `GovernanceVoteVerifier` 抽象，没有把 `mldsa87` 写死进治理主逻辑。

关键接线点：

- `crates/novovm-node/src/governance_verifier_ext.rs`
- `crates/novovm-node/src/governance_surface.rs`
- `crates/novovm-node/src/mainline_query.rs`

### 2）真实入口已成立

`governance_vote` 已支持显式传入：

- `signature`
- `mldsa_pubkey` / `pubkey`
- `signature_algo` / `signature_scheme`

这意味着当前成立的不是“内核能力存在”，而是：

`真实用户入口可调用`

### 3）主线治理纪律未被打乱

本次封盘没有改变以下事项：

- `proposal -> vote -> execute` 状态机
- `committee`
- `threshold`
- `timelock`
- 默认 `ed25519` 行为

扩展发生在验签层，而不是治理状态机本身。

## 当前支持边界

当前明确支持：

- 默认 `ed25519`
- 显式切换到 `mldsa87` 时，走 external vote
- 真实入口级 `mldsa87` 成功路径
- 真实入口级 `mldsa87` 错误签名拒绝路径

## 当前不支持边界

本次封盘后，仍明确不支持：

- mixed verifier
- 本地 `mldsa87 governance_sign`

换言之，当前项目语义是：

`单条 active verifier 主线下，支持第二种治理验签路径`

而不是：

`同一轮治理中同时混跑多种 verifier`

## 验收基线（2026-04-19 本地实际执行）

本次封盘基于以下真实执行结果：

- `cargo fmt --all`
- `cargo check -p novovm-node`
- `cargo test -p novovm-node`
- `cargo clippy -p novovm-node --all-targets -- -D warnings`
- 入口级治理回归：
  - `mldsa87` 成功执行链通过
  - `mldsa87` 错误签名拒绝链通过
- 真实产物级门禁：
  - `cargo run -p novovm-node --bin supervm-mainline-gate`
  - 结果：`supervm mainline gate passed`
  - 结果：`L1=100% L2=100% L3=100% L4=100% Overall=100%`

## 与治理用户入口封盘的关系

本文件是在以下封盘基础上的扩展里程碑：

- `docs_CN/NOVOVM-NETWORK/NOVOVM-GOVERNANCE-USER-SURFACE-SEAL-2026-04-18.md`

统一后的读取方式应为：

- `治理用户入口封盘`：治理读面、写面、执行面与 `governance_sign` 已统一进入真实主线
- `MLDSA87 external vote 封盘`：在不改变默认 `ed25519` 主线的前提下，第二种治理验签路径已接通

## 建议对外口径

`NOVOVM 治理真实入口体系已经完整覆盖：读面、写面、执行面、governance_sign，以及 mldsa87 external vote；当前保留边界仅为单 active verifier，不支持 mixed verifier 与本地 mldsa87 governance_sign。`
