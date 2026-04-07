# NOVOVM 主线现状-目标-差距清单（打勾版，更新于 2026-04-07）

## 1. 范围说明

本文只描述仓库主线的功能落地状态，不讨论营销叙事。  
口径以“真实生产链路可运行”为准，不以模拟环境和表演型测试作为完成标准。

## 2. 主线完成度总览

### 严格收口口径（2026-04-07）

1. `Rust policy core`：完成。  
2. `Rust runtime shell`：完成。  
3. `PowerShell mainline entry removal`：未完成；主线唯一入口仍需彻底收口到 `novovmctl`。  

1. [x] 单一可运维入口已收口到 `novovmctl`；`scripts/*.ps1` 仅保留遗留兼容壳。  
2. [x] 统一账户持久化主线已收口为 `rocksdb`（gateway/plugin 双侧生产硬约束）。  
3. [x] 统一账户一键生产操作命令已具备（backup/restore/migrate）。  
4. [x] `novovm-node` 常驻消费模式已具备（watch + daemon + lean I/O）。  
5. [x] 四层角色化运行已具备（`-RoleProfile full|l1|l2|l3`，同一程序不同角色）。  
6. [x] 四层最小闭环已具备：L4/L3/L2 真实消费计量写入 L1 锚点文件。  
7. [x] 公网常驻节点生命周期产品化最小版本已完成（版本注册、常驻启动、升级、回滚编排）。  
8. [x] L1/L2/L3 多机部署参数模板已完成（角色矩阵脚本+文档）。  
9. [x] 覆盖层寻址（NodeID/SessionID）最小可用版本已完成（gateway 入站记录 + node 锚点双侧落标识，底层仍兼容 IP 传输）。  
10. [x] 周期化收益结算最小版本已完成（按锚点汇总生成 voucher 凭据）。
11. [x] 自动收益发放最小版本已完成（消费 voucher 自动产出发放指令）。
12. [x] 链上到账执行最小版本已完成（消费 dispatch 自动产出 executed 到账状态）。
13. [x] 外部链回执确认最小版本已完成（RPC 回执落库 + 重放）。
14. [x] 真实签名与广播最小版本已完成（消费 dispatch 调用 RPC 提交交易）。
15. [x] 广播提交-回执确认强一致回补最小版本已完成（统一状态机 + 自动重放）。
16. [x] 回补状态机服务化常驻最小版本已完成（daemon 循环执行）。
17. [x] 回补状态机主入口一体化最小版本已完成（`novovm-up` 同生命周期拉起）。
18. [x] 回补状态机 gateway 二进制生命周期内嵌最小版本已完成（由 `novovm-evm-gateway` 进程内拉起并守护）。
19. [x] 回补状态机“纯二进制逻辑化”已完成（移除对 `powershell` 回补脚本执行器的主路径依赖）。
20. [x] 回补配置口径统一已完成（支持 `NOVOVM_RECONCILE_*`，兼容 `NOVOVM_GATEWAY_RECONCILE_*`）。
21. [x] 节点生命周期治理增强最小版本已完成（runtime template 收口 + node group 升级保护）。
22. [x] 外层自动灰度编排控制器最小版本已完成（按组推进 + 失败阈值 + 可选自动回滚）。
23. [x] 灰度编排跨主机执行适配已完成（local/ssh/winrm 统一计划驱动）。
24. [x] 灰度编排执行认证与审计追踪最小版本已完成（controller 准入 + SSH/WinRM 凭据托管 + jsonl 审计）。
25. [x] 灰度集中调度控制面最小版本已完成（多计划队列 + 并发限流 + 跨区域窗口编排）。
26. [x] 灰度控制面策略编排增强已完成（优先级抢占 + 区域容量配额 + 自动重试退避）。
27. [x] 灰度控制面策略学习化调度最小版本已完成（失败率 EMA + 区域拥塞动态调参）。
28. [x] 灰度控制面多控制器一致性治理最小版本已完成（主备仲裁 + 去重执行）。
29. [x] 灰度控制面跨站点控制器共识最小版本已完成（异地冲突仲裁 + 全局幂等）。
30. [x] 灰度控制面跨站点状态复制与恢复最小版本已完成（状态快照 + 断点恢复 + 冲突回放）。
31. [x] 灰度控制面跨机房双写容灾与副本一致性校验最小版本已完成（状态双写 + 周期校验 + 滞后阈值）。
32. [x] 灰度控制面副本健康分级与自动切主最小版本已完成（健康状态文件 + 冷却切主 + 二次校验）。
33. [x] 灰度控制面自动回切与异地恢复演练模板最小版本已完成（稳定回切 + drill 审计模板）。
34. [x] 灰度控制面副本健康 SLO 门槛与自动阻断最小版本已完成（滚动评分 + 阈值拦截）。
35. [x] 灰度控制面 SLO 分级熔断最小版本已完成（黄灯限流 + 红灯硬阻断）。
36. [x] 灰度控制面 SLO 多级熔断矩阵最小版本已完成（按 score 分段限流/阻断）。
37. [x] 灰度控制面 SLO 自适应阈值最小版本已完成（按健康趋势动态偏移熔断触发线）。
38. [x] 灰度控制面跨站点自动切主策略矩阵最小版本已完成（按 source/grade/site priority 决策切主）。
39. [x] 灰度控制面容灾演练自动评分最小版本已完成（滚动评分 + 通过率趋势）。
40. [x] 灰度控制面切主策略与 SLO/演练评分联动最小版本已完成（门槛不足自动阻断切主）。
41. [x] 灰度控制面跨站点冲突自动判责与降权矩阵最小版本已完成（冲突归因 + 责任站点处罚/恢复）。
42. [x] 灰度控制面跨站点信誉分长期治理最小版本已完成（按时间衰减自动恢复降权）。
43. [x] 灰度控制面跨站点信誉分多周期风险预测与自动限流联动最小版本已完成（风险升高自动收紧并发/派发）。
44. [x] 灰度控制面高风险站点赢家保护与切主风险联动最小版本已完成（高风险站点禁止赢得共识或触发切主）。
45. [x] 灰度控制面统一风险阻断矩阵基线最小版本已完成（winner_guard/risk_link 同源默认并可分别覆盖）。
46. [x] 灰度控制面风险策略角色矩阵最小版本已完成（winner/failover 角色默认阻断等级独立可配）。
47. [x] 灰度控制面风险动作矩阵最小版本已完成（按风险等级映射并发/发车/阻断动作）。
48. [x] 灰度控制面风险动作矩阵来源分层最小版本已完成（startup/cycle 分层治理并保留 `*` 默认规则）。
49. [x] 灰度控制面风险动作矩阵三级覆盖最小版本已完成（global->region->site 分层覆盖）。
50. [x] 灰度控制面风险阻断等级三级覆盖最小版本已完成（winner_guard/risk_link 支持 global->region->site 覆盖）。
51. [x] 灰度控制面切主策略矩阵三级覆盖最小版本已完成（failover policy 支持 global->region->site 覆盖）。
52. [x] 灰度控制面切主联动门槛三级覆盖最小版本已完成（slo_link/drill_link/risk_link 门槛支持 global->region->site 覆盖）。
53. [x] 灰度控制面站点优先级三级覆盖最小版本已完成（site priority 支持 global->region->site 覆盖）。
54. [x] 灰度控制面风险动作矩阵与站点优先级联动最小版本已完成（action_matrix 支持 min_site_priority 门槛并保守阻断）。
55. [x] 灰度控制面风险动作策略变更审计聚合最小版本已完成（`site_risk_throttle_policy` 按来源去重落审计）。
56. [x] 灰度控制面灰度决策摘要审计聚合最小版本已完成（`rollout_decision_summary` 汇总决策并按来源去重）。
57. [x] 灰度控制面灰度决策摘要角色告警分级最小版本已完成（L1/L2/L3 按固定级别输出 `decision_alert_level`）。
58. [x] 灰度控制面灰度决策摘要告警通道映射最小版本已完成（输出 `decision_alert_channel` 供一跳分发）。
59. [x] 灰度控制面灰度决策摘要告警目标映射最小版本已完成（输出 `decision_alert_target` 直接指向通知目标 ID）。
60. [x] 灰度控制面灰度决策摘要投递类型映射最小版本已完成（输出 `decision_delivery_type/decision_delivery_action`）。
61. [x] 灰度控制面灰度决策摘要真实投递执行最小版本已完成（输出 `rollout_decision_delivery`，支持 webhook/im/email）。
62. [x] 灰度控制面风险策略模板化与热切换参数收口最小版本已完成（`active_profile/policy_profiles/hot_reload` + 运行中自动刷新）。
63. [x] 覆盖层寻址增强版最小版本已完成（锚点新增 `overlay_route_id/overlay_route_epoch/overlay_route_mask_bits`，支持 seed+epoch+mask 轮换）。
64. [x] 覆盖层寻址增强版 gateway 同口径落地已完成（入站返回与 spool 记录补齐 `overlay_route_id/overlay_route_epoch/overlay_route_mask_bits`）。
65. [x] 覆盖层寻址第二阶段参数化最小版本已完成（`overlay_route_strategy/overlay_route_hop_count` 已在 node+gateway 同口径落地）。
66. [x] 覆盖层寻址第二阶段 ingress frame 同口径最小版本已完成（`EvmMempoolIngressFrameV1` 与 gateway ingress 快照已补齐 `overlay_route_* + strategy/hop_count`）。
67. [x] 覆盖层寻址第二阶段审计链路同口径最小版本已完成（plugin UA audit 记录已补齐 `overlay_route_* + strategy/hop_count`）。
68. [x] 跨站点信誉分前瞻预警与自动治理最小版本已完成（按风险趋势外推预警，并可提前触发风险动作矩阵治理）。
69. [x] 容灾闭环策略自动收敛最小版本已完成（failover 模式自动收敛并发/发车节奏，红灯可阻断派发）。
70. [x] 容灾收敛阈值区域化细化最小版本已完成（`failover_converge` 支持 global->region->site 覆盖，并落审计 `converge_scope`）。
71. [x] 容灾收敛参数运行中热重载最小版本已完成（queue 变更可即时刷新 `failover_converge` 并落审计 `replica_failover_converge_hot_reload`）。
72. [x] 容灾收敛策略模板联动最小版本已完成（`policy_profiles.*.failover_converge` 随 `active_profile` 切换并参与热重载）。
73. [x] 灰度决策摘要收敛参数透出最小版本已完成（`rollout_decision_summary` 输出 active_profile + failover_converge 生效参数）。
74. [x] 灰度决策投递链路收敛参数透出最小版本已完成（`rollout_decision_delivery` 同步输出 active_profile + failover_converge 生效参数）。
75. [x] 灰度决策审计下游导出最小版本已完成（`rollout_decision_summary/delivery` 归一化导出 dashboard jsonl）。
76. [x] 灰度决策审计导出控制面内置周期化最小版本已完成（queue 配置启用 + 热重载生效 + 审计落地）。
77. [x] 灰度决策看板消费端内置周期化最小版本已完成（状态快照 + 阻断告警文件 + 热重载生效）。
78. [x] 回补参数治理模板化最小版本已完成（统一入口支持 `reconcile.runtime.json` + profile 收口）。
79. [x] 覆盖层多跳强约束最小版本已完成（`enforce_multi_hop + min_hops` 三侧同口径 + prod 入口默认收紧）。
80. [x] 覆盖层多跳细粒度轮换最小版本已完成（`hop_slot_seconds` 三侧同口径参与 route_id 轮换）。
81. [x] 生产入口公网暴露保护最小版本已完成（prod 默认拒绝 gateway 公网直绑，需显式 override）。
82. [x] 覆盖层路由模式开关最小版本已完成（`secure|fast` 双模式，统一入口可切换）。
83. [x] 覆盖层路由模式落标最小版本已完成（锚点/gateway 返回/plugin 审计同口径输出 `overlay_route_mode`）。
84. [x] Gap-A 当前版本收口已完成（按控制面模板与运行手册冻结为生产基线）。
85. [x] Gap-B 当前版本收口已完成（四层闭环与回补模板参数已定版）。
86. [x] Gap-C 当前版本收口已完成（覆盖层模式治理与落标口径已闭环）。
87. [x] 覆盖层区域与中继桶分流最小版本已完成（`overlay_route_region/overlay_route_relay_bucket` 三侧同口径落标）。
88. [x] 覆盖层分流字段 ingress frame 原生化已完成（快照直接消费 frame 字段，不再临时推导）。
89. [x] 覆盖层中继候选集轮换最小版本已完成（`overlay_route_relay_set_size/overlay_route_relay_round/overlay_route_relay_index/overlay_route_relay_id` 三侧同口径落标）。
90. [x] L1/L2/L3 多节点部署矩阵文档已同步覆盖层新口径（`relay_set_size/relay_rotate_seconds` 与 `relay_*` 落标字段）。
91. [x] 灰度控制面手册已同步 Gap-C 新口径约束（覆盖层参数与 `relay_*` 落标字段对齐主线）。
92. [x] AOEM-FFI 执行层文档已同步主线边界口径（明确覆盖层 `overlay_route_*` 归属 host 层，不进入 AOEM ABI）。
93. [x] AOEM-FFI 中文权威文档已同步主线边界口径（明确覆盖层 `overlay_route_*` 归属宿主层，不进入 AOEM ABI）。
94. [x] AOEM-FFI 中文集成文档已补齐（`AOEM-FFI-BETA08-INTEGRATION-2026-03-01.md` 纳入中文权威索引）。
95. [x] AOEM-FFI 中文文档旧链接已清理（`docs/perf`、`docs-CN/perf`、失效 `docs/AOEM/*` 口径替换为当前仓内有效路径）。
96. [x] 覆盖层路由参数 runtime 模板化已完成（统一入口支持 `overlay.route.runtime.json` 按 profile 收口）。
97. [x] 灰度控制面已支持计划级覆盖层 profile 下发（`overlay_route_mode/overlay_route_runtime_file/overlay_route_runtime_profile` 直达 lifecycle/up 入口）。
98. [x] 覆盖层真实中继候选接入最小版本已完成（`overlay_route_relay_id` 优先使用 `NOVOVM_OVERLAY_ROUTE_RELAY_CANDIDATES`，无候选时保持原有回退格式）。
99. [x] 覆盖层 runtime 模板已支持中继候选集下发（`overlay.route.runtime.json` 可配置 `relay_candidates`，统一入口下发 `NOVOVM_OVERLAY_ROUTE_RELAY_CANDIDATES`）。
100. [x] 覆盖层 runtime 模板已支持区域/角色候选集分层下发（`relay_candidates_by_region` 与 `relay_candidates_by_role` 收口到统一入口）。
101. [x] 灰度控制面已支持计划级中继候选集下发（`plans[].overlay_route_relay_candidates` 直达 rollout/lifecycle/up 入口）。
102. [x] 灰度控制面已支持计划级中继候选分层下发（`plans[].overlay_route_relay_candidates_by_region/by_role` 直达统一入口并参与优先级决策）。
103. [x] 覆盖层已支持真实中继目录与健康阈值筛选（`overlay_route_relay_directory_file + overlay_route_relay_health_min`，计划级可下发）。
104. [x] 覆盖层已支持惩罚-恢复调优闭环（`overlay_route_relay_penalty_*` 持久化惩罚 + 每次运行恢复）。
105. [x] 覆盖层已支持失败重试自动惩罚注入（`overlay_route_auto_penalty_*` 失败分支自动生成并合并 `overlay_route_relay_penalty_delta`）。
106. [x] 覆盖层已支持自动惩罚联动调优（runtime profile 默认 `auto_penalty_*` + 连续失败/目录健康联动步长）。
107. [x] 覆盖层已支持控制面派发前健康探活刷新（`overlay_route_relay_health_refresh_*` 按冷却周期刷新目录 health，失败不阻断派发）。
108. [x] 覆盖层已支持控制面派发前动态发现合并（`overlay_route_relay_discovery_*` 按冷却周期将发现源并入目录，失败不阻断派发）。
109. [x] 覆盖层已支持多源发现与源权重合并（本地文件 + HTTP 源，`source_weights` 参与冲突裁决与加权健康）。
110. [x] 覆盖层已支持多源发现来源信誉衰减与黑名单治理（`source_reputation_file/source_decay/source_penalty_on_fail/source_recover_on_success/source_blacklist_threshold/source_denylist`，低信誉来源自动跳过）。
111. [x] 覆盖层已支持 seed 源分层接入与热更新（`http_urls_file + seed_region/seed_mode/seed_profile`，按计划上下文分层选源并在每次 discovery 实时读取）。
112. [x] 覆盖层已支持 seed 源故障切换自治策略（`seed_priority/success_rate_threshold/cooldown_seconds/max_consecutive_failures`，支持降级冷却与恢复并透出审计落标）。
113. [x] 覆盖层已支持公网中继信誉分级与区域 failover 策略化（`region_priority/region_failover_threshold/region_cooldown_seconds` + `relay_score`，审计透出 `relay_selected/relay_score/region_failover_reason/region_recover_at_unix_ms`）。
114. [x] 覆盖层 P2 区域生产模板已完成（`prod-cn/prod-eu/prod-us` 默认参数：`region_priority/relay_score_smoothing_alpha/region_failover_threshold/region_cooldown_seconds/max_consecutive_failures/success_rate_threshold`）。
115. [x] 覆盖层 Auto Profile v0 选择器 Rust 化已完成（`crates/novovm-rollout-policy/src/policy/overlay/auto_profile.rs`，rule-based + 防抖 + 状态持久化）。
116. [x] 覆盖层 Auto Profile 已打通生命周期编排链路（`rollout-control -> rollout -> lifecycle -> novovm-up` 参数透传，默认关闭）。
117. [x] 固定策略程序 Rust 迁移封盘已启动（脚本保留编排薄壳，策略核心优先迁移 Rust）。
118. [x] 覆盖层 discovery merge 策略核心 Rust 化已完成（新增 `novovm-overlay-relay-discovery-merge`，`rollout-control` 优先调用 Rust 二进制，ps1 保留兜底）。
119. [x] 覆盖层 health refresh 策略核心 Rust 化已完成（新增 `novovm-overlay-relay-health-refresh`，`rollout-control` 优先调用 Rust 二进制，ps1 保留兜底）。
120. [x] 灰度决策审计导出策略核心 Rust 化已完成（新增 `novovm-rollout-decision-dashboard-export`，`rollout-control` 优先调用 Rust 二进制，ps1 保留兜底）。
121. [x] 灰度决策看板消费策略核心 Rust 化已完成（新增 `novovm-rollout-decision-dashboard-consumer`，`rollout-control` 优先调用 Rust 二进制，ps1 保留兜底）。
122. [x] 灰度决策真实投递策略核心 Rust 化已完成（新增 `novovm-rollout-decision-delivery`，`rollout-control` 优先调用 Rust 二进制，失败自动回退内置 PowerShell 逻辑）。
123. [x] 灰度决策路由策略核心 Rust 化已完成（新增 `novovm-rollout-decision-route`，`rollout-control` 优先调用 Rust 二进制，失败自动回退脚本路由逻辑）。
124. [x] 风险动作矩阵评估策略核心 Rust 化已完成（新增 `novovm-risk-action-eval`，`rollout-control` 优先调用 Rust 二进制，失败自动回退脚本评估逻辑）。
125. [x] 风险动作矩阵选择策略核心 Rust 化已完成（新增 `novovm-risk-matrix-select`，`rollout-control` 优先调用 Rust 二进制，失败自动回退脚本选择逻辑）。
126. [x] 风险阻断集合选择策略核心 Rust 化已完成（新增 `novovm-risk-blocked-select`，`rollout-control` 优先调用 Rust 二进制，失败自动回退脚本选择逻辑）。
127. [x] 风险阻断集合覆盖映射构建策略核心 Rust 化已完成（新增 `novovm-risk-blocked-map-build`，`rollout-control` 优先调用 Rust 二进制，失败自动回退脚本构建逻辑）。
128. [x] 风险等级集合规范化策略核心 Rust 化已完成（新增 `novovm-risk-level-set`，`rollout-control` 优先调用 Rust 二进制，失败自动回退脚本解析逻辑）。
129. [x] 风险策略模板选择策略核心 Rust 化已完成（新增 `novovm-risk-policy-profile-select`，`rollout-control` 优先调用 Rust 二进制，失败自动回退脚本选择逻辑）。
130. [x] 风险动作矩阵规范化构建策略核心 Rust 化已完成（新增 `novovm-risk-action-matrix-build`，`rollout-control` 优先调用 Rust 二进制，失败自动回退脚本构建逻辑）。
131. [x] 切主策略矩阵规范化构建策略核心 Rust 化已完成（新增 `novovm-failover-policy-matrix-build`，`rollout-control` 优先调用 Rust 二进制，失败自动回退脚本构建逻辑）。
132. [x] 策略二进制目录收口已完成（`novovm-overlay-*`、`novovm-rollout-*`、`novovm-risk-*`、`novovm-failover-policy-*` 从 `crates/novovm-node/src/bin` 批量迁移到 `crates/novovm-rollout-policy/src/bin`，`novovm-node` 保留节点主程序）。
133. [x] 灰度控制面统一策略入口已打通（新增 `risk_policy.policy_cli.binary_file` 指向 `novovm-rollout-policy`，支持热更新与单工具二进制覆盖）。
134. [x] `novovm-rollout-policy` clap 单入口子命令树骨架已落地（`overlay/rollout/risk/failover` 四域子命令 + 旧平铺 tool 名兼容转发）。
135. [x] `overlay relay-discovery-merge` 已内收到统一策略实现（新单入口直接执行共享模块，旧 bin 保留薄兼容包装）。
136. [x] `overlay auto-profile` 已内收到统一策略实现（新单入口直接执行共享模块，旧 bin 保留薄兼容包装）。
137. [x] `overlay relay-health-refresh` 已内收到统一策略实现（新单入口直接执行共享模块，旧 bin 保留薄兼容包装）。
138. [x] 风险控制面残余矩阵重建逻辑已削为 emergency fallback（`Resolve-RiskActionMatrix` 不再在 PowerShell 内重建完整 `source/min_site_priority` 覆盖矩阵，仅保留保守全局基线）。
139. [x] 风险策略 profile 选择已优先收口到统一 CLI（`Resolve-RiskPolicyProfileSelection` 优先走 `novovm-rollout-policy risk policy-profile-select`，脚本只保留 `active_profile/policy_profiles` 查表兜底）。
140. [x] 风险控制面 `Apply-ReplicaSloPolicy` 本地 SLO / circuit fallback 已削为 emergency fallback（Rust 不可用时仅按 `grade` 生成保守 score 与限流/阻断动作，不再在 PowerShell 内执行完整窗口评分与 score 命中矩阵）。
141. [x] 风险控制面 `matrix/blockset` 选择 fallback 已削为保守默认（`Select-RiskActionMatrix / Resolve-RiskBlockedSetMap / Select-RiskBlockedSet` 不再在 PowerShell 内执行 site/region 分层选择或映射构建，仅回全局 baseline/default set）。
142. [x] 灰度控制面 `rollout decision-route` fallback 已削为保守默认（`Resolve-RolloutDecisionAlertLevel / AlertChannel / AlertTarget / DeliveryType / DeliveryEndpoint` 不再在 PowerShell 内执行角色/映射推导，仅保留 `ops-observe/ops-oncall` 两档与 endpoint 地址簿查表）。
143. [x] `rollout` 域四个固定子命令已内收到统一策略实现（`decision-route / decision-delivery / decision-dashboard-export / decision-dashboard-consumer` 已迁入 `crates/novovm-rollout-policy/src/policy/rollout/*`，旧独立二进制仅保留兼容薄壳）。
144. [x] 灰度控制面 `Send-RolloutDecisionDelivery` fallback 已削为 emergency fallback（脚本不再执行本地 SMTP 邮件发送或复杂投递分支，仅保留显式 http(s) endpoint 的 webhook/im 保守投递）。
145. [x] `failover-policy-matrix-build` 已内收到统一策略实现（迁入 `crates/novovm-rollout-policy/src/policy/failover/policy_matrix_build.rs`，旧独立二进制仅保留兼容薄壳）。
146. [x] 统一入口 legacy 平铺 tool 名已完成最终清场（`novovm-rollout-policy <flat-tool>` 不再经 sibling bin 中转，直接分发到统一共享模块；无价值的 `commands/shared.rs` 已退役）。

