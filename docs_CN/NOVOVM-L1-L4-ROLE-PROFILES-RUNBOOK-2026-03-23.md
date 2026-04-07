# NOVOVM L1-L4 角色运行手册（2026-03-23）

## 1. 目标

把“四层网络”落到一个统一入口里运行，不拆成四个不同软件。  
主线命令：`novovmctl daemon`（前台单进程可用 `novovmctl up`）。

## 2. 角色参数

入口新增参数：

```powershell
--role-profile full|l1|l2|l3
```

说明：

1. `full`：默认。按现有主链路运行（gateway + node pipeline）。
2. `l1`：L1 运维形态（强制 `-NoGateway`，常驻消费模式）。
3. `l2`：L2 运维形态（强制 `-NoGateway`，常驻消费模式）。
4. `l3`：L3 运维形态（保留 gateway，常驻消费模式）。

当前版本是“角色编排”，不是拆成四个二进制程序。

## 3. 生产推荐命令（可直接执行）

### 3.1 全栈（单机闭环）

```powershell
novovmctl daemon --profile prod --role-profile full
```

### 3.2 L1 节点（不拉 gateway）

```powershell
novovmctl daemon --profile prod --role-profile l1
```

### 3.3 L2 节点（不拉 gateway）

```powershell
novovmctl daemon --profile prod --role-profile l2
```

### 3.4 L3 接入节点（拉 gateway）

```powershell
novovmctl daemon --profile prod --role-profile l3 --gateway-bind 0.0.0.0:9899
```

## 4. 自动注入的角色环境变量

脚本会写入：

1. `NOVOVM_NODE_ROLE_PROFILE=full|l1|l2|l3`
2. `NOVOVM_NETWORK_LAYER_HINT=L1-L4|L1|L2|L3`

并在启动时打印：

```text
novovm_up_profile: profile=... role=... no_gateway=... daemon=... use_node_watch_mode=... lean_io=...
```

## 5. 四层闭环锚点文件

生产模式默认开启：

1. `NOVOVM_L1L4_ANCHOR_PATH=artifacts/l1/l1l4-anchor.jsonl`
2. `NOVOVM_L1L4_ANCHOR_LEDGER_ENABLED=1`
3. `NOVOVM_L1L4_ANCHOR_LEDGER_KEY_PREFIX=ledger:l1:l1l4_anchor:v1:`

`novovm-node` 会按真实消费周期生成锚点，并同时：

1. 追加到本地锚点文件（审计）
2. 写入统一账本键空间（结算主线）

锚点记录字段包括：

1. `l4_ingress_ops`
2. `l3_routed_batches`
3. `l2_exec_ok_ops`
4. `l2_exec_failed_files`

这就是当前版本的“贡献-计量-L1锚点”最小闭环。

## 6. 常用扩展参数

```powershell
-PollMs 100 -SupervisorPollMs 1000 -NodeWatchBatchMaxFiles 2048 -SpoolDir artifacts/ingress/spool -GatewayMaxRequests 0
```

建议先保持默认，再按机器能力逐步调优。

## 7. 多机部署矩阵

多机（L1/L2/L3 分机）命令模板见：`docs_CN/NOVOVM-L1-L3-MULTI-NODE-PROD-MATRIX-2026-03-23.md`。  
可用生成脚本：`scripts/novovm-generate-role-matrix.ps1`（输出默认 `novovmctl daemon` 命令模板）。

## 8. 收益结算周期

按锚点生成结算凭据命令见：`docs_CN/NOVOVM-L1L4-SETTLEMENT-CYCLE-RUNBOOK-2026-03-23.md`。  
可用脚本：`scripts/novovm-l1l4-settlement-cycle.ps1`。

## 9. 自动收益发放

消费 voucher 生成发放指令命令见：`docs_CN/NOVOVM-L1L4-AUTO-PAYOUT-RUNBOOK-2026-03-23.md`。  
可用脚本：`scripts/novovm-l1l4-auto-payout.ps1`。

## 10. 到账执行状态

消费 dispatch 生成到账执行状态命令见：`docs_CN/NOVOVM-L1L4-PAYOUT-EXECUTE-RUNBOOK-2026-03-23.md`。  
可用脚本：`scripts/novovm-l1l4-payout-execute.ps1`。

## 11. 外部链回执确认

消费 executed 并查询外部链回执命令见：`docs_CN/NOVOVM-L1L4-EXTERNAL-CONFIRM-RUNBOOK-2026-03-23.md`。  
可用脚本：`scripts/novovm-l1l4-external-confirm.ps1`。

## 12. 真实签名与广播

消费 dispatch 并提交真实交易命令见：`docs_CN/NOVOVM-L1L4-REAL-BROADCAST-RUNBOOK-2026-03-23.md`。  
可用脚本：`scripts/novovm-l1l4-real-broadcast.ps1`。

## 13. 强一致回补

统一状态机（提交+确认+自动重放）命令见：`docs_CN/NOVOVM-L1L4-RECONCILE-RUNBOOK-2026-03-23.md`。  
可用脚本：`scripts/novovm-l1l4-reconcile.ps1`。

