# NOVOVM UA 生产运维命令（2026-03-23）

## 1. 目标

给 UA 主线补两条生产命令，不引入额外观测链路：

1. `UA store 一键备份/恢复`
2. `统一入口常驻守护`
3. `主线入口收口（统一到 novovmctl）`

主线入口：`novovmctl`（UA store 历史操作仍保留遗留兼容壳）

## 2. UA store 一键备份/恢复

默认覆盖路径（rocksdb）：

1. `artifacts/gateway/unified-account-router.rocksdb`
2. `artifacts/migration/unifiedaccount/ua-plugin-self-guard-router.rocksdb`
3. `artifacts/migration/unifiedaccount/ua-plugin-self-guard-audit.rocksdb`

### 2.1 备份

该操作仍属 UA store 历史兼容动作，仓库内不再保留 `novovm-up.ps1` 的可执行示例命令。

产物目录：

`artifacts/migration/unifiedaccount/store-backups/<timestamp>/`

### 2.2 恢复（默认恢复最新快照）

该操作仍属 UA store 历史兼容动作，仓库内不再保留 `novovm-up.ps1` 的可执行示例命令。

恢复指定快照：

同上，指定快照恢复仍归入遗留兼容流程，不纳入主线命令面。

说明：

1. 恢复前默认要求 `novovm-node` / `novovm-evm-gateway` 已停止。
2. 如需绕过该检查可加 `-Force`（仅用于你确认停机窗口内）。

### 2.3 迁移（rocksdb 路径迁移）

该操作仍属 UA store 历史兼容动作，仓库内不再保留 `novovm-up.ps1` 的可执行示例命令。

说明：

1. `migrate` 只做 rocksdb 路径迁移，不做 `.bin/.jsonl` 编解码转换。
2. 若源为 `.bin/.jsonl`，脚本会直接阻断，避免生产误迁。

## 3. 统一入口常驻守护

生产守护运行：

```powershell
novovmctl daemon --profile prod
```

说明：`prod + daemon` 默认启用 `UseNodeWatchMode + LeanIo`，由 `novovm-node` 常驻消费 `opsw1`，并跳过 done/failed 归档路径以减少磁盘 I/O。
在 `LeanIo` 下，gateway/node 默认不落地 stdout/stderr 日志文件，进一步降低磁盘写入。
在 `LeanIo` 下，失败 `.opsw1` 默认直接删除（`NOVOVM_OPS_WIRE_WATCH_DROP_FAILED=1`），避免失败路径磁盘写放大。

仅连接外部 gateway：

```powershell
novovmctl daemon --profile prod -NoGateway
```

可配置重启间隔与最大重启次数：

```powershell
novovmctl daemon --profile prod -RestartDelaySeconds 5 -MaxRestarts 20
```

可直通生产性能参数（不引入观测链路）：

```powershell
novovmctl daemon --profile prod -PollMs 100 -SupervisorPollMs 1000 -NodeWatchBatchMaxFiles 2048 -GatewayBind 0.0.0.0:9899 -SpoolDir artifacts/ingress/spool -GatewayMaxRequests 0
```

如需显式打开低 I/O（非 daemon 也可）：

```powershell
novovmctl daemon --profile prod --use-node-watch-mode --lean-io --no-gateway
```

如需在非 daemon 下显式启用常驻消费模式：

```powershell
novovmctl daemon --profile prod --use-node-watch-mode --no-gateway
```

如需强制每次启动前先构建（默认生产模式为跳过构建以减少开销）：

```powershell
novovmctl daemon --profile prod -BuildBeforeRun
```

遗留兼容壳（内部转发到 `novovmctl daemon`）：

仓库内不再保留 `novovm-prod-daemon.ps1` 的可执行示例命令。

`novovm-prod-daemon.ps1` 仅作为遗留兼容壳保留，主线生产入口已统一为 `novovmctl daemon`。

## 4. 口径

1. 主线生产运维入口已统一为 `novovmctl`；本文件中的 `.ps1` 仅在遗留兼容壳/阶段外场景保留。
2. 不新增测试框架依赖，不改业务执行语义。
