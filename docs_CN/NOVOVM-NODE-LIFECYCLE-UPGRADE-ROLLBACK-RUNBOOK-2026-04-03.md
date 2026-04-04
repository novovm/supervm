# NOVOVM 公网节点生命周期编排手册（升级/回滚，2026-04-03）

## 1. 目标

提供生产可执行的节点生命周期编排，不改业务链路，只收口运维动作：

1. 版本注册
2. 常驻启动
3. 状态查询
4. 运行参数收口（runtime template + 参数热切换落状态）
5. 分组策略收口（node group / upgrade window）
6. 升级
7. 回滚

统一脚本：`scripts/novovm-node-lifecycle.ps1`。

## 2. 版本注册（把二进制纳入 release）

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-node-lifecycle.ps1 `
  -Action register `
  -Version v2026.04.03 `
  -GatewayBinaryFrom target\debug\novovm-evm-gateway.exe `
  -NodeBinaryFrom target\debug\novovm-node.exe `
  -SetCurrent
```

注册后 release 目录：

1. `artifacts/runtime/releases/v2026.04.03/novovm-evm-gateway.exe`
2. `artifacts/runtime/releases/v2026.04.03/novovm-node.exe`

## 3. 启动常驻节点（跳过构建，直接跑 release）

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-node-lifecycle.ps1 `
  -Action start `
  -Profile prod `
  -RoleProfile l3 `
  -GatewayBind 0.0.0.0:9899 `
  -EnableReconcileDaemon `
  -ReconcileSenderAddress 0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa `
  -ReconcileRpcEndpoint http://127.0.0.1:9899
```

说明：

1. 实际进程仍由 `scripts/novovm-up.ps1 -Daemon` 承担。
2. 生命周期脚本负责 PID 管理、版本路径注入、升级/回滚编排。

## 4. 状态与停止

状态：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-node-lifecycle.ps1 -Action status
```

停止：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-node-lifecycle.ps1 -Action stop
```

## 5. 升级（失败自动回滚）

先注册目标版本后执行升级：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-node-lifecycle.ps1 `
  -Action upgrade `
  -TargetVersion v2026.04.04 `
  -UpgradeHealthSeconds 12
```

行为：

1. 停止当前进程
2. 切换 `current_release` 到目标版本
3. 启动新版本并做健康等待
4. 若健康失败，自动回滚到 `previous_release`

## 6. 运行参数收口（set-runtime）

仅更新状态（不重启）：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-node-lifecycle.ps1 `
  -Action set-runtime `
  -RuntimeTemplateFile .\config\runtime\lifecycle\prod-l3.runtime.json `
  -PollMs 100 `
  -SupervisorPollMs 500
```

更新状态并立刻重启生效：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-node-lifecycle.ps1 `
  -Action set-runtime `
  -RuntimeTemplateFile .\config\runtime\lifecycle\prod-l3.runtime.json `
  -RestartAfterSetRuntime
```

说明：

1. 模板字段会覆盖 `state.runtime` 对应字段。
2. 命令行显式参数优先级高于模板。
3. 仅显式传入的参数会覆盖当前值，未传入参数保持原值。
4. 仓库已提供样例模板：`config/runtime/lifecycle/prod-l3.runtime.json`。

## 7. 分组策略收口（set-policy + upgrade guard）

设置节点分组/升级窗口：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-node-lifecycle.ps1 `
  -Action set-policy `
  -NodeGroup canary `
  -UpgradeWindow "02:00-04:00 UTC"
```

按分组保护执行升级（防误升）：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-node-lifecycle.ps1 `
  -Action upgrade `
  -TargetVersion v2026.04.04 `
  -RequireNodeGroup canary `
  -UpgradeHealthSeconds 12
```

## 8. 手工回滚

回滚到 `previous_release`：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-node-lifecycle.ps1 -Action rollback
```

回滚到指定版本：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-node-lifecycle.ps1 `
  -Action rollback `
  -RollbackVersion v2026.04.03
```

## 9. 状态文件与日志

1. 状态文件：`artifacts/runtime/lifecycle/state.json`
2. PID 文件：`artifacts/runtime/lifecycle/novovm-up.pid`
3. 日志目录：`artifacts/runtime/lifecycle/logs`

`state.json` 关键结构新增：

1. `runtime`：运行参数模板化结果（start/upgrade/rollback 按此执行）。
2. `governance.node_group`：节点升级分组标识。
3. `governance.upgrade_window`：节点升级窗口描述。

## 10. 运行边界

1. 当前已覆盖最小可用治理增强：runtime 模板收口 + 节点分组保护升级。
2. 外层自动分批灰度编排控制器已补齐：`scripts/novovm-node-rollout.ps1`。
3. 灰度控制器已支持跨主机执行（`local|ssh|winrm`）。
4. 灰度控制器使用手册：`docs_CN/NOVOVM-NODE-GRAY-ROLLOUT-CONTROLLER-RUNBOOK-2026-04-03.md`。
5. 灰度集中调度控制面已补齐：`scripts/novovm-node-rollout-control.ps1`（多计划队列/并发限流/跨区域窗口）。
6. 控制面使用手册：`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。

## 11. 当前工作区可直接执行命令（D:\WEB3_AI\SUPERVM）

先进入仓库目录：

```powershell
Set-Location D:\WEB3_AI\SUPERVM
```

1. 注册版本 `v2026.04.03` 并设为当前：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-node-lifecycle.ps1 `
  -Action register `
  -Version v2026.04.03 `
  -GatewayBinaryFrom target\debug\novovm-evm-gateway.exe `
  -NodeBinaryFrom target\debug\novovm-node.exe `
  -SetCurrent
```

2. 启动常驻（生产、L3）：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-node-lifecycle.ps1 `
  -Action start `
  -Profile prod `
  -RoleProfile l3 `
  -GatewayBind 0.0.0.0:9899 `
  -EnableReconcileDaemon `
  -ReconcileSenderAddress 0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa `
  -ReconcileRpcEndpoint http://127.0.0.1:9899
```

3. 查看状态：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-node-lifecycle.ps1 -Action status
```

4. 注册新版本 `v2026.04.04`：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-node-lifecycle.ps1 `
  -Action register `
  -Version v2026.04.04 `
  -GatewayBinaryFrom target\debug\novovm-evm-gateway.exe `
  -NodeBinaryFrom target\debug\novovm-node.exe
```

5. 升级到新版本（失败自动回滚）：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-node-lifecycle.ps1 `
  -Action upgrade `
  -TargetVersion v2026.04.04 `
  -UpgradeHealthSeconds 12
```

6. 手工回滚（若需要）：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-node-lifecycle.ps1 -Action rollback
```

7. 设置节点分组为 canary（用于灰度门控）：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-node-lifecycle.ps1 `
  -Action set-policy `
  -NodeGroup canary `
  -UpgradeWindow "02:00-04:00 UTC"
```
