# NOVOVM 灰度集中调度控制面手册（2026-04-04）

## 1. 目标

在 `novovm-node-rollout.ps1` 之上增加集中调度控制面，覆盖：

1. 多计划队列执行
2. 计划级并发限流
3. 跨区域时间窗编排
4. 统一执行认证与审计追踪

脚本：`scripts/novovm-node-rollout-control.ps1`。

## 2. 队列模板

默认队列文件：`config/runtime/lifecycle/rollout.queue.json`。

关键字段：

1. `max_concurrent_plans`：最多并发执行的计划数量
2. `poll_seconds`：进程轮询间隔
3. `dispatch_pause_seconds`：计划发车间隔
4. `plans[]`：每个区域/批次一条计划
5. `plans[].region_window`：计划级窗口（`HH:MM-HH:MM UTC`）
6. `plans[].plan_file`：底层灰度计划文件（传给 `novovm-node-rollout.ps1`）

## 3. 基础执行（升级）

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-node-rollout-control.ps1 `
  -PlanAction upgrade `
  -QueueFile .\config\runtime\lifecycle\rollout.queue.json `
  -ControllerId ops-main `
  -OperationId rollout-control-2026-04-04-a `
  -AuditFile .\artifacts\runtime\rollout\control-plane-audit.jsonl
```

## 4. 跨区域窗口门控

默认强制执行窗口门控：

1. 不在 `plans[].region_window` 的计划会被阻断并记审计。
2. 计划内节点仍受 `upgrade_window` 二次门控（由底层 rollout 脚本执行）。

需要临时绕过计划窗口时，显式加：

```powershell
-IgnoreUpgradeWindow
```

## 5. 认证与凭据

1. 控制器准入：`rollout.plan.json` 的 `controllers.allowed_ids`。
2. SSH：支持 `SshIdentityFile`、`SshKnownHostsFile`、`SshStrictHostKeyChecking`。
3. WinRM：优先从环境变量读取账号密码（不落文件）：
   - `NOVOVM_WINRM_USER`
   - `NOVOVM_WINRM_PASS`

## 6. 并发与失败策略

1. 计划级并发：`-MaxConcurrentPlans` 或队列文件 `max_concurrent_plans`。
2. 节点级失败阈值：由每个 `rollout.plan.json` 的 `groups[].max_failures` 控制。
3. 计划失败策略：
   - 默认失败即停后续计划
   - `-ContinueOnPlanFailure` 允许继续后续计划

## 7. 审计文件

1. 控制面审计：默认 `artifacts/runtime/rollout/control-plane-audit.jsonl`
2. 计划内审计：由队列中每条计划的 `audit_file` 指定

建议固定 `ControllerId + OperationId`，便于全链路追踪。
