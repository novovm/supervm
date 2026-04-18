# NOVOVM Treasury Policy P2-C 总览封盘草案（2026-04-18）

Status: SUPERSEDED by `NOVOVM-TREASURY-POLICY-P2C-SEAL-2026-04-18.md`

## 目的

本草案用于把已签收的 P2-C 子阶段收成一份统一总览，避免后续推进时阶段边界漂移。

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
- `P2-C overall`：进行中

## P2-C 当前已封盘范围

### 1. policy 对象具备版本与来源

- `policy_version` 已进入查询与 journal 追踪。
- `policy_source` 已归一并稳定（`config_path`、`governance_path`）。
- 治理更新后的版本与来源可延续到后续结算事实。

### 2. threshold state 已执行化

- `healthy`、`constrained`、`blocked` 已形成可执行行为差异。
- clearing 不再是“有路由就尝试”，而是先过 policy 状态门。

### 3. constrained strategy 已执行化

- 最小策略枚举已冻结：
  - `daily_volume_only`
  - `treasury_direct_only`
  - `blocked`
- 策略路由约束先于通用失败逻辑执行。
- 拒绝语义已稳定并可查询。

### 4. 跨视图 policy context 已合同化

同一组 policy context 已可在以下视图一致追踪：

- receipt（`policy_meta`）
- last selected route
- candidate routes
- settlement summary
- settlement policy query
- risk summary
- settlement journal

## P2-C 已冻结合同面

### A. policy 标识字段（冻结）

- `policy_contract_id`
- `policy_version`
- `policy_source`
- `policy_threshold_state`
- `policy_constrained_strategy`

### B. constrained 策略枚举（冻结）

- `daily_volume_only`
- `treasury_direct_only`
- `blocked`

### C. 失败码命名空间（冻结）

- `fee.quote.*`
- `fee.clearing.*`
- `fee.settlement.*`

## 当前验收基线

以下命令继续作为最小签收门：

- `cargo fmt --all --check`
- `cargo check -p novovm-node`
- `cargo clippy -p novovm-node --all-targets -- -D warnings`
- `cargo test -p novovm-node`
- `cargo run -p novovm-node --bin supervm-mainline-gate`
- `cargo deny check --disable-fetch`（按现行策略允许非阻塞 warning）

## 明确不包含（仍在范围外）

- multi-hop 路由
- 拆单 clearing
- 自动策略调参引擎
- 复杂金融扩展层（staking、分红、收益产品）

## P2-C overall 剩余边界

后续尾段建议继续收窄为三件事：

1. 参数对象稳定比较与差异追踪
2. constrained 行为差异继续细化（保持可解释，不做求解器）
3. config/governance 全路径同构锁死（跨视图一致）

## 建议对外口径

`P2-C 已建立“可版本化、可来源化、可状态化、可策略化、可跨视图追踪”的 policy 执行合同；P2-C overall 仍处于受控尾段收口。`
