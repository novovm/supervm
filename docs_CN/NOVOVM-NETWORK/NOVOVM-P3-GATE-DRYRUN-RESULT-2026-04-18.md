# NOVOVM P3 开关 Dry-Run 结果（2026-04-18）

Status: RECORDED RESULT（权威运行证据）  
Scope: 基于合成 `pay_asset != NOV` 样本的 P3 门槛可计算性验证  
Depends on:
- `NOVOVM-OBSERVABILITY-P2D-SEAL-2026-04-18.md`
- `NOVOVM-P3-FEATURE-GATE-DECISION-THRESHOLDS-2026-04-18.md`

## 目的

本文件用于记录首次“分母非 0”的 P3 门槛 dry-run 结果。该结果用于验证决策机制可执行，不用于触发 P3 启用。

## 证据快照

- 报告路径：`artifacts/mainline/p2d-run-phase/2026-04-18/NOVOVM-CLEARING-METRICS-REPORT-2026-04-18.md`
- 数据来源：`mainline_query`（RPC 不可用时自动回退）
- 样本类型：合成注入/回放（非生产真实流量）

## 关键指标（dry-run）

- `total_clearing_attempts = 3`
- `successful_clearings = 1`
- `failed_clearings = 2`
- 失败分布：
  - `insufficient_liquidity = 1`
  - `slippage_exceeded = 1`

## 正式结论

本次 dry-run 已证明：

1. P2-D Run Phase 导出链路可端到端运行。
2. P3 门槛分母与失败率可计算。
3. 决策规则可由日报直接执行判断。

本次 dry-run 未证明：

1. 生产流量真实行为。
2. 真实 route/liquidity 分布。
3. 可以正式开启 P3-A。

## 当前决策状态

- P3 状态：`Decision Only / Not Enabled`
- 决策状态：`可判定，但不可启用（仅合成样本）`

## 下一步运行动作

在任何 P3-A 决策前，先跑真实窗口：

- 短窗口：3 天（口径与稳定性确认）
- 首次正式判定窗口：7 天（生产级判定输入）

## 固定边界

- 不得将本文件作为 P3 启用依据。
- 不得根据本文件直接修改 P3 开关。
- 本文件仅用于“门槛机制可计算”与审计留痕。
