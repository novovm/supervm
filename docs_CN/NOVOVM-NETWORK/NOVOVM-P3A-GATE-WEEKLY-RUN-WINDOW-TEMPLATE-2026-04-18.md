# NOVOVM P3-A 开关 7 天运行窗口模板（2026-04-18）

Status: OPERATIONAL TEMPLATE（周度决策报告权威格式）  
Scope: 用于 P3-A 启用/保持关闭判定的 7 天真实运行窗口报告  
Depends on:
- `NOVOVM-OBSERVABILITY-P2D-SEAL-2026-04-18.md`
- `NOVOVM-P3-FEATURE-GATE-DECISION-THRESHOLDS-2026-04-18.md`

## 目的

本模板用于把 P3-A 周度判定固定为可执行规则，而不是主观判断：

- 分母口径固定
- 纳入/排除失败码固定
- 启用/保持关闭结论固定
- 回退/熔断检查固定

## 固定指标口径

统计口径固定为：

- population：仅 `pay_asset != NOV` clearing 尝试
- window：滚动 7 天（UTC）
- 纳入失败码：
  - `fee.clearing.route_unavailable`
  - `fee.clearing.insufficient_liquidity`
  - `fee.clearing.slippage_exceeded`
- 分母排除：
  - `fee.quote.quote_expired`
  - `fee.clearing.clearing_disabled`

## 输入文件（最小集合）

7 天窗口内每天至少保留：

- `NOVOVM-CLEARING-METRICS-REPORT-<day>.md`
- `export-summary.json`
- `raw/clearing_metrics.json`
- `raw/policy_metrics.json`
- `raw/settlement_summary.json`
- `raw/clearing_summary.json`

## 报告模板

```markdown
# NOVOVM P3-A 周度开关判定报告 - <YYYY-MM-DD>

Status: DECISION INPUT（除非明确批准，否则 P3 保持关闭）
Window:
- Start (UTC):
- End (UTC):
- Included daily reports:

## 1. 7 天 clearing 聚合指标
- attempts_non_nov_7d:
- route_unavailable_7d:
- insufficient_liquidity_7d:
- slippage_exceeded_7d:
- failure_combined_7d:
- failure_combined_rate_7d:

## 2. 策略与风险指标（7d）
- blocked_ratio_7d:
- threshold_state 分布:
- constrained_strategy 分布:
- risk_buffer 告警:

## 3. P3-A 启用条件检查
- 条件 A ((route_unavailable + insufficient_liquidity) / attempts >= 20%): pass/fail
- 条件 B (slippage_exceeded / attempts >= 10%): pass/fail
- 条件 C (blocked_ratio < 5%): pass/fail
- 条件 D (无 active risk alert): pass/fail
- 最终启用资格: yes/no

## 4. 回退/熔断检查
- 24h 连续 combined failure >= 25%: yes/no
- 24h 连续 blocked >= 10%: yes/no
- risk alert active: yes/no
- rollback required: yes/no

## 5. 判定结论
- Decision: HOLD / ENABLE-CANARY-10%
- Reason:
- Owner:
- Timestamp (UTC):

## 6. 若进入 ENABLE-CANARY-10%，放量约束
- 阶段顺序: 10% -> 25% -> 50% -> 100%
- 每阶段最短观察: 24h（建议 72h）
- 晋级条件: 当前阶段未触发任一回退条件

## 7. 审计附件
- Metrics 快照目录:
- Raw 证据路径:
- Trace 引用:
```

## 固定边界

- 本模板本身不会启用 P3。
- 任意启用动作必须由独立日期化决策文件记录。
- 若未完成明确决策，feature flags 保持不变。
