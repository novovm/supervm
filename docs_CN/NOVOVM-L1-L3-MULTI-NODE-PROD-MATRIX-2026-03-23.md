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

生成脚本默认产出 `novovmctl daemon` 命令模板。

## 3. 推荐机器角色

1. L1：最终性锚点与治理参数节点（不拉 gateway）。
2. L2：执行与证明算力节点（不拉 gateway）。
3. L3：接入与路由节点（拉 gateway，对外开放 RPC）。

## 4. 直接命令版本（不生成脚本时）

### L1 主机

```powershell
$env:NOVOVM_NODE_ID="novovm-l1-01"
novovmctl daemon --profile prod --role-profile l1 --spool-dir artifacts/ingress/spool --poll-ms 100 --supervisor-poll-ms 1000 --node-watch-batch-max-files 2048
```

### L2 主机

```powershell
$env:NOVOVM_NODE_ID="novovm-l2-01"
novovmctl daemon --profile prod --role-profile l2 --spool-dir artifacts/ingress/spool --poll-ms 100 --supervisor-poll-ms 1000 --node-watch-batch-max-files 2048
```

### L3 主机

```powershell
$env:NOVOVM_NODE_ID="novovm-l3-01"
novovmctl daemon --profile prod --role-profile l3 --gateway-bind 0.0.0.0:9899 --spool-dir artifacts/ingress/spool --poll-ms 100 --supervisor-poll-ms 1000 --node-watch-batch-max-files 2048
```

## 5. L1 锚点写入口径（生产）

`-Profile prod` 下默认：

1. `NOVOVM_L1L4_ANCHOR_PATH=artifacts/l1/l1l4-anchor.jsonl`
2. `NOVOVM_L1L4_ANCHOR_LEDGER_ENABLED=1`
3. `NOVOVM_L1L4_ANCHOR_LEDGER_KEY_PREFIX=ledger:l1:l1l4_anchor:v1:`

含义：

1. 锚点继续写本地文件（运维可审计）。
2. 同一锚点同时写入统一账本键空间（执行主线内闭环）。

## 6. 覆盖层路由参数矩阵（生产口径）

`-Profile prod` 默认启用 `NOVOVM_OVERLAY_ROUTE_MODE=secure`，并收口为：

1. `NOVOVM_OVERLAY_ROUTE_REGION=global`
2. `NOVOVM_OVERLAY_ROUTE_RELAY_BUCKETS=8`
3. `NOVOVM_OVERLAY_ROUTE_RELAY_SET_SIZE=3`
4. `NOVOVM_OVERLAY_ROUTE_RELAY_ROTATE_SECONDS=60`
5. `NOVOVM_OVERLAY_ROUTE_STRATEGY=multi_hop`
6. `NOVOVM_OVERLAY_ROUTE_ENFORCE_MULTI_HOP=1`
7. `NOVOVM_OVERLAY_ROUTE_HOP_COUNT>=3`
8. `NOVOVM_OVERLAY_ROUTE_MIN_HOPS>=2`
9. `NOVOVM_OVERLAY_ROUTE_HOP_SLOT_SECONDS=30`

显式切换快速模式（仅在你确认场景需要时）：

1. `NOVOVM_OVERLAY_ROUTE_MODE=fast`
2. `NOVOVM_OVERLAY_ROUTE_RELAY_BUCKETS=1`
3. `NOVOVM_OVERLAY_ROUTE_RELAY_SET_SIZE=1`
4. `NOVOVM_OVERLAY_ROUTE_RELAY_ROTATE_SECONDS=300`
5. `NOVOVM_OVERLAY_ROUTE_STRATEGY=direct`
6. `NOVOVM_OVERLAY_ROUTE_HOP_COUNT=1`
7. `NOVOVM_OVERLAY_ROUTE_MIN_HOPS=1`
8. `NOVOVM_OVERLAY_ROUTE_HOP_SLOT_SECONDS=300`

当前主线落标字段（node/gateway/plugin 同口径）：

1. `overlay_route_mode`
2. `overlay_route_region`
3. `overlay_route_relay_bucket`
4. `overlay_route_relay_set_size`
5. `overlay_route_relay_round`
6. `overlay_route_relay_index`
7. `overlay_route_relay_id`