## 3. 已完成项（主线）

1. 统一入口与生产运行手册：`docs_CN/NOVOVM-UNIFIED-ENTRYPOINT-AND-RUNBOOK-2026-03-22.md`。  
2. 四层路线图与角色化入口：`docs_CN/NOVOVM-L1-L4-ROADMAP-v1-2026-03-22.md`。  
3. 角色运行手册：`docs_CN/NOVOVM-L1-L4-ROLE-PROFILES-RUNBOOK-2026-03-23.md`。  
4. UA 生产操作命令手册：`docs_CN/NOVOVM-UA-PROD-OPS-CMDS-2026-03-23.md`。  
5. 脚本入口支持角色参数：`scripts/novovm-up.ps1`。  
6. 节点侧四层锚点写入：`crates/novovm-node/src/bin/novovm-node.rs`（`NOVOVM_L1L4_ANCHOR_PATH`）。  
7. 节点侧四层锚点已接入统一账本键空间：`NOVOVM_L1L4_ANCHOR_LEDGER_*`。  
8. 多机部署模板脚本与文档：`scripts/novovm-generate-role-matrix.ps1`、`docs_CN/NOVOVM-L1-L3-MULTI-NODE-PROD-MATRIX-2026-03-23.md`。  
9. 收益结算周期脚本与手册：`scripts/novovm-l1l4-settlement-cycle.ps1`、`docs_CN/NOVOVM-L1L4-SETTLEMENT-CYCLE-RUNBOOK-2026-03-23.md`。  
10. 自动收益发放脚本与手册：`scripts/novovm-l1l4-auto-payout.ps1`、`docs_CN/NOVOVM-L1L4-AUTO-PAYOUT-RUNBOOK-2026-03-23.md`。  
11. 到账执行脚本与手册：`scripts/novovm-l1l4-payout-execute.ps1`、`docs_CN/NOVOVM-L1L4-PAYOUT-EXECUTE-RUNBOOK-2026-03-23.md`。  
12. 外部链确认脚本与手册：`scripts/novovm-l1l4-external-confirm.ps1`、`docs_CN/NOVOVM-L1L4-EXTERNAL-CONFIRM-RUNBOOK-2026-03-23.md`。  
13. 真实签名广播脚本与手册：`scripts/novovm-l1l4-real-broadcast.ps1`、`docs_CN/NOVOVM-L1L4-REAL-BROADCAST-RUNBOOK-2026-03-23.md`。  
14. 强一致回补脚本与手册：`scripts/novovm-l1l4-reconcile.ps1`、`docs_CN/NOVOVM-L1L4-RECONCILE-RUNBOOK-2026-03-23.md`。  
15. 回补 daemon 脚本与手册：`scripts/novovm-l1l4-reconcile-daemon.ps1`、`docs_CN/NOVOVM-L1L4-RECONCILE-DAEMON-RUNBOOK-2026-03-23.md`。  
16. 主入口一体化回补参数：`scripts/novovm-up.ps1`、`scripts/migration/run_gateway_node_pipeline.ps1`。  
17. gateway 二进制生命周期内嵌回补：`crates/gateways/evm-gateway/src/main.rs`（`NOVOVM_GATEWAY_EMBED_RECONCILE_DAEMON`）。  
18. 公网节点生命周期编排脚本与手册：`scripts/novovm-node-lifecycle.ps1`、`docs_CN/NOVOVM-NODE-LIFECYCLE-UPGRADE-ROLLBACK-RUNBOOK-2026-04-03.md`。  
19. 生命周期治理增强（runtime/set-policy/upgrade group guard）：`scripts/novovm-node-lifecycle.ps1`。  
20. 外层灰度编排控制器与手册：`scripts/novovm-node-rollout.ps1`、`docs_CN/NOVOVM-NODE-GRAY-ROLLOUT-CONTROLLER-RUNBOOK-2026-04-03.md`。  
21. 跨主机执行适配（SSH/WinRM）：`scripts/novovm-node-rollout.ps1`、`config/runtime/lifecycle/rollout.plan.json`。  
22. 执行认证与审计追踪：`scripts/novovm-node-rollout.ps1`、`config/runtime/lifecycle/rollout.plan.json`。  
23. 灰度集中调度控制面：`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
24. 控制面策略编排增强：`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`。  
25. 控制面策略学习化调度：`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`。  
26. 多控制器一致性治理：`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`。  
27. 跨站点控制器共识：`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`。  
28. 跨站点状态复制与恢复：`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
29. 跨机房双写容灾与副本一致性校验：`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
30. 副本健康分级与自动切主：`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
31. 自动回切与异地恢复演练模板：`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`、`docs_CN/NOVOVM-STATE-REPLICA-DRILL-TEMPLATE-2026-04-04.md`。  
32. 副本健康 SLO 门槛与自动阻断：`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
33. SLO 分级熔断：`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
34. SLO 多级熔断矩阵：`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
35. SLO 自适应阈值：`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
36. 跨站点自动切主策略矩阵：`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
37. 容灾演练自动评分：`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
38. 切主策略与 SLO/演练评分联动：`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
39. 跨站点冲突自动判责与降权矩阵：`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
40. 跨站点信誉分长期治理：`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
41. 跨站点信誉分多周期风险预测与自动限流联动：`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
42. 高风险站点赢家保护与切主风险联动：`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
43. 统一风险阻断矩阵基线：`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
44. 风险策略角色矩阵默认值：`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
45. 风险动作矩阵：`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
46. 风险动作矩阵来源分层：`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
47. 风险动作矩阵三级覆盖：`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
48. 风险阻断等级三级覆盖：`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
49. 切主策略矩阵三级覆盖：`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
50. 切主联动门槛三级覆盖：`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
51. 站点优先级三级覆盖：`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
52. 风险动作矩阵与站点优先级联动：`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
53. 风险动作策略变更审计聚合：`scripts/novovm-node-rollout-control.ps1`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
54. 灰度决策摘要审计聚合：`scripts/novovm-node-rollout-control.ps1`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
55. 灰度决策摘要角色告警分级：`scripts/novovm-node-rollout-control.ps1`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
56. 灰度决策摘要告警通道映射：`scripts/novovm-node-rollout-control.ps1`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
57. 灰度决策摘要告警目标映射：`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
58. 灰度决策摘要投递类型映射：`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
59. 灰度决策摘要真实投递执行：`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
60. 风险策略模板化与热切换参数收口：`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
61. 覆盖层寻址增强版最小落地：`crates/novovm-node/src/bin/novovm-node.rs`、`docs_CN/NOVOVM-L1-L4-ROLE-PROFILES-RUNBOOK-2026-03-23.md`、`docs_CN/NOVOVM-L1-L4-ROADMAP-v1-2026-03-22.md`。  
62. 覆盖层寻址增强版 gateway 同口径落地：`crates/gateways/evm-gateway/src/main.rs`、`docs_CN/NOVOVM-L1-L4-ROLE-PROFILES-RUNBOOK-2026-03-23.md`、`docs_CN/NOVOVM-L1-L4-ROADMAP-v1-2026-03-22.md`。  
63. 覆盖层寻址第二阶段参数化最小落地：`crates/novovm-node/src/bin/novovm-node.rs`、`crates/gateways/evm-gateway/src/main.rs`、`docs_CN/NOVOVM-L1-L4-ROLE-PROFILES-RUNBOOK-2026-03-23.md`。  
64. 覆盖层寻址第二阶段 ingress frame 同口径最小落地：`crates/novovm-adapter-api/src/evm_mirror.rs`、`crates/plugins/evm/plugin/src/lib.rs`、`crates/gateways/evm-gateway/src/main.rs`。  
65. 覆盖层寻址第二阶段审计链路同口径最小落地：`crates/plugins/evm/plugin/src/lib.rs`。  
66. 跨站点信誉分前瞻预警与自动治理最小落地：`scripts/novovm-node-rollout-control.ps1`。  
67. 容灾闭环策略自动收敛最小落地：`scripts/novovm-node-rollout-control.ps1`。  
68. 容灾收敛阈值区域化细化最小落地：`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
69. 容灾收敛参数运行中热重载最小落地：`scripts/novovm-node-rollout-control.ps1`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
70. 容灾收敛策略模板联动最小落地：`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
71. 灰度决策摘要收敛参数透出最小落地：`scripts/novovm-node-rollout-control.ps1`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
72. 灰度决策投递链路收敛参数透出最小落地：`scripts/novovm-node-rollout-control.ps1`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
73. 灰度决策审计下游导出最小落地：`scripts/novovm-rollout-decision-dashboard-export.ps1`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
74. 灰度决策审计导出控制面内置周期化最小落地：`scripts/novovm-node-rollout-control.ps1`、`scripts/novovm-rollout-decision-dashboard-export.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
75. 灰度决策看板消费端内置周期化最小落地：`scripts/novovm-node-rollout-control.ps1`、`scripts/novovm-rollout-decision-dashboard-consumer.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
76. 回补参数治理模板化最小落地：`scripts/novovm-up.ps1`、`config/runtime/lifecycle/reconcile.runtime.json`、`docs_CN/NOVOVM-UNIFIED-ENTRYPOINT-AND-RUNBOOK-2026-03-22.md`。  
77. 覆盖层多跳强约束最小落地：`scripts/novovm-up.ps1`、`crates/novovm-node/src/bin/novovm-node.rs`、`crates/gateways/evm-gateway/src/main.rs`、`crates/plugins/evm/plugin/src/lib.rs`、`docs_CN/NOVOVM-L1-L4-ROADMAP-v1-2026-03-22.md`、`docs_CN/NOVOVM-L1-L4-ROLE-PROFILES-RUNBOOK-2026-03-23.md`。  
78. 覆盖层多跳细粒度轮换最小落地：`scripts/novovm-up.ps1`、`crates/novovm-node/src/bin/novovm-node.rs`、`crates/gateways/evm-gateway/src/main.rs`、`crates/plugins/evm/plugin/src/lib.rs`、`docs_CN/NOVOVM-L1-L4-ROADMAP-v1-2026-03-22.md`。  
79. 生产入口公网暴露保护最小落地：`scripts/novovm-up.ps1`、`docs_CN/NOVOVM-UNIFIED-ENTRYPOINT-AND-RUNBOOK-2026-03-22.md`。  
80. 覆盖层路由模式开关最小落地：`scripts/novovm-up.ps1`、`crates/novovm-node/src/bin/novovm-node.rs`、`crates/gateways/evm-gateway/src/main.rs`、`crates/plugins/evm/plugin/src/lib.rs`、`docs_CN/NOVOVM-UNIFIED-ENTRYPOINT-AND-RUNBOOK-2026-03-22.md`、`docs_CN/NOVOVM-L1-L4-ROLE-PROFILES-RUNBOOK-2026-03-23.md`。  
81. 覆盖层路由模式落标最小落地：`crates/novovm-node/src/bin/novovm-node.rs`、`crates/gateways/evm-gateway/src/main.rs`、`crates/plugins/evm/plugin/src/lib.rs`、`docs_CN/NOVOVM-L1-L4-ROLE-PROFILES-RUNBOOK-2026-03-23.md`。  
82. Gap-A/B/C 收口清单：`docs_CN/NOVOVM-GAP-ABC-CLOSURE-CHECKLIST-v1-2026-04-05.md`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`、`config/runtime/lifecycle/rollout.queue.json`、`config/runtime/lifecycle/reconcile.runtime.json`。  
83. 覆盖层区域与中继桶分流最小落地：`crates/novovm-node/src/bin/novovm-node.rs`、`crates/gateways/evm-gateway/src/main.rs`、`crates/plugins/evm/plugin/src/lib.rs`。  
84. 覆盖层分流字段 ingress frame 原生化最小落地：`crates/novovm-adapter-api/src/evm_mirror.rs`、`crates/plugins/evm/plugin/src/lib.rs`、`crates/gateways/evm-gateway/src/main.rs`。  
85. 覆盖层中继候选集轮换最小落地：`scripts/novovm-up.ps1`、`crates/novovm-node/src/bin/novovm-node.rs`、`crates/gateways/evm-gateway/src/main.rs`、`crates/plugins/evm/plugin/src/lib.rs`、`crates/novovm-adapter-api/src/evm_mirror.rs`。  
86. 多节点部署矩阵文档口径同步：`docs_CN/NOVOVM-L1-L3-MULTI-NODE-PROD-MATRIX-2026-03-23.md`。  
87. 灰度控制面手册 Gap-C 口径同步：`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
88. AOEM-FFI 执行层边界口径同步：`docs/AOEM-FFI/AOEM-FFI-BETA08-INTEGRATION-2026-03-01.md`、`docs/AOEM-FFI/AOEM-FFI-FULLMAX-CAPABILITY-MATRIX-2026-03-12.md`、`docs/AOEM-FFI/AOEM-FFI-HOST-CALL-PARAMS-V1-2026-03-10.md`。  
89. AOEM-FFI 中文权威文档边界口径同步：`docs_CN/AOEM-FFI/AOEM-FFI-FULLMAX-CAPABILITY-MATRIX-2026-03-12.md`、`docs_CN/AOEM-FFI/AOEM-FFI-HOST-CALL-PARAMS-V1-2026-03-10.md`。  
90. AOEM-FFI 中文集成文档补齐：`docs_CN/AOEM-FFI/AOEM-FFI-BETA08-INTEGRATION-2026-03-01.md`、`docs_CN/AOEM-FFI/README.md`。  
91. AOEM-FFI 中文文档旧链接清理：`docs_CN/AOEM-FFI/AOEM-FFI-FULLMAX-CAPABILITY-MATRIX-2026-03-12.md`、`docs_CN/AOEM-FFI/AOEM-FFI-HOST-CALL-PARAMS-V1-2026-03-10.md`、`docs_CN/AOEM-FFI/README.md`。  
92. 覆盖层路由参数 runtime 模板化：`scripts/novovm-up.ps1`、`config/runtime/lifecycle/overlay.route.runtime.json`、`docs_CN/NOVOVM-UNIFIED-ENTRYPOINT-AND-RUNBOOK-2026-03-22.md`。  
93. 灰度控制面计划级覆盖层 profile 下发：`scripts/novovm-node-rollout-control.ps1`、`scripts/novovm-node-rollout.ps1`、`scripts/novovm-node-lifecycle.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。  
94. 覆盖层真实中继候选接入最小落地：`crates/novovm-node/src/bin/novovm-node.rs`、`crates/plugins/evm/plugin/src/lib.rs`、`crates/gateways/evm-gateway/src/main.rs`（`NOVOVM_OVERLAY_ROUTE_RELAY_CANDIDATES`）。
95. 覆盖层 runtime 模板区域/角色候选集分层下发：`scripts/novovm-up.ps1`、`config/runtime/lifecycle/overlay.route.runtime.json`、`docs_CN/NOVOVM-UNIFIED-ENTRYPOINT-AND-RUNBOOK-2026-03-22.md`（`relay_candidates_by_region/relay_candidates_by_role`）。
96. 覆盖层 runtime 模板候选集下发：`scripts/novovm-up.ps1`、`config/runtime/lifecycle/overlay.route.runtime.json`、`docs_CN/NOVOVM-UNIFIED-ENTRYPOINT-AND-RUNBOOK-2026-03-22.md`（`relay_candidates`）。
97. 灰度控制面计划级中继候选集下发：`scripts/novovm-node-rollout-control.ps1`、`scripts/novovm-node-rollout.ps1`、`scripts/novovm-node-lifecycle.ps1`、`scripts/novovm-up.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。
98. 灰度控制面计划级中继候选分层下发：`scripts/novovm-node-rollout-control.ps1`、`scripts/novovm-node-rollout.ps1`、`scripts/novovm-node-lifecycle.ps1`、`scripts/novovm-up.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`（`overlay_route_relay_candidates_by_region/by_role`）。
99. 覆盖层真实中继目录与健康阈值筛选：`scripts/novovm-up.ps1`、`scripts/novovm-node-lifecycle.ps1`、`scripts/novovm-node-rollout.ps1`、`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/overlay.relay.directory.json`、`config/runtime/lifecycle/overlay.route.runtime.json`、`config/runtime/lifecycle/rollout.queue.json`。
100. 覆盖层惩罚-恢复调优闭环：`scripts/novovm-up.ps1`、`scripts/novovm-node-lifecycle.ps1`、`scripts/novovm-node-rollout.ps1`、`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/overlay.route.runtime.json`、`config/runtime/lifecycle/rollout.queue.json`（`overlay_route_relay_penalty_*`）。
101. 覆盖层失败重试自动惩罚注入：`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`（`overlay_route_auto_penalty_*` + 重试审计落标）。
102. 覆盖层自动惩罚联动调优：`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/overlay.route.runtime.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`、`docs_CN/NOVOVM-UNIFIED-ENTRYPOINT-AND-RUNBOOK-2026-03-22.md`（runtime profile 默认 + `streak_boost/health_factor`）。
103. 覆盖层派发前健康探活刷新：`scripts/novovm-overlay-relay-health-refresh.ps1`、`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/overlay.route.runtime.json`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`（`overlay_route_relay_health_refresh_*` + 审计事件 `relay_health_refreshed/relay_health_refresh_error`）。
104. 覆盖层派发前动态发现合并：`scripts/novovm-overlay-relay-discovery-merge.ps1`、`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/overlay.relay.discovery.json`、`config/runtime/lifecycle/overlay.route.runtime.json`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`（`overlay_route_relay_discovery_*` + 审计事件 `relay_discovery_merged/relay_discovery_error`）。
105. 覆盖层多源发现与源权重合并：`scripts/novovm-overlay-relay-discovery-merge.ps1`、`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/overlay.route.runtime.json`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`、`docs_CN/NOVOVM-UNIFIED-ENTRYPOINT-AND-RUNBOOK-2026-03-22.md`（`overlay_route_relay_discovery_http_urls/source_weights/http_timeout_ms`）。
106. 覆盖层多源发现来源信誉衰减与黑名单治理：`scripts/novovm-overlay-relay-discovery-merge.ps1`、`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/overlay.route.runtime.json`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`、`docs_CN/NOVOVM-UNIFIED-ENTRYPOINT-AND-RUNBOOK-2026-03-22.md`（`overlay_route_relay_discovery_source_reputation_file/source_decay/source_penalty_on_fail/source_recover_on_success/source_blacklist_threshold/source_denylist` + 审计透出）。
107. 覆盖层 seed 源分层接入与热更新：`scripts/novovm-overlay-relay-discovery-merge.ps1`、`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/overlay.relay.discovery.seeds.json`、`config/runtime/lifecycle/overlay.route.runtime.json`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`、`docs_CN/NOVOVM-UNIFIED-ENTRYPOINT-AND-RUNBOOK-2026-03-22.md`（`overlay_route_relay_discovery_http_urls_file/seed_region/seed_mode/seed_profile` + 审计透出）。
108. 覆盖层 seed 源故障切换自治策略：`scripts/novovm-overlay-relay-discovery-merge.ps1`、`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/overlay.route.runtime.json`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`、`docs_CN/NOVOVM-UNIFIED-ENTRYPOINT-AND-RUNBOOK-2026-03-22.md`（`overlay_route_relay_discovery_seed_priority/seed_success_rate_threshold/seed_cooldown_seconds/seed_max_consecutive_failures` + 审计透出 `seed_selected/seed_failover_reason/seed_recover_at_unix_ms/seed_cooldown_skip`）。
109. 覆盖层公网中继信誉分级与区域 failover 策略化：`scripts/novovm-overlay-relay-discovery-merge.ps1`、`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/overlay.route.runtime.json`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`、`docs_CN/NOVOVM-UNIFIED-ENTRYPOINT-AND-RUNBOOK-2026-03-22.md`（`overlay_route_relay_discovery_region_priority/region_failover_threshold/region_cooldown_seconds` + 审计透出 `relay_selected/relay_score/region_failover_reason/region_recover_at_unix_ms`）。
110. 覆盖层 P2 区域生产模板定版：`config/runtime/lifecycle/overlay.route.runtime.json`、`config/runtime/lifecycle/rollout.queue.json`、`scripts/novovm-overlay-relay-discovery-merge.ps1`、`scripts/novovm-node-rollout-control.ps1`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`、`docs_CN/NOVOVM-UNIFIED-ENTRYPOINT-AND-RUNBOOK-2026-03-22.md`（`prod-cn/prod-eu/prod-us` + `relay_score_smoothing_alpha`）。
111. 覆盖层 Auto Profile v0 Rust selector 落地：`crates/novovm-rollout-policy/src/policy/overlay/auto_profile.rs`、`crates/novovm-rollout-policy/src/bin/overlay/novovm-overlay-auto-profile.rs`、`crates/novovm-rollout-policy/Cargo.toml`。
112. 覆盖层 Auto Profile 生命周期链路打通：`scripts/novovm-up.ps1`、`scripts/novovm-node-lifecycle.ps1`、`scripts/novovm-node-rollout.ps1`、`scripts/novovm-node-rollout-control.ps1`。
113. 统一入口手册已同步 Auto Profile 生产口径：`docs_CN/NOVOVM-UNIFIED-ENTRYPOINT-AND-RUNBOOK-2026-03-22.md`。
114. 固定策略程序 Rust 迁移封盘计划已形成：`docs_CN/NOVOVM-RUST-MIGRATION-SEAL-PLAN-2026-04-05.md`。
115. 覆盖层 discovery merge Rust 化落地：`crates/novovm-rollout-policy/src/policy/overlay/relay_discovery_merge.rs`、`crates/novovm-rollout-policy/src/bin/overlay/novovm-overlay-relay-discovery-merge.rs`、`crates/novovm-rollout-policy/Cargo.toml`、`scripts/novovm-node-rollout-control.ps1`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。
116. 覆盖层 health refresh Rust 化落地：`crates/novovm-rollout-policy/src/policy/overlay/relay_health_refresh.rs`、`crates/novovm-rollout-policy/src/bin/overlay/novovm-overlay-relay-health-refresh.rs`、`crates/novovm-rollout-policy/Cargo.toml`、`scripts/novovm-node-rollout-control.ps1`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。
117. 灰度决策审计导出 Rust 化落地：`crates/novovm-rollout-policy/src/bin/rollout/novovm-rollout-decision-dashboard-export.rs`、`crates/novovm-rollout-policy/Cargo.toml`、`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。
118. 灰度决策看板消费 Rust 化落地：`crates/novovm-rollout-policy/src/bin/rollout/novovm-rollout-decision-dashboard-consumer.rs`、`crates/novovm-rollout-policy/Cargo.toml`、`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。
119. 灰度决策真实投递 Rust 化落地：`crates/novovm-rollout-policy/src/bin/rollout/novovm-rollout-decision-delivery.rs`、`crates/novovm-rollout-policy/Cargo.toml`、`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。
120. 灰度决策路由 Rust 化落地：`crates/novovm-rollout-policy/src/bin/rollout/novovm-rollout-decision-route.rs`、`crates/novovm-rollout-policy/Cargo.toml`、`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。
121. 风险动作矩阵评估 Rust 化落地：`crates/novovm-rollout-policy/src/bin/risk/novovm-risk-action-eval.rs`、`crates/novovm-rollout-policy/Cargo.toml`、`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。
122. 风险动作矩阵选择 Rust 化落地：`crates/novovm-rollout-policy/src/bin/risk/novovm-risk-matrix-select.rs`、`crates/novovm-rollout-policy/Cargo.toml`、`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。
123. 风险阻断集合选择 Rust 化落地：`crates/novovm-rollout-policy/src/bin/risk/novovm-risk-blocked-select.rs`、`crates/novovm-rollout-policy/Cargo.toml`、`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。
124. 风险阻断集合覆盖映射构建 Rust 化落地：`crates/novovm-rollout-policy/src/bin/risk/novovm-risk-blocked-map-build.rs`、`crates/novovm-rollout-policy/Cargo.toml`、`scripts/novovm-node-rollout-control.ps1`、`config/runtime/lifecycle/rollout.queue.json`、`docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md`。
125. Failover seed-evaluate 共享实现落地：`crates/novovm-rollout-policy/src/policy/failover/seed_evaluate.rs`、`crates/novovm-rollout-policy/src/policy/failover/mod.rs`、`crates/novovm-rollout-policy/src/commands/failover.rs`、`crates/novovm-rollout-policy/src/cli/failover.rs`。
126. Failover region-evaluate 共享实现落地：`crates/novovm-rollout-policy/src/policy/failover/region_evaluate.rs`、`crates/novovm-rollout-policy/src/policy/failover/mod.rs`、`crates/novovm-rollout-policy/src/commands/failover.rs`、`crates/novovm-rollout-policy/src/cli/failover.rs`，并由 `crates/novovm-rollout-policy/src/policy/overlay/relay_discovery_merge.rs` 复用同一套 seed/region failover 规则。
127. Risk slo-evaluate 共享实现落地：`crates/novovm-rollout-policy/src/policy/risk/slo_evaluate.rs`、`crates/novovm-rollout-policy/src/policy/risk/mod.rs`、`crates/novovm-rollout-policy/src/commands/risk.rs`、`crates/novovm-rollout-policy/src/cli/risk.rs`。
128. Risk circuit-breaker-evaluate 共享实现落地：`crates/novovm-rollout-policy/src/policy/risk/circuit_breaker_evaluate.rs`、`crates/novovm-rollout-policy/src/policy/risk/mod.rs`、`crates/novovm-rollout-policy/src/commands/risk.rs`、`crates/novovm-rollout-policy/src/cli/risk.rs`。
129. 控制面副本 SLO / circuit-breaker 已优先接入统一 Rust risk 内核：`scripts/novovm-node-rollout-control.ps1`（`Apply-ReplicaSloPolicy` 优先调用 `novovm-rollout-policy risk slo-evaluate / circuit-breaker-evaluate`，失败才回退本地规则）。
130. Risk action-eval 共享实现落地：`crates/novovm-rollout-policy/src/policy/risk/action_eval.rs`、`crates/novovm-rollout-policy/src/commands/risk.rs`，旧兼容入口 `crates/novovm-rollout-policy/src/bin/risk/novovm-risk-action-eval.rs` 已收薄壳。
131. Risk level-set 共享实现落地：`crates/novovm-rollout-policy/src/policy/risk/level_set.rs`、`crates/novovm-rollout-policy/src/commands/risk.rs`，旧兼容入口 `crates/novovm-rollout-policy/src/bin/risk/novovm-risk-level-set.rs` 已收薄壳。
132. Risk action-matrix-build 共享实现落地：`crates/novovm-rollout-policy/src/policy/risk/action_matrix_build.rs`、`crates/novovm-rollout-policy/src/commands/risk.rs`，旧兼容入口 `crates/novovm-rollout-policy/src/bin/risk/novovm-risk-action-matrix-build.rs` 已收薄壳。
133. Risk matrix-select 共享实现落地：`crates/novovm-rollout-policy/src/policy/risk/matrix_select.rs`、`crates/novovm-rollout-policy/src/commands/risk.rs`，旧兼容入口 `crates/novovm-rollout-policy/src/bin/risk/novovm-risk-matrix-select.rs` 已收薄壳。
134. Risk blocked-select / blocked-map-build 共享实现落地：`crates/novovm-rollout-policy/src/policy/risk/blocked_select.rs`、`crates/novovm-rollout-policy/src/policy/risk/blocked_map_build.rs`、`crates/novovm-rollout-policy/src/commands/risk.rs`，旧兼容入口已收薄壳。
135. Risk policy-profile-select 共享实现落地：`crates/novovm-rollout-policy/src/policy/risk/policy_profile_select.rs`、`crates/novovm-rollout-policy/src/commands/risk.rs`，旧兼容入口 `crates/novovm-rollout-policy/src/bin/risk/novovm-risk-policy-profile-select.rs` 已收薄壳。

