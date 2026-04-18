# NOVOVM 双轨清算与市场定价制度（P2-A 实施冻结）
_2026-04-17_

## 1. 目的

本文件用于把“货币制度、储备制度、清算制度、市场定价制度”的交叉边界一次收敛，作为 `P2-A` 实施冻结稿。

目标不是扩功能，而是先把制度边界写成可执行约束，避免后续实现再次出现混账、混价、混责任。

## 2. 与现有阶段的关系

- `P0`：已签收（原生交易三分型与原生执行主入口成立）
- `P1-A`：已签收（quote 主线成立）
- `P1-B`：已签收（clearing 最小真实主线成立）
- `P1-C`：已签收（settlement 账务主线、journal、snapshot、query 成立）

本稿用于定义 `P2-A`：把 `pay_asset != NOV` 从“最小可用 clearing”升级为“制度轨 + 市场轨并存”的最小路由竞争主线。

## 3. 三池结构（冻结）

### 3.1 清算储备池（Reserve Settlement Pool, RSP）

来源：执行费收入、清算收入、外部结算流入等。

作用：

1. 支撑规则化清算兑换。
2. 提供系统风险缓冲。
3. 作为信用扩张约束的底层储备之一。

### 3.2 镜像托管池（Mirror Custody Pool, MCP）

来源：外链资产锁仓（例如 ETH/USDT 在插件侧锁仓）。

作用：

1. 支撑镜像资产的 1:1 回滚赎回。
2. 保证托管资产与镜像负债对应。

硬约束：MCP 资产不进入市场做市，不作为系统自由支配储备。

### 3.3 市场流动池（Market Liquidity Pool, MLP）

来源：做市流动性（系统投放或外部 LP）。

作用：

1. 提供市场价格发现。
2. 提供交易深度与滑点缓冲。
3. 承接套利收敛。

## 4. 双轨价格制度（冻结）

### 4.1 规则清算价（轨道 A）

由公开规则决定，不等于市场成交价。

参考形式：

`清算价 = 参考价 × 储备折扣 × 风险折扣 × 费率因子`

特点：有限容量、可审计参数、可治理调整。

### 4.2 市场交易价（轨道 B）

由 AMM/撮合市场决定，允许自由波动。

特点：无固定承诺、由流动性与交易行为形成。

### 4.3 冻结结论

- 清算价不等于市场价。
- AMM 价不等于规则清算价。
- 两者通过套利在区间内收敛，但职责不同。

## 5. 资产分层（冻结）

### 5.1 NOV

基础结算币（M0/M1 主体）。

### 5.2 镜像资产

建议命名 `m*`（如 `mETH`、`mUSDT`），用于表示 1:1 托管映射与回滚赎回权。

### 5.3 信用扩张资产

保留 `n*`（如 `nUSD`、`nRWA`），归属 M2。

冻结约束：镜像资产与信用扩张资产不得混同命名与混同风控。

## 6. 与当前代码主线的对齐

### 6.1 已成立能力

- `fee.quote.*` 与 `fee.clearing.*` 失败码边界已分离。
- `quote -> clearing -> settlement` 已有最小闭环。
- `settled_fee_nov / paid_asset / paid_amount / route_ref` 可见。
- settlement journal 与 accounting snapshot 已可查询。

### 6.2 当前缺口（P2-A）

- route source 仍偏最小实现。
- 市场轨与规则轨尚未形成标准化“多 route 可选”主线。
- 多 source 选择与回退策略尚未固定。

## 7. P2-A 实施冻结（最小可执行）

### 7.1 范围

只做“多 route 最小聚合器”，不做复杂聚合器。

### 7.2 最小模型

1. Route source 至少支持两类：
   - `reserve_direct`
   - `amm_pool`
2. Router 固定三段：
   - `quote_routes`
   - `select_best_route`
   - `execute_selected_route`
3. 选择策略先固定为：`max_expected_out`。

### 7.3 失败码（保持前缀冻结）

- `fee.clearing.route_unavailable`
- `fee.clearing.insufficient_liquidity`
- `fee.clearing.quote_expired`
- `fee.clearing.slippage_exceeded`

不新增跨前缀混合失败码。

### 7.4 回执与查询必带

- `route_id`
- `route_source`
- `expected_nov_out`
- `route_fee_ppm`

并继续保持 `settled_fee_nov / paid_asset / paid_amount`。

## 8. 风险控制边界（P2-A）

1. 报价有效期（TTL）硬校验。
2. 滑点上限硬校验。
3. `max_pay_amount` 硬校验。
4. 流动性不足必须失败，不允许静默降级为错误结算。
5. MCP 与 MLP 账务隔离，不允许资产跨池挪用。

## 9. 验收门（P2-A）

1. `pay_asset != NOV` 至少可在两条 route 之间择优执行。
2. route 不可用与流动性不足能稳定返回标准化失败码。
3. 成功回执可见 route 元数据。
4. clearing result 继续进入 settlement 主线，不绕开 NOV 内部结算。
5. `cargo check / clippy / test / supervm-mainline-gate` 全绿。

## 10. 本稿替换与冲突规则

若后续实现与本稿冲突：

1. 先更新本稿并写明偏离理由。
2. 再改实现。
3. 未更新文档的偏离不视为有效决议。

---

本文件是 `P2-A` 的制度级冻结稿，用于保证“规则清算轨 + 市场交易轨 + 三池隔离”先成立，再扩展复杂聚合与高级策略。
