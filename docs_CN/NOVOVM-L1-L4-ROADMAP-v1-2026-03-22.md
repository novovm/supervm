# NOVOVM 四层网络落地路线图 v1（2026-03-22）

## 1. 原则

1. 功能闭环优先，不做工程化表演。  
2. 先可运行，再扩展性能与隐私。  
3. 设计目标和实现状态分开写，避免“文档已实现”错觉。  

## 2. 分层目标

1. L1：最终性、仲裁、治理参数锚定。  
2. L2：执行与证明算力层。  
3. L3：接入、路由、缓存、聚合。  
4. L4：钱包、终端、轻节点、设备侧 SDK。  

## 3. 三阶段落地

## Phase-0（当前到可控生产）
1. 固化入口：统一脚本启动 gateway+node 主线。  
2. 固化边界：外部 JSON-RPC，内部二进制 pipeline。  
3. 固化账户：统一账户路由与策略持久化主线。  

完成标志：可持续接入、可恢复、可审计。

## Phase-1（四层最小闭环）
1. L4 轻节点上报基础贡献指标。  
2. L3 路由层形成可计量转发与可验证日志。  
3. L2 执行与证明任务形成分工统计。  
4. L1 记录贡献结算锚点（先中心化结算，再去中心化）。  

完成标志：贡献-计量-结算单周期闭环可跑。

## Phase-2（覆盖层寻址与隐私增强）
1. 在 IP 传输之上增加 NodeID/SessionID 覆盖层。  
2. 逐步减少固定地址暴露，支持多跳路由策略。  
3. 完成抗分析增强基线（不是“不可追踪”承诺）。  
4. 主线已补充 route 轮换标识：`overlay_route_id/overlay_route_epoch/overlay_route_mask_bits`（node 锚点 + gateway 入站返回同口径落地）。
5. 主线已补充多跳策略参数：`overlay_route_strategy/overlay_route_hop_count`（当前为可治理参数化，不改动底层 IP 兼容传输）。
6. 主线已补充多跳强约束参数：`overlay_route_enforce_multi_hop/overlay_route_min_hops`（prod 入口默认强制 multi_hop 并抬高最小跳数）。
7. 主线已补充多跳细粒度轮换参数：`overlay_route_hop_slot_seconds`（在 epoch 内继续按 hop slot 轮换 route_id，降低路径可关联性）。
8. 主线已补充区域与中继桶分流参数：`overlay_route_region/overlay_route_relay_bucket`（node 锚点 + gateway 返回 + plugin 审计同口径）。
9. 主线已补充中继候选集轮换参数：`overlay_route_relay_set_size/overlay_route_relay_round/overlay_route_relay_index/overlay_route_relay_id`（锚点 + gateway 返回/快照 + plugin 审计同口径）。

完成标志：在不破坏兼容性的前提下显著提升寻址弹性与隐私强度。

## 4. 关键约束

1. 不承诺“完全摆脱 IP”作为短期目标。  
2. 不承诺“传统互联网下绝对不可追踪”。  
3. 不为演示引入大量模拟环境替代真实链路。  

## 5. 近期执行清单

1. 统一入口只保留一套 runbook。  
2. 统一账户持久化作为 P0。  
3. L3 路由贡献计量原型作为 P1。  
4. 覆盖层寻址 PoC 作为 P1。  

## 6. 当前可执行入口（2026-03-23 更新）

1. `scripts/novovm-up.ps1` 已支持 `-RoleProfile full|l1|l2|l3`。  
2. 角色化运行手册：`docs_CN/NOVOVM-L1-L4-ROLE-PROFILES-RUNBOOK-2026-03-23.md`。  
3. 生产模式默认写入四层闭环锚点：`artifacts/l1/l1l4-anchor.jsonl`。  
