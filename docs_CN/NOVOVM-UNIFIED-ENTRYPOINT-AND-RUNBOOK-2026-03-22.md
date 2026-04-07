# NOVOVM 统一入口与运行手册（2026-03-22）

## 1. 目标

把“入口很多”的实际体验收口为一个 Rust 运维入口：`novovmctl`。

## 2. 统一入口

前台节点统一命令：

```powershell
novovmctl up --profile dev
```

生产模式：

```powershell
novovmctl up --profile prod
```

生产模式（显式选择覆盖层路由模式）：

```powershell
novovmctl up --profile prod -OverlayRouteMode secure
```

仅连接外部 gateway（本机不拉起 gateway）：

```powershell
novovmctl up --profile prod -NoGateway
```

## 3. 实际执行链

统一 Rust 入口按固定三层主线执行：

1. `novovmctl` 负责参数整理、默认路径发现、统一输出与审计。  
2. `novovm-rollout-policy` 负责主线策略判断。  
3. `novovm-node` 负责节点执行与消费链。  
4. `scripts/*.ps1` 仅保留遗留兼容壳，不再是主线入口。  

补充口径：

1. 单机/节点主线入口已统一为 `novovmctl`：前台用 `novovmctl up`，守护用 `novovmctl daemon`。  
2. 灰度控制面主线入口已统一为 `novovmctl rollout-control`；策略内核统一由 `novovm-rollout-policy` 承接。  
3. 正常主路径只允许通过 `novovm-rollout-policy` 进入策略内核；legacy 平铺 tool 名仅保留兼容口径，内部也直接分发到统一共享模块。  
4. PowerShell 不再承载主线入口或完整策略实现，只保留遗留兼容壳、环境注入、审计转发与 emergency fallback。  

## 4. 环境默认值（兼容映射参考，主线 CLI 以 `novovmctl --help` 为准）

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
16. `NOVOVM_OVERLAY_ROUTE_MODE=secure|fast`（`prod` 默认 `secure`，其他 profile 默认 `fast`）  
17. `NOVOVM_OVERLAY_ROUTE_REGION=global`  
18. `NOVOVM_OVERLAY_ROUTE_RELAY_BUCKETS`（`secure` 默认 `8`，`fast` 默认 `1`）  
19. `NOVOVM_OVERLAY_ROUTE_RELAY_SET_SIZE`（`secure` 默认 `3`，`fast` 默认 `1`）  
20. `NOVOVM_OVERLAY_ROUTE_RELAY_ROTATE_SECONDS`（`secure` 默认 `60`，`fast` 默认 `300`）  
21. `NOVOVM_OVERLAY_ROUTE_RELAY_CANDIDATES`（可选，逗号/分号分隔 relay_id；配置后 `overlay_route_relay_id` 优先从候选集中选取）  
22. `overlay.route.runtime.json` 模板字段：`relay_candidates_by_region`（按区域候选集覆盖）  
23. `overlay.route.runtime.json` 模板字段：`relay_candidates_by_role`（按角色候选集覆盖）  
24. `-OverlayRouteRuntimeFile`（默认 `config/runtime/lifecycle/overlay.route.runtime.json`）  
25. `-OverlayRouteRuntimeProfile`（默认跟随 `-Profile`）  
26. `-OverlayRouteRelayCandidates`（可选，计划级/命令级显式下发候选集，优先于模板）  
27. `-OverlayRouteRelayCandidatesByRegion`（可选，JSON 映射：按 region 选择候选集）  
28. `-OverlayRouteRelayCandidatesByRole`（可选，JSON 映射：按 role 选择候选集）  
29. `-OverlayRouteRelayDirectoryFile`（可选，真实中继目录文件）  
30. `-OverlayRouteRelayHealthMin`（可选，健康阈值，范围 `0..1`）  
31. `-OverlayRouteRelayPenaltyStateFile`（可选，惩罚状态文件）  
32. `-OverlayRouteRelayPenaltyDelta`（可选，JSON 惩罚增量映射）  
33. `-OverlayRouteRelayPenaltyRecoverPerRun`（可选，每次运行恢复步长，范围 `0..1`）  
34. `-EnableAutoProfile`（可选，启用 Rust `novovm-overlay-auto-profile` 自动选择 `prod-cn/prod-eu/prod-us`）  
35. `-AutoProfileStateFile`（可选，自动 profile 状态文件）  
36. `-AutoProfileProfiles`（可选，候选 profile 列表，逗号/分号分隔）  
37. `-AutoProfileMinHoldSeconds`（可选，最小持有秒数）  
38. `-AutoProfileSwitchMargin`（可选，切换分差阈值，范围 `0..1`）  
39. `-AutoProfileSwitchbackCooldownSeconds`（可选，回切冷却秒数）  
40. `-AutoProfileRecheckSeconds`（可选，决策重算周期秒数）  
41. `-AutoProfileBinaryPath`（可选，Rust selector 二进制路径；未指定时优先 `target/release/novovm-overlay-auto-profile(.exe)`，否则回退 `cargo run`）  

