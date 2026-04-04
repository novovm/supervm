# NOVOVM 统一入口与运行手册（2026-03-22）

## 1. 目标

把“入口很多”的实际体验收口为一个运维入口：`scripts/novovm-up.ps1`。

## 2. 统一入口

Windows 统一命令：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-up.ps1 -Profile dev
```

生产模式：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-up.ps1 -Profile prod
```

仅连接外部 gateway（本机不拉起 gateway）：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-up.ps1 -Profile prod -NoGateway
```

## 3. 实际执行链

统一入口脚本内部调用既有主线：

1. 可选拉起 `evm-gateway`。  
2. 轮询 `artifacts/ingress/spool`。  
3. 收敛为 `.opsw1` 批次。  
4. 调用 `novovm-node` 执行 AOEM `ffi_v2` 路径。  

## 4. 环境默认值（脚本自动补齐）

1. `NOVOVM_NODE_MODE=full`  
2. `NOVOVM_EXEC_PATH=ffi_v2`  
3. `NOVOVM_HOST_ADMISSION=disabled`  
4. `NOVOVM_GATEWAY_SPOOL_DIR=artifacts/ingress/spool`  
5. `NOVOVM_GATEWAY_UA_STORE_BACKEND=rocksdb`  
6. `NOVOVM_GATEWAY_UA_STORE_PATH=artifacts/gateway/unified-account-router.rocksdb`  
7. `NOVOVM_GATEWAY_ETH_TX_INDEX_BACKEND=rocksdb`  
8. `NOVOVM_GATEWAY_ETH_TX_INDEX_PATH=artifacts/gateway/eth-tx-index.rocksdb`  
9. `NOVOVM_ADAPTER_PLUGIN_UA_STORE_BACKEND=rocksdb`  
10. `NOVOVM_ADAPTER_PLUGIN_UA_STORE_PATH=artifacts/migration/unifiedaccount/ua-plugin-self-guard-router.rocksdb`  
11. `NOVOVM_ADAPTER_PLUGIN_UA_AUDIT_BACKEND=rocksdb`  
12. `NOVOVM_ADAPTER_PLUGIN_UA_AUDIT_PATH=artifacts/migration/unifiedaccount/ua-plugin-self-guard-audit.rocksdb`  
13. `NOVOVM_ALLOW_NON_PROD_PLUGIN_BACKEND=0`  
14. `NOVOVM_OVERLAY_NODE_ID=<host>`  
15. `NOVOVM_OVERLAY_SESSION_ID=sess-<unix_ms>`  

## 5. 运行边界

1. 这是当前主线“生产可运行入口”，不是最终公网 daemon 形态。  
2. Linux 可以先用 `pwsh` 直接调用同一脚本；后续再补 native shell 启动器。  
3. 后续所有运维文档默认只给这一条入口，避免多入口分裂。  

## 6. 生产硬约束（`-Profile prod` 自动强制）

1. 强制 `NOVOVM_NODE_MODE=full`、`NOVOVM_EXEC_PATH=ffi_v2`、`NOVOVM_HOST_ADMISSION=disabled`。  
2. 强制 `NOVOVM_GATEWAY_UA_STORE_BACKEND=rocksdb`，并关闭 `NOVOVM_ALLOW_NON_PROD_UA_BACKEND`。  
3. 强制 `NOVOVM_GATEWAY_ETH_TX_INDEX_BACKEND=rocksdb`，并关闭 `NOVOVM_ALLOW_NON_PROD_GATEWAY_BACKEND`。  
4. 强制插件 UA store/audit 后端为 `rocksdb`，并关闭 `NOVOVM_ALLOW_NON_PROD_PLUGIN_BACKEND`。  
5. `memory`、`bincode`、`jsonl`、`none` 仅允许在显式非生产覆盖时启用。  

## 7. 四层网络角色化运行

统一入口已支持 `-RoleProfile full|l1|l2|l3`。  
详细命令与参数说明见：`docs_CN/NOVOVM-L1-L4-ROLE-PROFILES-RUNBOOK-2026-03-23.md`。

## 8. 一体化回补 daemon（gateway 内嵌生命周期）

统一入口已支持回补 daemon 参数：

1. `-EnableReconcileDaemon`
2. `-ReconcileSenderAddress`
3. `-ReconcileRpcEndpoint`
4. `-ReconcileIntervalSeconds`
5. `-ReconcileReplayMaxPerPayout`
6. `-ReconcileReplayCooldownSec`

说明：

1. 统一入口不再额外拉起独立回补 sidecar 进程。  
2. `run_gateway_node_pipeline.ps1` 会把回补参数注入 `novovm-evm-gateway`。  
3. 回补 daemon 由 `gateway` 进程内 supervisor 管理，与 gateway 同生命周期。  
4. `-NoGateway` 与 `-EnableReconcileDaemon` 不能同时使用。  
5. 生产主路径不再依赖 `powershell` 回补脚本执行器。  
6. 配置口径已统一支持 `NOVOVM_RECONCILE_*`（并兼容 `NOVOVM_GATEWAY_RECONCILE_*`）。  

示例：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-up.ps1 -Profile prod -RoleProfile l3 -Daemon -ReconcileSenderAddress 0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa -ReconcileRpcEndpoint http://127.0.0.1:9899
```

## 9. 公网节点生命周期编排（升级/回滚）

已提供统一生命周期脚本：`scripts/novovm-node-lifecycle.ps1`。  
支持动作：`register|start|stop|status|upgrade|rollback`。  
详细命令见：`docs_CN/NOVOVM-NODE-LIFECYCLE-UPGRADE-ROLLBACK-RUNBOOK-2026-04-03.md`。