## 4. 收口结果（Gap-A/B/C）

## Gap-A：节点服务体系（升级/回滚/灰度/容灾控制面）
[x] 当前版本已收口。  
现状：控制面能力已全部进入主线并由 `config/runtime/lifecycle/rollout.queue.json` + `docs_CN/NOVOVM-NODE-ROLLOUT-CONTROL-PLANE-RUNBOOK-2026-04-04.md` 统一口径。  
收口判定：从本版本起，Gap-A 不再作为“未完成缺口”，后续仅作为参数优化与区域策略增强（不新增并行链路）。

## Gap-B：四层闭环与强一致回补
[x] 当前版本已收口。  
现状：四层闭环与回补主路径已统一在二进制主线，且由 `config/runtime/lifecycle/reconcile.runtime.json` 进行模板化参数收口。  
收口判定：从本版本起，Gap-B 不再作为“未完成缺口”，后续仅做模板参数调优与环境差异压平。

## Gap-C：覆盖层寻址增强
[x] 当前版本已收口。  
现状：覆盖层已形成 `secure|fast` 模式开关、强制多跳约束与 `overlay_route_mode` 落标闭环，并已补齐 `overlay_route_region/overlay_route_relay_bucket/overlay_route_relay_set_size/overlay_route_relay_round/overlay_route_relay_index/overlay_route_relay_id` 分流口径（锚点/gateway/plugin 同口径），同时已支持 `overlay.route.runtime.json` 按 profile 模板化治理、`NOVOVM_OVERLAY_ROUTE_RELAY_CANDIDATES` 真实中继候选接入，以及 `overlay_route_relay_penalty_*` + `overlay_route_auto_penalty_*`（含 runtime profile 默认、连续失败与目录健康联动）惩罚-恢复-失败注入闭环，并补齐派发前目录探活刷新、动态发现合并、多源权重治理、来源信誉黑名单治理、seed 源分层热更新、seed 故障切换自治策略，以及公网中继信誉分级与区域 failover 策略化能力；当前已提供 `prod-cn/prod-eu/prod-us` 三套默认生产参数模板。  
收口判定：从本版本起，Gap-C 不再作为“未完成缺口”，后续多跳中继强化归入下一阶段增强任务。

