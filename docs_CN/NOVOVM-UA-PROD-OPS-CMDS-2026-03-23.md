# NOVOVM UA 生产运维命令（2026-03-23）

## 1. 目标

给 UA 主线补两条生产命令，不引入额外观测链路：

1. `UA store 一键备份/恢复`
2. `统一入口常驻守护`
3. `单入口收口（全部走 novovm-up）`

主入口：`scripts/novovm-up.ps1`

## 2. UA store 一键备份/恢复

默认覆盖路径（rocksdb）：

1. `artifacts/gateway/unified-account-router.rocksdb`
2. `artifacts/migration/unifiedaccount/ua-plugin-self-guard-router.rocksdb`
3. `artifacts/migration/unifiedaccount/ua-plugin-self-guard-audit.rocksdb`

### 2.1 备份

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-up.ps1 -UaStoreAction backup
```

产物目录：

`artifacts/migration/unifiedaccount/store-backups/<timestamp>/`

### 2.2 恢复（默认恢复最新快照）

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-up.ps1 -UaStoreAction restore
```

恢复指定快照：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-up.ps1 -UaStoreAction restore -UaSnapshot 20260323-021500
```

说明：

1. 恢复前默认要求 `novovm-node` / `novovm-evm-gateway` 已停止。
2. 如需绕过该检查可加 `-Force`（仅用于你确认停机窗口内）。

### 2.3 迁移（rocksdb 路径迁移）

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-up.ps1 -UaStoreAction migrate -GatewayStoreFrom "E:\old\gateway-ua.rocksdb" -PluginStoreFrom "E:\old\plugin-ua.rocksdb" -PluginAuditFrom "E:\old\plugin-ua-audit.rocksdb"
```

说明：

1. `migrate` 只做 rocksdb 路径迁移，不做 `.bin/.jsonl` 编解码转换。
2. 若源为 `.bin/.jsonl`，脚本会直接阻断，避免生产误迁。

## 3. 统一入口常驻守护

生产守护运行：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-up.ps1 -Profile prod -Daemon
```

说明：`prod + daemon` 默认启用 `UseNodeWatchMode + LeanIo`，由 `novovm-node` 常驻消费 `opsw1`，并跳过 done/failed 归档路径以减少磁盘 I/O。
在 `LeanIo` 下，gateway/node 默认不落地 stdout/stderr 日志文件，进一步降低磁盘写入。
在 `LeanIo` 下，失败 `.opsw1` 默认直接删除（`NOVOVM_OPS_WIRE_WATCH_DROP_FAILED=1`），避免失败路径磁盘写放大。

仅连接外部 gateway：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-up.ps1 -Profile prod -Daemon -NoGateway
```

可配置重启间隔与最大重启次数：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-up.ps1 -Profile prod -Daemon -RestartDelaySeconds 5 -MaxRestarts 20
```

可直通生产性能参数（不引入观测链路）：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-up.ps1 -Profile prod -Daemon -PollMs 100 -SupervisorPollMs 1000 -NodeWatchBatchMaxFiles 2048 -GatewayBind 0.0.0.0:9899 -SpoolDir artifacts/ingress/spool -GatewayMaxRequests 0
```

如需显式打开低 I/O（非 daemon 也可）：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-up.ps1 -Profile prod -UseNodeWatchMode -LeanIo -NoGateway
```

如需在非 daemon 下显式启用常驻消费模式：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-up.ps1 -Profile prod -UseNodeWatchMode -NoGateway
```

如需强制每次启动前先构建（默认生产模式为跳过构建以减少开销）：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-up.ps1 -Profile prod -Daemon -BuildBeforeRun
```

兼容入口（内部转发到 `novovm-up.ps1`）：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-prod-daemon.ps1 -Profile prod
```

`novovm-prod-daemon.ps1` 已支持透传 `PollMs/NodeWatchBatchMaxFiles/GatewayBind/SpoolDir/GatewayMaxRequests/LeanIo` 等性能参数。

## 4. 口径

1. 这是生产主线运维命令，不引入额外模拟/观测路径。
2. 不新增测试框架依赖，不改业务执行语义。
