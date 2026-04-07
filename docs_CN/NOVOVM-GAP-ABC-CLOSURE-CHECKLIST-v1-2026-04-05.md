# NOVOVM Gap-A/B/C 收口清单（v1，2026-04-05）

## 1. 目的

把 Gap-A、Gap-B、Gap-C 从“未完成差距”收口为“当前版本已闭环”，统一生产口径。

## 2. Gap-A 收口（节点生命周期与灰度控制面）

1. 主线控制面脚本：`scripts/novovm-node-rollout-control.ps1`。  
2. 主线模板文件：`config/runtime/lifecycle/rollout.queue.json`。  
3. 主线运行手册：`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
4. 收口标准：主线路径统一为“单控制面模板 + 热重载参数”，后续仅做参数优化，不再引入并行链路。

## 3. Gap-B 收口（四层闭环与回补）

1. 回补模板文件：`config/runtime/lifecycle/reconcile.runtime.json`。  
2. 主线入口：`scripts/novovm-up.ps1`。  
3. gateway 内嵌回补：`crates/gateways/evm-gateway/src/main.rs`。  
4. 收口标准：回补运行、参数模板、生命周期入口三者已统一，后续仅做模板调参与环境差异压平。

## 4. Gap-C 收口（覆盖层寻址增强）

1. 模式开关：`NOVOVM_OVERLAY_ROUTE_MODE=secure|fast`。  
2. 强约束：`enforce_multi_hop + min_hops + hop_slot_seconds`。  
3. 分流参数：`NOVOVM_OVERLAY_ROUTE_REGION`、`NOVOVM_OVERLAY_ROUTE_RELAY_BUCKETS`、`NOVOVM_OVERLAY_ROUTE_RELAY_SET_SIZE`、`NOVOVM_OVERLAY_ROUTE_RELAY_ROTATE_SECONDS`。  
4. 落标字段：`overlay_route_mode/overlay_route_region/overlay_route_relay_bucket/overlay_route_relay_set_size/overlay_route_relay_round/overlay_route_relay_index/overlay_route_relay_id` 已进入锚点、gateway 返回、plugin 审计。  
5. ingress frame 原生化：`EvmMempoolIngressFrameV1` 已内置上述字段，gateway 快照直接消费 frame 字段（无临时推导兜底路径依赖）。  
6. 收口标准：模式治理、候选集轮换与落标口径已闭环，后续真实中继网络增强归入 P2。

## 5. 后续增强定义（不再计入 Gap）

1. Gap-A 后续：阈值与区域策略持续优化。  
2. Gap-B 后续：按区域和负载优化回补模板参数。  
3. Gap-C 后续：逐步引入真实多跳中继网络能力。  