## 5. 后续增强（非 Gap，P2）

1. Gap-A 后续：控制面阈值与区域策略持续调优。  
2. Gap-B 后续：回补模板参数按区域与业务负载做精细化调参。  
3. Gap-C 后续：在已具备公网中继信誉分级与区域 failover 策略化能力基础上，继续做真实公网中继策略参数调优。  
4. 固定策略程序 Rust 化封盘：优先迁移覆盖层策略核心（selector/discovery/health/policy），脚本逐步收敛为跨平台编排薄壳。  

## 2026-04-06 PS1 thin-wrapper seal

147. `scripts/novovm-rollout-decision-dashboard-export.ps1` has been reduced to a compatibility thin wrapper and now directly invokes `novovm-rollout-policy rollout decision-dashboard-export`.
148. `scripts/novovm-rollout-decision-dashboard-consumer.ps1` has been reduced to a compatibility thin wrapper and now directly invokes `novovm-rollout-policy rollout decision-dashboard-consumer`.
149. `scripts/novovm-overlay-relay-health-refresh.ps1` has been reduced to a compatibility thin wrapper and now directly invokes `novovm-rollout-policy overlay relay-health-refresh`.
150. `scripts/novovm-overlay-relay-discovery-merge.ps1` has been reduced to a compatibility thin wrapper and now directly invokes `novovm-rollout-policy overlay relay-discovery-merge`.
151. Added `docs_CN/NOVOVM-PS1-INVENTORY-AND-MIGRATION-CUTLIST-2026-04-06.md` to classify the remaining `111` PowerShell scripts into keep/thin-wrapper/frozen groups; `scripts/migration/*` is explicitly frozen outside the current strategy-core migration path.

