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

主线入口：`novovmctl lifecycle`。`scripts/novovm-node-lifecycle.ps1` 仅保留遗留兼容壳。

## 2. 版本注册（把二进制纳入 release）

```powershell
novovmctl lifecycle `
  --action register `
  --version v2026.04.03 `
  -GatewayBinaryFrom target\debug\novovm-evm-gateway.exe `
  --node-binary-from target\debug\novovm-node.exe `
  --set-current
```

注册后 release 目录：

1. `artifacts/runtime/releases/v2026.04.03/novovm-evm-gateway.exe`
2. `artifacts/runtime/releases/v2026.04.03/novovm-node.exe`

## 3. 启动常驻节点（跳过构建，直接跑 release）

```powershell
novovmctl lifecycle `
  --action start `
  --profile prod `
  --role-profile l3 `
  -GatewayBind 0.0.0.0:9899 `
  -EnableReconcileDaemon `
  -ReconcileSenderAddress 0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa `
  -ReconcileRpcEndpoint http://127.0.0.1:9899
```

说明：

1. 实际进程主线由 `novovmctl up` / `novovmctl daemon` 承担。
2. `novovmctl lifecycle` 负责 PID 管理、版本路径注入、升级/回滚编排。

## 4. 状态与停止

状态：

```powershell
novovmctl lifecycle --action status
```

停止：

```powershell
novovmctl lifecycle --action stop
```

## 5. 升级（失败自动回滚）

先注册目标版本后执行升级：

```powershell
novovmctl lifecycle `
  --action upgrade `
  --target-version v2026.04.04 `
  --upgrade-health-seconds 12
```

行为：

1. 停止当前进程
2. 切换 `current_release` 到目标版本
3. 启动新版本并做健康等待
4. 若健康失败，自动回滚到 `previous_release`

## 6. 运行参数收口（set-runtime）

仅更新状态（不重启）：

```powershell
novovmctl lifecycle `
  --action set-runtime `
  --runtime-template-file .\config\runtime\lifecycle\prod-l3.runtime.json `
  --poll-ms 100 `
  -SupervisorPollMs 500
```

更新状态并立刻重启生效：

```powershell
novovmctl lifecycle `
  --action set-runtime `
  --runtime-template-file .\config\runtime\lifecycle\prod-l3.runtime.json `
  --restart-after-set-runtime
```

说明：

1. 模板字段会覆盖 `state.runtime` 对应字段。
2. 命令行显式参数优先级高于模板。
3. 仅显式传入的参数会覆盖当前值，未传入参数保持原值。
4. 仓库已提供样例模板：`config/runtime/lifecycle/prod-l3.runtime.json`。

## 7. 分组策略收口（set-policy + upgrade guard）

设置节点分组/升级窗口：

```powershell
novovmctl lifecycle `
  --action set-policy `
  --node-group canary `
  --upgrade-window "02:00-04:00 UTC"
```

按分组保护执行升级（防误升）：

```powershell
novovmctl lifecycle `
  --action upgrade `
  --target-version v2026.04.04 `
  --require-node-group canary `
  --upgrade-health-seconds 12
```

## 8. 手工回滚

回滚到 `previous_release`：

```powershell
novovmctl lifecycle --action rollback
```

回滚到指定版本：

```powershell
novovmctl lifecycle `
  --action rollback `
  --rollback-version v2026.04.03
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
2. 外层自动分批灰度编排控制器主线已补齐：`novovmctl rollout`。
3. 灰度控制器已支持跨主机执行（`local|ssh|winrm`）。
4. 灰度控制器使用手册：`docs_CN/NOVOVM-NODE-GRAY-ROLLOUT-CONTROLLER-RUNBOOK-2026-04-03.md`。
5. 灰度集中调度控制面主线已补齐：`novovmctl rollout-control`（多计划队列/并发限流/跨区域窗口）。
6. 控制面使用手册：`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。
7. 控制面策略增强已补齐：优先级抢占、区域容量配额、自动重试退避。
8. 控制面策略学习化调度已补齐：失败率 EMA + 区域拥塞动态调参。
9. 控制面多控制器一致性治理已补齐：主备仲裁 + 去重执行。
10. 控制面跨站点控制器共识已补齐：异地冲突仲裁 + 全局幂等。

## 11. 当前工作区可直接执行命令（D:\WEB3_AI\SUPERVM）

先进入仓库目录：

```powershell
Set-Location D:\WEB3_AI\SUPERVM
```

1. 注册版本 `v2026.04.03` 并设为当前：

```powershell
novovmctl lifecycle `
  --action register `
  --version v2026.04.03 `
  -GatewayBinaryFrom target\debug\novovm-evm-gateway.exe `
  --node-binary-from target\debug\novovm-node.exe `
  --set-current
```

2. 启动常驻（生产、L3）：

```powershell
novovmctl lifecycle `
  --action start `
  --profile prod `
  --role-profile l3 `
  -GatewayBind 0.0.0.0:9899 `
  -EnableReconcileDaemon `
  -ReconcileSenderAddress 0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa `
  -ReconcileRpcEndpoint http://127.0.0.1:9899
```

3. 查看状态：

```powershell
novovmctl lifecycle --action status
```

4. 注册新版本 `v2026.04.04`：

```powershell
novovmctl lifecycle `
  --action register `
  --version v2026.04.04 `
  -GatewayBinaryFrom target\debug\novovm-evm-gateway.exe `
  --node-binary-from target\debug\novovm-node.exe
```

5. 升级到新版本（失败自动回滚）：

```powershell
novovmctl lifecycle `
  --action upgrade `
  --target-version v2026.04.04 `
  --upgrade-health-seconds 12
```

6. 手工回滚（若需要）：

```powershell
novovmctl lifecycle --action rollback
```

7. 设置节点分组为 canary（用于灰度门控）：

```powershell
novovmctl lifecycle `
  --action set-policy `
  --node-group canary `
  --upgrade-window "02:00-04:00 UTC"
```
