# NOVOVM 节点灰度编排控制器手册（2026-04-03）

## 1. 目标

提供外层自动灰度编排控制器，按节点分组推进升级，不改业务代码路径：

1. 分组策略批量下发
2. 分组顺序升级
3. 分组失败阈值拦截
4. 失败节点自动回滚（可选）
5. 执行认证与控制器准入（controller allowlist）
6. 统一审计追踪（jsonl 审计日志）

统一脚本：`scripts/novovm-node-rollout.ps1`。  
底层执行器：`scripts/novovm-node-lifecycle.ps1`。

## 2. 计划文件

默认计划文件：`config/runtime/lifecycle/rollout.plan.json`。  
你只需要改两类内容：

1. `nodes[].repo_root`：每台节点机器上的仓库路径
2. `nodes[].node_group`：节点归属分组（如 `canary` / `stable`）

`groups[].max_failures` 为分组失败阈值，超过即停止后续组。

远程执行字段（跨主机）：

1. `nodes[].transport`：`local|ssh|winrm`（兼容写法：`remote_mode`）
2. `nodes[].remote_host`：远程主机
3. `nodes[].remote_repo_root`：远程仓库路径（可选，默认取 `repo_root`）
4. `nodes[].lifecycle_script_path`：远程生命周期脚本绝对路径（可选）
5. SSH 专用：`remote_user`、`remote_port`、`remote_shell`
6. WinRM 专用：`winrm_use_ssl`、`winrm_port`、`winrm_auth`

执行认证字段（计划与环境变量）：

1. `controllers.allowed_ids`：允许执行本计划的控制器 ID。
2. SSH 认证：`ssh_identity_file`、`ssh_known_hosts_file`、`ssh_strict_host_key`。
3. WinRM 凭据：`winrm_cred_user_env`、`winrm_cred_pass_env`（从环境变量取值，不落盘）。

## 3. 批量下发分组策略

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-node-rollout.ps1 `
  -Action set-policy `
  -PlanFile .\config\runtime\lifecycle\rollout.plan.json `
  -ControllerId local-controller `
  -AuditFile .\artifacts\runtime\rollout\audit.jsonl
```

## 4. 执行灰度升级

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-node-rollout.ps1 `
  -Action upgrade `
  -PlanFile .\config\runtime\lifecycle\rollout.plan.json `
  -TargetVersion v2026.04.04 `
  -ControllerId local-controller `
  -AuditFile .\artifacts\runtime\rollout\audit.jsonl `
  -UpgradeHealthSeconds 12 `
  -AutoRollbackOnFailure
```

行为：

1. 按 `group_order` 顺序推进（默认 canary -> stable）。
2. 每个节点调用 lifecycle 的 `upgrade`，并带 `RequireNodeGroup` 防误升。
3. 分组内错误数超过阈值即中断后续组。
4. 开启 `-AutoRollbackOnFailure` 时，失败节点立刻执行 lifecycle `rollback`。
5. `upgrade_window` 默认强制门控（不在窗口会阻断升级）；需要绕过时显式加 `-IgnoreUpgradeWindow`。
6. 每个节点动作都会写入审计文件，便于复盘与责任追踪。

跨主机升级（SSH + WinRM 混合）：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-node-rollout.ps1 `
  -Action upgrade `
  -PlanFile .\config\runtime\lifecycle\rollout.plan.json `
  -TargetVersion v2026.04.04 `
  -DefaultTransport local `
  -SshBinary ssh `
  -SshIdentityFile C:\Users\ops\.ssh\id_ed25519 `
  -SshKnownHostsFile C:\Users\ops\.ssh\known_hosts `
  -SshStrictHostKeyChecking accept-new `
  -WinRmCredentialUserEnv NOVOVM_WINRM_USER `
  -WinRmCredentialPasswordEnv NOVOVM_WINRM_PASS `
  -RemoteTimeoutSeconds 30 `
  -ControllerId ops-main `
  -OperationId rollout-2026-04-03-a `
  -AuditFile .\artifacts\runtime\rollout\audit.jsonl `
  -AutoRollbackOnFailure
```

## 5. 查看全组状态

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-node-rollout.ps1 `
  -Action status `
  -PlanFile .\config\runtime\lifecycle\rollout.plan.json
```

## 6. 全组回滚

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-node-rollout.ps1 `
  -Action rollback `
  -PlanFile .\config\runtime\lifecycle\rollout.plan.json
```

指定回滚版本：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-node-rollout.ps1 `
  -Action rollback `
  -PlanFile .\config\runtime\lifecycle\rollout.plan.json `
  -RollbackVersion v2026.04.03
```

## 7. 生产建议

1. 正式执行前先用 `-DryRun` 预演参数与顺序。
2. canary 分组建议 `max_failures=0`。
3. stable 分组建议小阈值（如 1）。
4. 脚本只做编排，实际节点进程治理仍由 lifecycle 承担。
5. 跨主机场景优先用计划文件里的 `transport` 明确指定，不依赖默认值推断。
6. WinRM 账号密码只放环境变量，不写进计划文件或命令行明文。
7. 每次灰度执行固定一个 `ControllerId` 和 `OperationId`，保证审计可追溯。

## 8. 与集中调度控制面的关系

单计划或小规模执行继续用本脚本。  
多计划并发、跨区域窗口编排请使用：

1. `scripts/novovm-node-rollout-control.ps1`
2. `docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`