152. Switched PowerShell cleanup from single-file mode to batch mode: root-script thin-wrapper conversion, legacy-bin retirement audit, and migration/history isolation are now tracked as separate batches.
153. Added `docs_CN/NOVOVM-LEGACY-BIN-RETIREMENT-AUDIT-2026-04-06.md` and classified current legacy `src/bin/*` policy tools as compatibility thin-wrapper surfaces pending external-usage audit.
154. Added `scripts/migration/README.md` and formally isolated `scripts/migration/*` as a frozen history asset pool outside the current mainline Rust strategy-core cleanup path.

155. `scripts/novovm-node-rollout-control.ps1` now auto-discovers `target/release|debug/novovm-rollout-policy(.exe)` when `policy_cli.binary_file` is not explicitly set, so risk/rollout/failover tool defaults no longer require manual CLI wiring.
156. `scripts/novovm-node-rollout-control.ps1` overlay relay discovery and relay health refresh now prefer the unified `novovm-rollout-policy` binary as the default Rust path before falling back to legacy dedicated binaries or compatibility scripts.
157. `scripts/novovm-up.ps1` overlay auto-profile selection now prefers the unified `novovm-rollout-policy` binary and cargo fallback path instead of defaulting to the legacy dedicated `novovm-overlay-auto-profile` binary.
158. Added `Resolve-DefaultRolloutPolicyCliBinaryPath` to `scripts/novovm-node-rollout-control.ps1` so early runtime config and profile-selection paths can resolve the unified policy CLI before any legacy dedicated binary default is considered.
159. `Resolve-RiskPolicyProfileSelection` and `Apply-RiskLevelSetRuntimeConfig` now prefer the unified `novovm-rollout-policy` binary during early config resolution; legacy dedicated defaults remain only as compatibility fallback.
160. Centralized remaining legacy binary default resolution inside `scripts/novovm-node-rollout-control.ps1` through `Resolve-PolicyToolBinaryConfig`; repeated hard-coded `target/release/novovm-...` defaults for rollout/risk/failover/overlay helper tools were removed from the normal config path.
161. Added first-round legacy bin physical deletion candidates to `docs_CN/NOVOVM-LEGACY-BIN-RETIREMENT-AUDIT-2026-04-06.md`; these wrappers no longer hold default-path execution rights after the default-path cleanup batch.

