# NOVOVM Clearing Router P2-A 封盘（2026-04-17）

## 目的

本文件用于封盘 `P2-A` 已完成范围，固定当前可执行边界，避免把后续增强项误读为已上线能力。

## 阶段状态

- `P0`：已签收
- `P1-A`：已签收
- `P1-B`：已签收
- `P1-C`：已签收
- `P2-A`：已签收

## P2-A 已完成范围（代码事实）

### 多 route 最小聚合主线已成立

- `pay_asset != NOV` 不再是单路径清算。
- 清算路径最小并存：
  - `TreasuryDirect`
  - `StaticAmmPool`
- Router 固定三段：
  - `quote_routes`
  - `select_best_route`
  - `execute_selected_route`

### 模块分层已落地

- `clearing_types.rs`
- `liquidity_sources.rs`
- `clearing_router.rs`
- `treasury_settlement.rs`
- `tx_ingress.rs`（编排与接线）

### 回执与查询已接线

- 回执可见 route 元数据：
  - `route_id`
  - `route_source`
  - `expected_nov_out`
  - `route_fee_ppm`
- 已接查询：
  - `treasury.get_clearing_routes`
  - `treasury.get_last_clearing_route`
  - `nov_getTreasuryClearingSummary`

### 失败码边界保持稳定

- `fee.quote.*` 与 `fee.clearing.*` 继续分层，不混前缀。
- `fee.clearing.*` 最小集已稳定覆盖：
  - `route_unavailable`
  - `insufficient_liquidity`
  - `quote_expired`
  - `slippage_exceeded`
  - `max_pay_exceeded`

### 主线验收与 gate

- `cargo check -p novovm-node`
- `cargo clippy -p novovm-node --all-targets -- -D warnings`
- `cargo test -p novovm-node`
- `cargo run -p novovm-node --bin supervm-mainline-gate`

上述验收在本地通过，`P2-A` 可签收。

## P2-A 未包含范围（明确非声明）

- multi-hop 路由
- 拆单路由
- 复杂 AMM 数学模型
- 智能全局最优路由
- 多源聚合报价重构

## 下一阶段命名冻结

为避免命名冲突，下一阶段统一使用：

- `P2-B1`：多源 route / liquidity aggregation
- `P2-B2`：risk hardening（TTL / slippage / liquidity guard / global switch）
- `P2-C`：settlement policy / reserve strategy 增强

## 稳定对外表述（推荐）

`NOV 原生支付清算主线已从单路径升级为“规则轨 + 市场轨”并存的最小多 route 聚合器；多源聚合与高级风控属于后续增强阶段。`

