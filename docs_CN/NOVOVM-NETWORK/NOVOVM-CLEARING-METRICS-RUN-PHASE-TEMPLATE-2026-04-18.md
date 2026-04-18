# NOVOVM Clearing Metrics 运行阶段报告模板（2026-04-18）

Status: OPERATIONAL TEMPLATE（运行阶段报告格式权威模板）  
Scope: 基于 P2-D 封盘后的 Day-1 / Week-1 报告格式与一键导出流程

## 目的

本模板用于统一运行阶段的观测报告口径：

- 用一键脚本导出 `nov_*` 观测数据
- 固化日报结构
- 固化周汇总结构

本模板不启用 P3。P3 仍为 `Decision Only / Not Enabled`。

## 一键导出

在仓库根目录执行：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/novovm-p2d-daily-report.ps1
```

可选参数示例：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/novovm-p2d-daily-report.ps1 `
  -RpcUrl "http://127.0.0.1:8899" `
  -DayLabel "2026-04-18" `
  -OutputDir "artifacts/mainline/p2d-run-phase/2026-04-18" `
  -JournalLimit 100
```

输出产物：

- `NOVOVM-CLEARING-METRICS-REPORT-<day>.md`
- `export-summary.json`
- `raw/clearing_metrics.json`
- `raw/policy_metrics.json`
- `raw/settlement_summary.json`
- `raw/clearing_summary.json`
- `raw/settlement_policy.json`
- `raw/execution_trace.json`
- `raw/settlement_journal.json`

## 日报模板

```markdown
# NOVOVM Clearing Metrics Report - <YYYY-MM-DD>

- Generated at (UTC):
- RPC endpoint:
- Raw snapshot directory:

## 1. Clearing Overview

| Metric | Value |
| --- | ---: |
| total_clearing_attempts | |
| successful_clearings | |
| failed_clearings | |
| success_rate | |

## 2. P3 Decision Inputs (Current Snapshot)

- denominator (attempts_non_nov excluding quote_expired and clearing_disabled):
- route_unavailable:
- insufficient_liquidity:
- slippage_exceeded:
- included_failure_total:
- blocked_state_hits:

Failure counts (all):
- ...

## 3. Policy Snapshot

- policy_contract_id:
- policy_source:
- threshold_state:
- constrained_strategy:

threshold_state_hits:
- ...

constrained_strategy_hits:
- ...

## 4. Settlement Buckets Snapshot

- reserve bucket NOV:
- fee bucket NOV:
- risk_buffer bucket NOV:
- journal total entries:

## 5. Execution Trace Snapshot

- trace_found:
- trace_tx_hash:
- trace_final_status:
- trace_final_failure_code:

## 6. Operator Notes

- Decision statement: P3 remains Decision Only / Not Enabled unless threshold policy is satisfied.
- Observation:
- Action:
```

## Week-1 汇总模板

```markdown
# NOVOVM Clearing Metrics Weekly Rollup - Week 1

## 1. Window
- Start (UTC):
- End (UTC):
- Included day reports:

## 2. Aggregated Clearing Indicators
- attempts_non_nov_7d:
- route_unavailable_7d:
- insufficient_liquidity_7d:
- slippage_exceeded_7d:
- failure_combined_rate_7d:

## 3. Policy and Risk Indicators
- blocked_ratio_7d:
- threshold_state distribution:
- constrained_strategy distribution:
- risk alerts:

## 4. Decision
- P3-A decision: hold / enable-canary
- Reason:
- Next checkpoint:
```

## 固定边界

- 本报告不改变已封盘的 P2-C/P2-D 语义。
- 不得基于单日快照直接宣告 P3 启用。
- 术语保持稳定：`route_unavailable`、`insufficient_liquidity`、`slippage_exceeded`、`blocked`。