162. Physically retired the first batch of `crates/novovm-rollout-policy/src/bin/*` compatibility wrapper bins and removed their `Cargo.toml` registrations; normal compatibility is now provided by the unified `novovm-rollout-policy` entrypoint plus flat legacy tool-name dispatch.
163. First-round legacy wrapper retirement completed for `overlay`, `risk`, `rollout`, and `failover` per-tool wrapper executables; these names no longer exist as separate dedicated build outputs under the current mainline policy-core layout.

164. Removed implicit legacy dedicated-binary auto-search from `Resolve-PolicyToolBinaryConfig`; empty default helper resolution now means unified `novovm-rollout-policy` first, otherwise missing-default, not silent fallback to deleted wrapper executables.
165. `scripts/novovm-up.ps1` no longer auto-searches the legacy dedicated `novovm-overlay-auto-profile` executable when no explicit binary path is provided; the default Rust path is unified CLI or cargo fallback only.

166. Added `docs_CN/NOVOVM-UNIFIED-POLICY-CORE-SEAL-AUDIT-2026-04-07.md` as the final mainline seal audit for the current policy-core migration phase.
167. The seal audit fixes the three-layer boundary explicitly: normal main path = unified Rust core, compatibility path = explicit shell/override only, emergency path = PowerShell startup+minimal conservative fallback only.
168. The current mainline policy-core operating model is now documented as a sealed model: deleted per-tool wrapper executables must not be auto-discovered as default runtime paths.