## 5. 运行边界

1. 当前主线“生产可运行入口”已固定为 `novovmctl up|daemon`。  
2. Linux/macOS/Windows 主线路径统一走 `novovmctl`；`.ps1` 只保留遗留兼容壳。  
3. 后续所有运维文档默认只给 `novovmctl` 入口，避免多入口分裂。  

## 6. 生产硬约束（`-Profile prod` 自动强制）

1. 强制 `NOVOVM_NODE_MODE=full`、`NOVOVM_EXEC_PATH=ffi_v2`、`NOVOVM_HOST_ADMISSION=disabled`。  
2. 强制 `NOVOVM_GATEWAY_UA_STORE_BACKEND=rocksdb`，并关闭 `NOVOVM_ALLOW_NON_PROD_UA_BACKEND`。  
3. 强制 `NOVOVM_GATEWAY_ETH_TX_INDEX_BACKEND=rocksdb`，并关闭 `NOVOVM_ALLOW_NON_PROD_GATEWAY_BACKEND`。  
4. 强制插件 UA store/audit 后端为 `rocksdb`，并关闭 `NOVOVM_ALLOW_NON_PROD_PLUGIN_BACKEND`。  
5. 生产默认 `NOVOVM_OVERLAY_ROUTE_MODE=secure`（可显式传 `-OverlayRouteMode fast` 切换快速模式）。  
6. `secure` 模式默认强制多跳：`NOVOVM_OVERLAY_ROUTE_STRATEGY=multi_hop`、`NOVOVM_OVERLAY_ROUTE_ENFORCE_MULTI_HOP=1`。  
7. `secure` 模式默认门槛：`NOVOVM_OVERLAY_ROUTE_HOP_COUNT>=3`、`NOVOVM_OVERLAY_ROUTE_MIN_HOPS>=2`，`NOVOVM_OVERLAY_ROUTE_HOP_SLOT_SECONDS=30`。  
8. `secure` 模式默认区域分流：`NOVOVM_OVERLAY_ROUTE_REGION=global`、`NOVOVM_OVERLAY_ROUTE_RELAY_BUCKETS=8`（按 route_id + region 计算 relay bucket），并启用中继候选集轮换：`NOVOVM_OVERLAY_ROUTE_RELAY_SET_SIZE=3`、`NOVOVM_OVERLAY_ROUTE_RELAY_ROTATE_SECONDS=60`。  
9. 若设置 `NOVOVM_OVERLAY_ROUTE_RELAY_CANDIDATES`，则 `overlay_route_relay_id` 优先按候选集选取；未设置时回退为 `rly:<region>:<bucket>:<index>`。  
10. 模板候选集优先级：`relay_candidates_by_region[region]` > `relay_candidates_by_role[role]` > `relay_candidates`。  
11. `fast` 模式默认直连：`NOVOVM_OVERLAY_ROUTE_STRATEGY=direct`、`NOVOVM_OVERLAY_ROUTE_HOP_COUNT=1`、`NOVOVM_OVERLAY_ROUTE_MIN_HOPS=1`，`NOVOVM_OVERLAY_ROUTE_HOP_SLOT_SECONDS=300`、`NOVOVM_OVERLAY_ROUTE_RELAY_BUCKETS=1`，并固定单候选：`NOVOVM_OVERLAY_ROUTE_RELAY_SET_SIZE=1`、`NOVOVM_OVERLAY_ROUTE_RELAY_ROTATE_SECONDS=300`。  
12. 生产默认拒绝 gateway 公网直绑（`GatewayBind` 必须是 loopback），避免节点位置直接暴露；如确需公网监听，必须显式传 `-AllowPublicGatewayBind`。  
13. `memory`、`bincode`、`jsonl`、`none` 仅允许在显式非生产覆盖时启用。  
14. 覆盖层参数支持 runtime 模板化（`overlay.route.runtime.json`），优先级：CLI 显式 mode > 覆盖层模板 > 脚本默认值。  

## 7. 四层网络角色化运行

统一入口已支持 `--role-profile full|l1|l2|l3`。  
详细命令与参数说明见：`docs_CN/NOVOVM-L1-L4-ROLE-PROFILES-RUNBOOK-2026-03-23.md`。

## 7.1 覆盖层路由 runtime 模板（新增）

统一入口已支持覆盖层模板化参数：

1. `-OverlayRouteRuntimeFile`
2. `-OverlayRouteRuntimeProfile`
3. `-OverlayRouteRelayCandidates`（命令级候选集覆盖）

