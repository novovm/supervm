# NOVOVM L1/L2/L3 多机生产部署矩阵（2026-03-23）

## 1. 目标

把 `RoleProfile` 从单机编排扩展为多机部署模板，统一命令口径，降低运维偏差。

## 2. 一键生成模板脚本

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-generate-role-matrix.ps1
```

默认会在 `artifacts/deploy/role-matrix` 生成：

1. `run-l1.ps1`
2. `run-l2.ps1`
3. `run-l3.ps1`
4. `run-full.ps1`
5. `README.txt`

## 3. 推荐机器角色

1. L1：最终性锚点与治理参数节点（不拉 gateway）。
2. L2：执行与证明算力节点（不拉 gateway）。
3. L3：接入与路由节点（拉 gateway，对外开放 RPC）。

## 4. 直接命令版本（不生成脚本时）

### L1 主机

```powershell
$env:NOVOVM_NODE_ID="novovm-l1-01"
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-up.ps1 -Profile prod -RoleProfile l1 -Daemon -SpoolDir artifacts/ingress/spool -PollMs 100 -SupervisorPollMs 1000 -NodeWatchBatchMaxFiles 2048
```

### L2 主机

```powershell
$env:NOVOVM_NODE_ID="novovm-l2-01"
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-up.ps1 -Profile prod -RoleProfile l2 -Daemon -SpoolDir artifacts/ingress/spool -PollMs 100 -SupervisorPollMs 1000 -NodeWatchBatchMaxFiles 2048
```

### L3 主机

```powershell
$env:NOVOVM_NODE_ID="novovm-l3-01"
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-up.ps1 -Profile prod -RoleProfile l3 -Daemon -GatewayBind 0.0.0.0:9899 -SpoolDir artifacts/ingress/spool -PollMs 100 -SupervisorPollMs 1000 -NodeWatchBatchMaxFiles 2048
```

## 5. L1 锚点写入口径（生产）

`-Profile prod` 下默认：

1. `NOVOVM_L1L4_ANCHOR_PATH=artifacts/l1/l1l4-anchor.jsonl`
2. `NOVOVM_L1L4_ANCHOR_LEDGER_ENABLED=1`
3. `NOVOVM_L1L4_ANCHOR_LEDGER_KEY_PREFIX=ledger:l1:l1l4_anchor:v1:`

含义：

1. 锚点继续写本地文件（运维可审计）。
2. 同一锚点同时写入统一账本键空间（执行主线内闭环）。

