# NOVOVM 经济开放面运维手册（最小可执行版，2026-03-13）

## 1. 适用范围

- 适用对象：superVM 经济开放面（Week3）值班与发布人员。
- 目标：保障故障可回滚、资金可对账、日终可巡检、阈值可告警。
- 范围：限流/熔断/配额/告警/审计字段与资金路径门禁。

## 2. 故障回滚 SOP

### 2.1 发布后门禁回归失败（P1）

1. 立即冻结发布窗口，禁止新增配置变更与新版本发布。
2. 执行最小回滚验证：
   - `pwsh -File scripts/migration/run_runtime_security_baseline_gate.ps1 -RepoRoot . -OutputDir artifacts/migration/week1-2026-03-13/runtime-security-baseline-gate-rollback -TimeoutSec 420`
   - `pwsh -File scripts/migration/run_funds_path_safety_gate.ps1 -RepoRoot . -OutputDir artifacts/migration/week1-2026-03-13/funds-path-safety-gate-rollback -TimeoutSec 420`
3. 若仍失败，切换只读运营模式（停止新增写入路径，保留查询与审计落盘）。
4. 以最近一次 `pass=true` 的门禁结果作为恢复基线，恢复后重新跑全量 Week3 门禁。

### 2.2 运营控制面失效（限流/熔断/配额/告警字段）

1. 立即执行聚合门禁：
   - `pwsh -File scripts/migration/run_ops_control_surface_gate.ps1 -RepoRoot . -OutputDir artifacts/migration/week1-2026-03-13/ops-control-surface-gate-rollback -TimeoutSeconds 420`
2. 若 `pass=false`，按 `error_reason` 定位子项（`rate_limit/circuit_breaker/quota/alert_field/audit_field`）。
3. 子项恢复后再次执行聚合门禁，`pass=true` 才允许恢复写入。

## 3. 资金对账 SOP（日内至少 2 次）

1. 执行资金路径安全门禁：
   - `pwsh -File scripts/migration/run_funds_path_safety_gate.ps1 -RepoRoot . -OutputDir artifacts/migration/week1-2026-03-13/funds-path-safety-gate-reconcile -TimeoutSec 420`
2. 检查汇总字段必须全部为 `true`：
   - `reconcile_pass`
   - `compensation_pass`
   - `failure_injection_pass`
   - `invariant_pass`
3. 失败时必须保存日志并升级为 P1，不允许继续资金写路径。

## 4. 日终巡检清单（EOD）

1. 稳定窗口任务存活检查（如已启动）：
   - `pgrep -af "run_stability_window_gate.ps1"`
   - 或执行 `pwsh -File scripts/migration/run_week4_blocker_status.ps1 -RepoRoot .`，检查输出中的 `process_running=true`
2. 运营控制面门禁：
   - `artifacts/migration/week1-2026-03-13/ops-control-surface-gate/ops-control-surface-gate-summary.json` 的 `pass=true`
3. 运行时安全门禁：
   - `artifacts/migration/week1-2026-03-13/runtime-security-baseline-gate/runtime-security-baseline-gate-summary.json` 的 `pass=true`
4. 资金路径门禁：
   - `artifacts/migration/week1-2026-03-13/funds-path-safety-gate/funds-path-safety-gate-summary.json` 的 `pass=true`
5. 当日结果回填到 Week3 清单“每日更新模板”。

## 5. 指标阈值与告警分级

- P1（阻断发布/写入）：
  - 任一门禁 `pass=false`
  - 资金路径 `failure_injection_pass=false` 或 `invariant_pass=false`
  - 审计字段完整性 `audit_field_pass=false`
- P2（可降级运行）：
  - 稳定窗口单次迭代失败但可重试恢复
  - 非关键只读查询项抖动
- 恢复准入：
  - 相关门禁连续 1 次 `pass=true` 且失败原因已定位并回填。

## 6. 发布阻断规则

- 以下任一条件不满足，禁止进入 GA：
  - Week3 门禁未全绿（经济专项、资金路径、运行时安全、运营控制面）
  - 72h 稳定窗口未完成
  - 漏洞审计 `Critical/High` 未清零

## 7. 证据目录（当前）

- `artifacts/migration/week1-2026-03-13/ops-control-surface-gate/ops-control-surface-gate-summary.json`
- `artifacts/migration/week1-2026-03-13/runtime-security-baseline-gate/runtime-security-baseline-gate-summary.json`
- `artifacts/migration/week1-2026-03-13/funds-path-safety-gate/funds-path-safety-gate-summary.json`
