# NOVOVM Treasury Policy P2-C 正式封盘（2026-04-18）

Status: SEALED（Authoritative）  
Supersedes: `NOVOVM-TREASURY-POLICY-P2C-OVERVIEW-SEAL-DRAFT-2026-04-18.md`  
Scope: P2-C Stage1 + Stage2 + constrained strategy + policy consolidation/closure hardening

## 目的

本文件是 P2-C treasury policy 阶段的正式封盘文档，用于冻结已完成范围、冻结合同面、以及明确范围外边界。后续变更应按新阶段处理，不得隐式扩展 P2-C。

## 阶段状态

- `P0`：已签收
- `P1-A`：已签收
- `P1-B`：已签收
- `P1-C`：已签收
- `P2-A`：已签收
- `P2-B1`：已签收
- `P2-B2`：已签收
- `P2-C Stage1`：已签收
- `P2-C Stage2`：已签收
- `P2-C constrained strategy`：已签收
- `P2-C 后续策略层增量（第二刀）`：已签收
- `P2-C 收口增强`：已签收
- `P2-C overall`：受控尾段收口中

## 已封盘范围

### 1）policy 合同标识已冻结

- `policy_contract_id`
- `policy_version`
- `policy_source`
- `policy_threshold_state`
- `policy_constrained_strategy`

上述字段为稳定标识字段，且可查询。

### 2）policy 来源与版本可延续

- `policy_source` 已归一（`config_path`、`governance_path`）。
- 治理更新后的版本/来源可延续到后续结算事实。
- 版本与来源可在 journal 与 summary 中追踪。

### 3）threshold state 已执行化

- `healthy`、`constrained`、`blocked` 为可执行状态。
- 状态可支配候选路由与拒绝行为。
- 状态约束先于通用失败过滤执行。

### 4）constrained strategy 枚举已冻结

冻结策略值：

- `daily_volume_only`
- `treasury_direct_only`
- `blocked`

### 5）跨视图 policy context 已合同化

同一组 policy context 已可在以下视图一致追踪：

- receipt（`policy_meta`）
- last selected route
- candidate routes
- settlement summary
- settlement policy query
- risk summary
- settlement journal

### 6）journal 事件 policy 上下文同构

在适用范围内，accepted/settled/redeemed/rejected 等事件类均保留可追踪的 policy 上下文。

## 冻结失败码命名空间

- `fee.quote.*`
- `fee.clearing.*`
- `fee.settlement.*`

## 验收基线

最小签收门：

- `cargo fmt --all --check`
- `cargo check -p novovm-node`
- `cargo clippy -p novovm-node --all-targets -- -D warnings`
- `cargo test -p novovm-node`
- `cargo run -p novovm-node --bin supervm-mainline-gate`
- `cargo deny check --disable-fetch`（按现行策略允许非阻塞 warning）

## 明确不包含

以下仍在范围外：

- multi-hop routing
- 拆单 clearing
- 自动策略调参引擎
- 复杂金融扩展层（staking、分红、收益产品）

## 封盘后受控尾段边界

封盘后剩余工作建议继续收窄为：

1. 参数对象稳定比较与差异追踪
2. constrained 行为差异继续细化（保持可解释，不做求解器）
3. config/governance 全路径同构锁死（跨视图一致）

## 建议对外口径

`P2-C 已建立“可版本化、可来源化、可状态化、可策略化、可跨视图追踪”的 policy 执行合同。`

