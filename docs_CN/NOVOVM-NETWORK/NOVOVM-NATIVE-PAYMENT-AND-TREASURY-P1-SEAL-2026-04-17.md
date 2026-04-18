# NOVOVM 原生支付与国库结算 P1 封盘（2026-04-17）

## 目的

本文件用于封盘 `P0 + P1-A + P1-B + P1-C` 的已完成范围，冻结当前主线口径，避免把后续增强项误读为当前已上线能力。

## 阶段状态

- `P0`：已签收
- `P1-A (Quote Engine)`：已签收
- `P1-B (Clearing Engine)`：已签收
- `P1-C (Treasury Settlement Full Path)`：已签收

## 当前已实现边界（代码事实）

### 原生交易与主入口
- `nov_*` 为主链一等入口，原生交易三分型（`Transfer / Execute / Governance`）已进入主线。
- 至少一个原生模块真实执行闭环已成立，回执与查询可见。

### 费用与结算主线
- `Execution Fee -> SettledFee(NOV)` 已进入 runtime 主线。
- `pay_asset == NOV` 走直结算主线。
- `pay_asset != NOV` 已具备最小真实 clearing 路径（非占位）。

### Quote 与 Clearing
- quote 具备价格源优先级、freshness 校验、标准失败码 `fee.quote.*`、回执元数据可见。
- clearing 具备路由选择、流动性检查、TTL/slippage/max-pay 校验、标准失败码 `fee.clearing.*`。

### Treasury Settlement
- `quote -> settle -> journal` 已接入。
- `redeem` 已写入 settlement journal。
- accounting snapshot 已可查询（净结算、分桶一致性检查）。

### 查询面
- `nov_getTreasurySettlementSummary`
- `nov_getTreasurySettlementPolicy`
- `nov_getTreasurySettlementJournal`

## 当前权威失败码口径

- Quote 相位：`fee.quote.*`
- Clearing 相位：`fee.clearing.*`
- 禁止混用 quote 与 clearing 失败前缀。

## 当前回执可见字段（核心）

- `settled_fee_nov`
- `paid_asset`
- `paid_amount`
- `fee_quote_id`
- `fee_quote_contract`
- `fee_quote_required_pay_amount`
- `fee_quote_expires_at_unix_ms`
- `fee_clearing_route_ref`

## P1 未包含能力（明确不宣称）

- 多源 AMM 聚合定价
- 复杂 route aggregation 策略
- treasury 全量高级风险策略参数化
- 国库完整宏观策略自动化执行

## 对外稳定口径（建议）

`NOV 原生支付主线已具备从 quote、clearing 到 treasury settlement 的完整最小闭环；多源聚合与高级风险策略属于后续增强，不影响当前主线成立。`

## 下一阶段命名建议

- `P2-A`：最小多 route 清算聚合（已由 P2-A 封盘文档单独收口）
- `P2-B1`：多源 route / liquidity aggregation
- `P2-B2`：风险策略参数化
- `P2-C`：更复杂 treasury policy / reserve strategy

## 后续封盘关联

- `docs_CN/NOVOVM-NETWORK/NOVOVM-CLEARING-ROUTER-P2A-SEAL-2026-04-17.md`

## 参考文档（冻结依据）

- `docs_CN/NOVOVM-NETWORK/NOVOVM-MONETARY-ARCHITECTURE-M0-M1-M2-AND-MULTI-ASSET-PAYMENT-2026-04-17.md`
- `docs_CN/NOVOVM-NETWORK/NOVOVM-NATIVE-TX-AND-EXECUTION-INTERFACE-DESIGN-2026-04-17.md`
