# NOVOVM L1-L4 回补状态机常驻手册（2026-03-23）

## 1. 目标

把回补状态机从“手动批处理触发”升级为“常驻循环执行”。

## 2. 一键命令

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-l1l4-reconcile-daemon.ps1 -RpcEndpoint http://127.0.0.1:9899 -SenderAddress 0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
```

## 3. 行为

1. 周期调用 `scripts/novovm-l1l4-reconcile.ps1`。
2. 正常时按 `IntervalSeconds` 等待下一轮。
3. 失败时按 `RestartDelaySeconds` 快速重试。
4. 可设置 `MaxCycles`/`MaxFailures` 限制运行窗口。

## 4. 常用参数

```powershell
-IntervalSeconds 15 -RestartDelaySeconds 3 -ReplayMaxPerPayout 3 -ReplayCooldownSec 30
```

## 5. 首轮全量重放

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-l1l4-reconcile-daemon.ps1 -FullReplayFirstCycle -RpcEndpoint http://127.0.0.1:9899 -SenderAddress 0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
```

## 6. 与统一入口一体化

如需让回补 daemon 与主链路同生命周期运行，可直接使用：

仓库内不再保留 `novovm-up.ps1` 的可执行示例命令。

主线入口只保留 `novovmctl daemon --profile prod --role-profile l3`。

原 `novovm-up.ps1 -Daemon -Reconcile*` 口径已降级为遗留兼容壳，不再属于主线 runbook 命令面。