## 14. 回补状态机常驻

回补 daemon 常驻命令见：`docs_CN/NOVOVM-L1L4-RECONCILE-DAEMON-RUNBOOK-2026-03-23.md`。  
可用脚本：`scripts/novovm-l1l4-reconcile-daemon.ps1`。

## 15. 阶段外：统一入口内联回补 daemon（遗留兼容壳）

本节尚未纳入 `novovmctl` 主线入口面；以下命令仍为遗留兼容壳口径：

仓库内不再保留 `novovm-up.ps1` 的可执行示例命令。

主线入口仍为 `novovmctl daemon --profile prod --role-profile l3`。

如确需阶段外回补一体化口径，需在遗留兼容环境中单独确认，不纳入主线命令面。

常用参数：

```powershell
-EnableReconcileDaemon -ReconcileIntervalSeconds 15 -ReconcileReplayMaxPerPayout 3 -ReconcileReplayCooldownSec 30
```

约束：

1. 该模式要求本机由统一入口拉起 gateway（不能同时 `-NoGateway`）。  
2. 统一入口不再启动独立回补 sidecar 进程。  
3. 回补状态机主路径已改为 gateway 内嵌 Rust 循环，不依赖 powershell 执行器。  
4. 回补环境变量支持统一前缀 `NOVOVM_RECONCILE_*`（并兼容旧前缀）。  
5. 覆盖层寻址增强支持 `NOVOVM_OVERLAY_NODE_ID`、`NOVOVM_OVERLAY_SESSION_ID`、`NOVOVM_OVERLAY_ROUTE_ID`、`NOVOVM_OVERLAY_ROUTE_SEED`、`NOVOVM_OVERLAY_ROUTE_EPOCH_SECONDS`、`NOVOVM_OVERLAY_ROUTE_MASK_BITS`（默认按 epoch 轮换 route_id）。  
6. 覆盖层路由模式开关支持 `NOVOVM_OVERLAY_ROUTE_MODE=secure|fast`（统一入口可用 `-OverlayRouteMode secure|fast`）。  
7. `secure` 模式默认：`multi_hop`、`hop_count>=3`、`min_hops>=2`、`hop_slot_seconds=30`。  
8. `fast` 模式默认：`direct`、`hop_count=1`、`min_hops=1`、`hop_slot_seconds=300`。  
9. 仍支持细粒度参数 `NOVOVM_OVERLAY_ROUTE_STRATEGY`、`NOVOVM_OVERLAY_ROUTE_HOP_COUNT`、`NOVOVM_OVERLAY_ROUTE_ENFORCE_MULTI_HOP`、`NOVOVM_OVERLAY_ROUTE_MIN_HOPS`、`NOVOVM_OVERLAY_ROUTE_HOP_SLOT_SECONDS`（模式优先于策略默认值）。  
10. 新增分流参数：`NOVOVM_OVERLAY_ROUTE_REGION`（默认 `global`）、`NOVOVM_OVERLAY_ROUTE_RELAY_BUCKETS`（`secure` 默认 `8`，`fast` 默认 `1`）、`NOVOVM_OVERLAY_ROUTE_RELAY_SET_SIZE`（`secure` 默认 `3`，`fast` 默认 `1`）、`NOVOVM_OVERLAY_ROUTE_RELAY_ROTATE_SECONDS`（`secure` 默认 `60`，`fast` 默认 `300`）。  
11. gateway 侧 `eth_sendRawTransaction/eth_sendTransaction/web30_sendRawTransaction/web30_sendTransaction` 返回体已同口径输出 `overlay_route_id/overlay_route_epoch/overlay_route_mask_bits/overlay_route_mode/overlay_route_region/overlay_route_relay_bucket/overlay_route_relay_set_size/overlay_route_relay_round/overlay_route_relay_index/overlay_route_relay_id/overlay_route_strategy/overlay_route_hop_count`。  
12. gateway 侧 `evm_snapshotPendingIngress/evm_snapshotExecutableIngress/evm_drainPendingIngress/evm_drainExecutableIngress` 结果已同口径输出 `overlay_route_id/overlay_route_epoch/overlay_route_mask_bits/overlay_route_mode/overlay_route_region/overlay_route_relay_bucket/overlay_route_relay_set_size/overlay_route_relay_round/overlay_route_relay_index/overlay_route_relay_id/overlay_route_strategy/overlay_route_hop_count`，且字段来自 ingress frame 原生记录。  
13. plugin 侧 UA 审计记录（`ua-plugin-self-guard-audit.jsonl` / RocksDB）已同口径输出 `overlay_route_id/overlay_route_epoch/overlay_route_mask_bits/overlay_route_mode/overlay_route_region/overlay_route_relay_bucket/overlay_route_relay_set_size/overlay_route_relay_round/overlay_route_relay_index/overlay_route_relay_id/overlay_route_strategy/overlay_route_hop_count`。  