## 2026-04-07 主线收官结论

本轮 `NOVOVM` 统一 Rust 策略内核迁移，至此按主线口径正式收官。

当前已固定的主线事实：

- `overlay + failover + risk + rollout` 四组统一 Rust 内核已成立。
- 默认执行权已从运行期推进到配置期。
- 正常主路径 / 显式兼容路径 / emergency fallback 三层边界已封死。
- Batch 1 legacy wrapper bin 已物理退役。
- 已删除的 per-tool wrapper executable 不再允许被默认自动发现为主路径。

当前主线系统定义：

- 正常主路径：`novovm-rollout-policy`
- 兼容路径：显式脚本壳、显式 `binary_file` override、统一入口 flat legacy tool-name dispatch
- emergency fallback：PowerShell 启动壳、运维壳、审计与最小保守 fallback

因此，后续剩余事项统一降级为：

- 清理
- 历史资产整理
- 技术债回收
- 后续真实联调与生产验证

这些事项不再计入本轮主线能力建设范围。

## 2026-04-07 主线跨平台壳层迁移启动

169. 主线策略内核迁移收官后，新的下一阶段目标被固定为“主线运维壳 Rust 化”，不再继续把 `ps1` 壳层当作长期主线形态。
170. 工作区已新增 `crates/novovmctl` 骨架 crate，定位被明确锁定为“跨平台入口 / 运维壳”，不承载第二套策略判断，也不向 `novovm-node` 重新回灌壳层逻辑。
171. `novovmctl` 第一阶段只落 `up` 与 `rollout-control` 两条主线命令骨架；`rollout / lifecycle / daemon` 先占位，不扩大当前切换面。
172. 当前骨架的固定三层模型为：`novovmctl` = 壳层入口，`novovm-rollout-policy` = 统一策略脑子，`novovm-node` = 节点执行体；这一步用于推进真正的无-`ps1` 主线跨平台收口。
173. `scripts/novovm-up.ps1` 与 `scripts/novovm-node-rollout-control.ps1` 已切为前置 `novovmctl` 兼容壳：主线路径立即转发到 `novovmctl up` / `novovmctl rollout-control` 并直接退出，不再允许脚本内主逻辑继续作为正常执行路径。
174. 当前壳层切换采取“保留旧参数面、只支持已落地 `novovmctl` 子集”的严格模式；若显式使用尚未 Rust 化的 legacy 参数，兼容壳将直接报错，而不是悄悄回落到旧脚本脑子。
175. `crates/novovmctl` 已新增统一输出与 JSONL 审计 envelope：`up` 与 `rollout-control` 现在共用固定 `ok/command/timestamp_unix_ms/host/data|error` 结构，避免壳层 Rust 化后重新长出一套杂散日志口径。
176. `novovm-rollout-policy` 现已对齐第一批主线 JSON success-envelope：`overlay auto-profile-select / relay-discovery-merge / relay-health-refresh`、`risk slo-evaluate / circuit-breaker-evaluate / policy-profile-select`、`rollout decision-route` 统一采用 `ok/domain/action/timestamp_unix_ms/data` 外层结构，`novovmctl` 解析层也已兼容 envelope 内 `data` 载荷。

## 177. 2026-04-07 `novovmctl rollout-control` 输入契约对齐（queue-driven）

- `novovm-rollout-policy` 新增 `rollout controller-dispatch-evaluate`，接收 `--queue-file / --plan-action / --controller-id / --operation-id`，作为 `novovmctl rollout-control` 的统一 queue 适配入口。
- `novovmctl rollout-control` 不再把 `queue-file` 直接误传给 `risk slo-evaluate / circuit-breaker-evaluate / policy-profile-select`；改为先读取 `config/runtime/lifecycle/rollout.queue.json`，再按真实契约传入：
  - `slo-evaluate`: `state_file / grade / window_samples / min_green_rate / max_red_in_window / block_on_violation / now_unix_ms`
  - `circuit-breaker-evaluate`: `score / base_concurrent / base_pause / yellow_concurrent / yellow_pause / red_block / matrix_json`
  - `policy-profile-select`: `risk_policy_json / requested_profile`
- `replica_health_file` 现在作为 `grade` 的输入源；若缺失或无法解析，则默认保守回退到 `yellow`，避免 `novovmctl` 继续依赖旧 PowerShell 风险判定链。
- 这一步完成后，`novovmctl rollout-control` 与 `novovm-rollout-policy` 的主线输入契约不再悬空，下一步再补 `novovmctl up` 参数覆盖面。


## 178. 2026-04-07 `novovmctl up` auto-profile input-alignment + parameter-surface widen

- `novovmctl up` 已从伪契约切到 `overlay auto-profile-select` 的真实输入契约：
  - 不再误传 `--runtime-profile`
  - 改为传入 `--current-profile`
  - 不再误传 `--audit-file`
  - 新增支持 `--state-file / --profiles / --min-hold-seconds / --switch-margin / --switchback-cooldown-seconds / --recheck-seconds`
- `novovm-up.ps1` 兼容壳已同步扩大支持面：
  - `AutoProfileStateFile`
  - `AutoProfileProfiles`
  - `AutoProfileMinHoldSeconds`
  - `AutoProfileSwitchMargin`
  - `AutoProfileSwitchbackCooldownSeconds`
  - `AutoProfileRecheckSeconds`
  - `AutoProfileBinaryPath -> --policy-cli-binary-file`
- 顺手修复 `novovm-up.ps1` 兼容壳缺失 `DryRun` 参数定义的问题，避免严格模式下引用未定义变量。
- 这一步完成后，`up` 主线在 auto-profile 这一段不再依赖旧脚本参数惯性，下一步可继续推进 `rollout / lifecycle / daemon` 壳层迁 Rust。

## 179. 2026-04-07 `novovmctl daemon` first usable subset

- 新增 `novovmctl daemon`，作为 `novovm-prod-daemon.ps1` 的 Rust 主线入口子集：
  - 复用 `up` 主链（二进制发现 + auto-profile warmup + node launch）
  - 叠加 restart loop（`restart_delay_seconds / max_restarts`）
  - 支持 `NOVOVM_OPS_WIRE_WATCH*` 相关环境位：
    - `use_node_watch_mode`
    - `poll_ms`
    - `node_watch_batch_max_files`
    - `idle_exit_seconds`
- `novovm-prod-daemon.ps1` 已切成严格兼容壳，主路径改为转发 `novovmctl daemon`；未 Rust 化的 legacy 参数（如 `NoGateway / BuildBeforeRun / LeanIo / GatewayBind / SpoolDir / GatewayMaxRequests`）不再假装支持，而是显式报错。
- 这一步的范围是 `node-only daemon shell`，没有把 gateway / reconcile / build orchestration 伪装成已经迁完；后续若继续，需要单独迁这些旧 PowerShell 壳行为。

## 180. 2026-04-07 `novovmctl lifecycle` state-shell subset

- 新增 `novovmctl lifecycle` 可用子集，当前支持：
  - `status`
  - `set-runtime`
  - `set-policy`
- 这条线只处理 lifecycle state/governance 文件，不重建旧 PowerShell 的 start/stop/register/upgrade/rollback 主逻辑。
- `novovm-node-lifecycle.ps1` 已切成严格兼容壳，主路径改为转发 `novovmctl lifecycle`；未 Rust 化动作会显式拒绝，不再偷偷回旧逻辑。
- `set-runtime` 当前支持的 Rust 承接字段包括：
  - `profile / role_profile`
  - `overlay_route_*` 主线字段
  - `use_node_watch_mode / poll_ms / node_watch_batch_max_files / idle_exit_seconds`
  - `auto_profile_*` 主线字段
- `set-policy` 当前支持的 Rust 承接字段包括：
  - `governance.node_group`
  - `governance.upgrade_window`
