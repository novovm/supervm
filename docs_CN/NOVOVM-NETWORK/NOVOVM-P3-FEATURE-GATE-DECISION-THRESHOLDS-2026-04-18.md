# NOVOVM P3 功能开关决策门槛（2026-04-18）

Status: AUTHORITATIVE  
Scope: P3 路由能力启用/回退决策规范  
Depends on: `NOVOVM-OBSERVABILITY-P2D-SEAL-2026-04-18.md`

## 目的

本文件把 P3 启用从“建议”升级为“可执行规则”。  
P3 默认保持关闭；任何启用动作都必须满足 P2-D 已封盘观测数据门槛。

## 指标口径冻结（防歧义）

分母与过滤条件固定如下：

- 样本总体：仅统计非 NOV clearing 尝试（`pay_asset != NOV`）
- 统计窗口：滚动 7 天（UTC 统一时基）
- 计入失败码：
  - `fee.clearing.route_unavailable`
  - `fee.clearing.insufficient_liquidity`
  - `fee.clearing.slippage_exceeded`
- 排除项（不计入分母与失败率）：
  - `fee.quote.quote_expired`
  - `fee.clearing.clearing_disabled`

记号：

- `attempts_non_nov_7d`：样本总体计数
- `failure_combined_7d`：计入失败码总数
- `failure_combined_rate_7d = failure_combined_7d / attempts_non_nov_7d`

## 口径示例（伪 SQL）

```sql
WITH base AS (
  SELECT *
  FROM execution_traces
  WHERE pay_asset <> 'NOV'
    AND created_at_utc >= now_utc - interval '7 day'
),
denom AS (
  SELECT *
  FROM base
  WHERE COALESCE(final_failure_code, '') NOT IN (
    'fee.quote.quote_expired',
    'fee.clearing.clearing_disabled'
  )
),
agg AS (
  SELECT
    COUNT(*) AS attempts_non_nov_7d,
    SUM(CASE WHEN final_failure_code = 'fee.clearing.route_unavailable' THEN 1 ELSE 0 END) AS route_unavailable_7d,
    SUM(CASE WHEN final_failure_code = 'fee.clearing.insufficient_liquidity' THEN 1 ELSE 0 END) AS insufficient_liquidity_7d,
    SUM(CASE WHEN final_failure_code = 'fee.clearing.slippage_exceeded' THEN 1 ELSE 0 END) AS slippage_exceeded_7d
  FROM denom
)
SELECT *,
       (route_unavailable_7d + insufficient_liquidity_7d + slippage_exceeded_7d)
         * 1.0 / NULLIF(attempts_non_nov_7d, 0) AS failure_combined_rate_7d
FROM agg;
```

## 默认开关

- `enable_multi_hop = false`
- `enable_split = false`

## P3-A（多源增强）决策规则

### 启用条件（除 OR 条件外需同时满足）

- （`route_unavailable + insufficient_liquidity`）/ `attempts_non_nov_7d` >= `20%`  
  OR `slippage_exceeded / attempts_non_nov_7d` >= `10%`
- `threshold_state=blocked` 占比 < `5%`（滚动 7 天）
- risk summary 中无风险缓冲告警

### 回退/熔断条件（任一触发即回退）

- 综合失败率 >= `25%` 且连续 `24h`
- `threshold_state=blocked` 占比 >= `10%` 且连续 `24h`
- 出现风险缓冲告警

### 发布策略（强制金丝雀）

- 流量分档：`10% -> 25% -> 50% -> 100%`
- 每档观察：至少 `24h`（建议 `72h`）
- 当前档未触发回退条件才允许升档

## P3-B（multi-hop）决策规则

### 启用条件

- P3-A 连续稳定 `14` 天
- 离线回放中位收益提升 >= `8%`
- CI/mainline gate 全绿

### 回退/熔断条件

- 继承 P3-A 全部回退条件
- 路径特异失败相对基线异常增长

## P3-C（split routing）决策规则

### 启用条件

- P3-B 连续稳定 `30` 天
- P95 滑点仍 >= 目标值 `+2%`
- 离线拆单回放在不提高失败率前提下提升结果

### 回退/熔断条件

- 继承 P3-A 全部回退条件
- 拆单特异失败相对基线异常增长

## 审计与可解释性要求

任何启用/回退决策都必须可由以下观测事实解释：

- execution trace（候选路由、最终路由、失败码、policy 上下文）
- clearing metrics summary
- policy metrics summary
- settlement/risk summary

## 明确不包含

本文件只定义决策规则：

- 不直接启用 P3
- 不改写已封盘 P2-C/P2-D 合同
- 不引入自动调参或策略求解器