默认模板文件：`config/runtime/lifecycle/overlay.route.runtime.json`。  
典型用途：按 profile 统一治理 `region/relay_buckets/relay_set_size/relay_rotate_seconds/relay_candidates/relay_candidates_by_region/relay_candidates_by_role`，避免逐机手工注入环境变量。控制面场景可复用同一模板中的 `auto_penalty_enabled/auto_penalty_step`、`relay_health_refresh_*`、`relay_discovery_*`（含 `relay_discovery_http_urls/relay_discovery_http_urls_file/relay_discovery_seed_region/relay_discovery_seed_mode/relay_discovery_seed_profile/relay_discovery_seed_failover_state_file/relay_discovery_seed_priority/relay_discovery_seed_success_rate_threshold/relay_discovery_seed_cooldown_seconds/relay_discovery_seed_max_consecutive_failures/relay_discovery_region_priority/relay_discovery_region_failover_threshold/relay_discovery_region_cooldown_seconds/relay_discovery_relay_score_smoothing_alpha/relay_discovery_source_weights/relay_discovery_http_timeout_ms/relay_discovery_source_reputation_file/relay_discovery_source_decay/relay_discovery_source_penalty_on_fail/relay_discovery_source_recover_on_success/relay_discovery_source_blacklist_threshold/relay_discovery_source_denylist`）作为失败重试自动惩罚/目录探活刷新/多源发现合并、seed 故障切换与区域 failover 默认值。  
推荐生产 profile：`prod-cn`、`prod-eu`、`prod-us`（分别针对 CN 稳态抗抖、EU 均衡、US 快速切换）。
命令级覆盖优先级：`-OverlayRouteRelayCandidates` > `-OverlayRouteRelayCandidatesByRegion` > `-OverlayRouteRelayCandidatesByRole` > `-OverlayRouteRelayDirectoryFile/-OverlayRouteRelayHealthMin`（经 `PenaltyState/PenaltyDelta/PenaltyRecoverPerRun` 修正）> 模板候选集字段。  

示例：

```powershell
novovmctl up --profile prod -OverlayRouteRuntimeFile .\config\runtime\lifecycle\overlay.route.runtime.json -OverlayRouteRuntimeProfile prod
```

## 7.2 Auto Profile v0（Rust selector，默认关闭）

启用后，`novovmctl up` 会在加载 `overlay.route.runtime.json` 前通过统一策略入口 `novovm-rollout-policy` 执行 auto-profile 选择；入口层只做参数透传与启动编排。

示例（单机开启自动 profile）：

```powershell
novovmctl up --profile prod -EnableAutoProfile -OverlayRouteRuntimeFile .\config\runtime\lifecycle\overlay.route.runtime.json
```

示例（显式指定 selector 二进制）：

```powershell
novovmctl up --profile prod -EnableAutoProfile -AutoProfileBinaryPath .\target\release\novovm-overlay-auto-profile.exe
```

## 8. 阶段外：一体化回补 daemon（遗留兼容壳）

本节尚未纳入 `novovmctl` 主线入口面；以下命令仍为遗留兼容壳口径：

1. `-EnableReconcileDaemon`
2. `-ReconcileSenderAddress`
3. `-ReconcileRpcEndpoint`
4. `-ReconcileIntervalSeconds`
5. `-ReconcileReplayMaxPerPayout`
6. `-ReconcileReplayCooldownSec`
7. `-ReconcileRuntimeFile`
8. `-ReconcileRuntimeProfile`

说明：

1. 统一入口不再额外拉起独立回补 sidecar 进程。  
2. `run_gateway_node_pipeline.ps1` 会把回补参数注入 `novovm-evm-gateway`。  
3. 回补 daemon 由 `gateway` 进程内 supervisor 管理，与 gateway 同生命周期。  
4. `-NoGateway` 与 `-EnableReconcileDaemon` 不能同时使用。  
5. 生产主路径不再依赖 `powershell` 回补脚本执行器。  
6. 配置口径已统一支持 `NOVOVM_RECONCILE_*`（并兼容 `NOVOVM_GATEWAY_RECONCILE_*`）。  
7. 统一入口已支持回补模板：默认读取 `config/runtime/lifecycle/reconcile.runtime.json`。  
8. 模板优先级：CLI 显式参数 > 模板参数 > 脚本默认值。  
9. 模板支持按 profile 选择（默认跟随 `-Profile`，也可用 `-ReconcileRuntimeProfile` 指定）。  

示例：

```powershell
novovmctl up --profile prod -RoleProfile l3 -Daemon -ReconcileSenderAddress 0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa -ReconcileRpcEndpoint http://127.0.0.1:9899
```

模板化示例（推荐生产运维）：

```powershell
novovmctl up --profile prod -RoleProfile l3 -Daemon -ReconcileRuntimeFile .\config\runtime\lifecycle\reconcile.runtime.json -ReconcileRuntimeProfile prod
```

## 9. 公网节点生命周期编排（升级/回滚）

主线入口：`novovmctl lifecycle`；`scripts/novovm-node-lifecycle.ps1` 仅保留遗留兼容壳。  
支持动作：`register|start|stop|status|upgrade|rollback`。  
详细命令见：`docs_CN/NOVOVM-NODE-LIFECYCLE-UPGRADE-ROLLBACK-RUNBOOK-2026-04-03.md`。