- 顺手修复兼容壳严格模式问题：`AuditFile` 现已显式加入参数表。

## 181. 2026-04-07 `novovmctl rollout` plan-state subset

- 新增 `novovmctl rollout` 的首个可用子集：当前只支持 `status`。
- `status` 会读取 `rollout.plan.json` 并输出：
  - `allowed_controllers`
  - `group_order`
  - `enabled_groups`
  - `enabled/disabled node count`
  - `local/ssh/winrm` 传输分布
- `novovm-node-rollout.ps1` 已切成严格兼容壳，主路径改为转发 `novovmctl rollout`；当前只放行 `status`，`upgrade/rollback/set-policy` 明确拒绝，不再偷偷回落到旧 PowerShell 编排逻辑。
- 这一步是 `plan-state shell`，不是远程 rollout orchestration 迁移；后续若继续迁，需要单独 Rust 化远程 lifecycle/upgrade/rollback 编排链。

## 182. 2026-04-07 `novovmctl` 主线壳层跨平台第一阶段收口

- 本阶段到此收口，不再把 `gateway / reconcile / build orchestration / 远程 upgrade/rollback orchestration` 混入同一主线。
- 当前可成立的阶段结论：
  - `novovmctl up` 已进入 Rust 主链
  - `novovmctl rollout-control` 已进入 Rust 主链
  - `novovmctl lifecycle` 已具备 `status / set-runtime / set-policy` 子集
  - `novovmctl daemon` 已具备 `node-only + watch` 子集
  - `novovmctl rollout` 已具备 `status` plan-state 子集
- 这意味着主线入口壳已不再只能依赖 PowerShell 才能运作；PowerShell 在主线上的角色已下降为严格兼容壳。
- 后续事项统一降级为第二阶段候选，不计入本阶段收口范围：
  - `gateway` 壳层 Rust 化
  - `reconcile` 壳层 Rust 化
  - `build orchestration` 壳层 Rust 化
  - 远程 `lifecycle / upgrade / rollback / set-policy` orchestration Rust 化
- 本阶段定性：`novovmctl` 第一阶段已完成“主线壳层跨平台收口”，后续工作转入第二阶段能力扩展或单独验证线。

## 183. 2026-04-07 第二阶段立项：壳层全量 Rust 化

- 第一阶段正式口径固定为：
  - `up`：主链级 Rust 壳承接
  - `rollout-control`：主链级 Rust 壳承接
  - `lifecycle / daemon / rollout`：已完成首个可用子集迁移，但尚未完成全量 Rust 壳承接
- 第二阶段不再按零碎动作推进，改为按命令域整包迁移：
  - `Phase 2-A`：`lifecycle` 全量 Rust 壳承接
  - `Phase 2-B`：`rollout` 全量 Rust 壳承接
  - `Phase 2-C`：`daemon` 全量 Rust 壳承接
- 第二阶段主线范围只处理生产壳层，不纳入：
  - `scripts/migration/*`
  - AOEM 构建/打包脚本
  - 历史迁移辅助脚本
- 第二阶段推进原则固定为：
  - 一次只打一个命令域
  - 每个命令域整包迁，不再按单动作零碎补齐
  - 命令域迁完后立即把对应 `ps1` 压成纯兼容壳
  - 命令域迁完即封口，不把未迁行为继续混留在主线
- 第二阶段建议顺序固定为：`lifecycle -> rollout -> daemon`

## 184. 2026-04-07 第二阶段主线边界压实

- 第二阶段主线只包含三个命令域的全量 Rust 壳承接：
  - `Phase 2-A`：`lifecycle`
  - `Phase 2-B`：`rollout`
  - `Phase 2-C`：`daemon`
- 以下三类事项明确记为阶段外，不计入第二阶段能力范围：
  - 验证线：构建验证、最小联调、smoke、dry-run / non-dry-run 对照、兼容壳转发验证
  - 历史/辅助壳层：`gateway / reconcile / build orchestration`，除非后续确认仍卡主线生产路径
  - 物理退役线：`ps1` 压成纯兼容壳后的外部依赖评估与物理删除时机判断
- 第二阶段的目标是“壳层承接”，不是同步完成：
  - 所有旧壳物理删除
  - 所有验证活动
  - 所有辅助历史脚本清场
- 第二阶段继续采用整包迁移口径，不再按单动作零碎补齐。

## 185. 2026-04-07 `Phase 2-A lifecycle` 整包任务清单立项

- `Phase 2-A lifecycle` 的交付目标固定为：把 `novovm-node-lifecycle.ps1` 从“严格兼容壳 + 可用子集”推进到“纯兼容壳”，由 `novovmctl lifecycle` 全量承接主线生命周期行为。
- 本命令域要求一次性补齐的动作集合：
  - `status`
  - `set-runtime`
  - `set-policy`
  - `start`
  - `stop`
  - `register`
  - `upgrade`
  - `rollback`
- 本命令域内部工作包固定为：
  - CLI 参数面拉平：旧 `ps1` 主线参数映射到 `novovmctl lifecycle`
  - 本地进程/状态承接：`start / stop / status`
  - 注册/治理状态承接：`register / set-runtime / set-policy`
  - 生命周期升级动作承接：`upgrade / rollback`
  - 输出与审计拉平：沿用 `novovmctl` 统一 terminal JSON / JSONL envelope
  - `ps1` 收口：迁完后把 `novovm-node-lifecycle.ps1` 压成纯兼容壳，不再保留任何真实主逻辑
- 本命令域不包含：
  - `gateway / reconcile / build orchestration`
  - 第二阶段验证线
  - 物理删除时机判断
- `Phase 2-A` 完成标准：
  - `novovmctl lifecycle` 承接上述全量动作
  - `novovm-node-lifecycle.ps1` 只做转发与退出码透传
  - 未迁行为不再存在
  - 命令域封口后再进入 `Phase 2-B rollout`

## 186. 2026-04-07 `Phase 2-A lifecycle` 最小验证闭环成立

- `Phase 2-A lifecycle` 已完成主线最小验证闭环，不再停留在“代码承接完成”状态：
  - `register`：通过
  - `start`：通过
  - `status`：通过
  - `stop`：通过
- `novovmctl` 与 `novovm-node` 已完成本轮最小构建验证，`lifecycle` 主链不再依赖旧 PowerShell 生命周期逻辑才能跑通。
- `start` 主链曾暴露的 ingress 单点故障 `NOVOVM_OPS_WIRE_DIR has no .opsw1 files` 已通过 pre-spawn managed ingress bootstrap 打通；最新验证审计显示：
  - `managed_ingress.bootstrap_seeded=true`
  - `managed_ingress.opsw1_count_after_seed=1`
- `novovm-node-lifecycle.ps1` 兼容壳已验证成立：
  - 支持真实 `novovmctl` 产物路径发现
  - 支持主线参数纯转发
  - 对阶段外参数继续显式拒绝
- `stop` clean 收尾已确认成立：
  - 进程已停
  - `pid_file` 已清理
  - 后续 `status` 返回 `running=false`、`pid=null`
- 先前观察到的 `pid_file` 残留属于并行检查读到未完成时刻的中间状态，不是实际 cleanup 缺陷；本轮不需要为此追加 lifecycle 代码修补。
- 结论固定为：`Phase 2-A lifecycle` 已完成“全量 Rust 壳承接 + 主线最小验证闭环”，下一包直接进入 `Phase 2-B rollout`。

## Phase 2-B rollout validation closure note (2026-04-07)

- `novovmctl rollout` now compiles and the compat shell `scripts/novovm-node-rollout.ps1` forwards into Rust without falling back to legacy PowerShell rollout orchestration.
- `status` and `set-policy` passed on the full validation plan fixture.
- `upgrade --dry-run` and `rollback --dry-run` passed on the canary-only rollout validation fixture after batch seeding release/state prerequisites.
- JSON terminal envelope and JSONL audit remained unified across `rollout -> lifecycle`.
- `dry-run` left the lifecycle state hash unchanged for both `upgrade` and `rollback` validation runs.
- The remaining full multi-group gate/state interaction was classified as fixture complexity, not a Rust shell linkage defect.
- Phase 2-B is therefore considered closed for minimum mainline Rust-shell validation.

## Phase 2-C daemon validation closure note (2026-04-07)

- `novovmctl daemon` compiles and the compat shell `scripts/novovm-prod-daemon.ps1` forwards into Rust without falling back to legacy PowerShell daemon logic.
- Minimum validation chain passed:
  - `daemon --dry-run`
  - `daemon --build-before-run --dry-run`
  - `daemon --use-node-watch-mode --lean-io --dry-run`
- `build-before-run` executed through the Rust daemon path and completed successfully when invoked via the release `novovmctl` binary, avoiding the Windows self-lock on the debug executable.
- `dry-run` left lifecycle state unchanged; the lifecycle state SHA256 hash remained stable before and after the daemon validation chain.
- Watch/spool preparation passed in Rust daemon dry-run validation:
  - spool dir created
  - `ops_wire_dir` recorded
  - `ops_wire_watch_drop_failed=true` under `lean_io`
  - `done/failed` directories intentionally omitted under `lean_io`
- JSON terminal envelope and JSONL audit remained unified.
- Phase 2-C is therefore considered closed for minimum mainline Rust-shell validation.
- With Phase 2-A, Phase 2-B, and Phase 2-C all closed, the second-stage mainline Rust shell migration is considered sealed.

## Final closure reference (2026-04-07)

Final closure summary:
- Phase 1 unified Rust policy core: closed
- Phase 2 mainline Rust shell migration: closed
- Formal final summary document:
  - `docs_CN/NOVOVM-RUST-MIGRATION-FINAL-CLOSURE-2026-04-07.md`
