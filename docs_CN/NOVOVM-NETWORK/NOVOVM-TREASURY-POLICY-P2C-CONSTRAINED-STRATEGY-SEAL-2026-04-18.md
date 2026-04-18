# NOVOVM Treasury Policy P2-C Constrained Strategy Seal (2026-04-18)

## 状态

- P2-C Stage 1: 已封盘
- P2-C Stage 2: 已封盘
- P2-C 后续策略层增量（Constrained Strategy）: 已封盘

## 本轮封盘范围

1. 新增 `clearing_constrained_strategy` 最小策略枚举：
   - `daily_volume_only`
   - `treasury_direct_only`
   - `blocked`
2. `threshold_state=constrained` 时，先按策略限制候选路由，再执行通用失败校验。
3. 新增失败语义：
   - `fee.clearing.constrained_blocked`
4. `policy_version / policy_source / threshold_state / clearing_constrained_strategy` 在 policy 与 risk 查询可见。

## 行为定义（Constrained）

- `daily_volume_only`:
  继续允许清算，但执行更紧的日额度与通用风控校验。
- `treasury_direct_only`:
  仅允许 `TreasuryDirect` 路由进入候选集；无可用候选时拒绝。
- `blocked`:
  直接拒绝清算，返回 `fee.clearing.constrained_blocked`。

## 已验证内容

- `cargo fmt --all --check`
- `cargo check -p novovm-node -p novovm-protocol -p novovm-consensus`
- `cargo clippy -p novovm-node -p novovm-protocol -p novovm-consensus --all-targets -- -D warnings`
- `cargo test -p novovm-node --quiet`
- `cargo run -p novovm-node --bin supervm-mainline-gate`

以上命令在本地均通过。

## 本轮明确不包含

- multi-hop 路由
- 拆单执行
- 复杂策略表达式（组合策略 / 自动调参）
- 收益分配、分红、staking 等金融扩展

## 当前结论

P2-C 已从“策略存在”推进到“策略以显式枚举约束 clearing 行为，并可查询、可回归验证”的阶段。
