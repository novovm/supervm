# NOVOVM 灰度集中调度控制面手册（2026-04-04）

## 1. 目标

在 `novovmctl rollout` 之上增加集中调度控制面，覆盖：

1. 多计划队列执行
2. 计划级并发限流
3. 跨区域时间窗编排
4. 统一执行认证与审计追踪
5. 高优计划优先级抢占（preemption）
6. 区域容量配额（region capacity quota）
7. 失败自动重试退避（retry backoff）
8. 策略学习化调度（失败率 EMA + 区域拥塞动态调参）
9. 多控制器一致性治理（主备仲裁 + 去重执行）
10. 跨站点控制器共识（异地冲突仲裁 + 全局幂等）
11. 跨站点状态复制与恢复（快照持久化 + 断点恢复 + 冲突回放）
12. 副本健康分级与自动切主（健康面板 + 冷却切主）
13. 自动回切与异地恢复演练模板（稳定回切 + drill 预演）
14. 副本健康 SLO 门槛与自动阻断（评分化治理 + 违规停发）
15. SLO 分级熔断策略（黄灯限流 + 红灯硬阻断）
16. SLO 自适应阈值（按健康趋势动态偏移熔断触发线）
17. 跨站点自动切主策略矩阵（按来源/健康档位/站点优先级决定是否切主）
18. 容灾演练自动评分（演练结果滚动评分 + 通过率趋势）
19. 切主策略与 SLO/演练评分联动（按健康分与演练质量门槛拦截误切主）
20. 跨站点冲突自动判责与降权矩阵（冲突归因 + 责任站点处罚/恢复）
21. 跨站点信誉分长期治理（按时间衰减自动恢复降权）
22. 跨站点信誉分多周期风险预测与自动限流联动（风险升高时自动收紧派发）
23. 高风险站点赢家保护与切主风险联动（风险站点禁止赢得共识或触发切主）
24. 赢家保护与切主风控统一阻断矩阵基线（同源默认、双侧可覆盖）
25. 风险策略角色矩阵默认值（winner/failover 分角色默认阻断）
26. 风险动作矩阵（按风险等级映射并发/发车/阻断动作）
27. 风险动作矩阵按来源分层（startup/cycle 可分开治理）
28. 风险动作矩阵三级覆盖（global->region->site）
29. 风险阻断等级三级覆盖（global->region->site）
30. 切主策略矩阵三级覆盖（global->region->site）
31. 切主联动门槛三级覆盖（slo_link/drill_link/risk_link）
32. 站点优先级三级覆盖（site_priorities 支持 global->region->site）
33. 风险动作矩阵与站点优先级联动（action_matrix 支持 min_site_priority 门槛）
34. 风险动作策略变更审计聚合（`site_risk_throttle_policy` 仅在策略变更时落审计）
35. 灰度决策摘要审计聚合（`rollout_decision_summary` 按来源去重输出）
36. 灰度决策摘要按角色告警分级（L1/L2/L3 固定级别映射）
37. 灰度决策摘要告警通道映射（`decision_alert_channel` 一跳分发字段）
38. 灰度决策摘要告警目标映射（`decision_alert_target` 直接指向通知目标 ID）
39. 灰度决策摘要投递类型映射（`decision_delivery_type/decision_delivery_action` 形成投递指令）
40. 灰度决策摘要真实投递执行（Rust `decision-delivery` 主路径支持 webhook/im/email；脚本兜底仅保留显式 endpoint 的 webhook/im 保守投递）
41. 风险策略模板化与热切换参数收口（`active_profile + policy_profiles + hot_reload`）
42. 跨站点信誉分前瞻预警与自动治理（趋势外推预测 + 提前命中风险动作矩阵）
43. 容灾闭环策略自动收敛（failover 模式自动收敛并发/发车节奏，红灯可阻断）
44. 容灾收敛阈值区域化细化（`failover_converge` 支持 global->region->site 覆盖）
45. 容灾收敛策略模板联动（`risk_policy.policy_profiles.*.failover_converge` 支持随 active_profile 切换）
46. 灰度决策摘要收敛参数透出（`rollout_decision_summary` 增加 active_profile + failover_converge 生效参数）
47. 灰度决策投递链路收敛参数透出（`rollout_decision_delivery` 同步携带 active_profile + failover_converge 生效参数）
48. 灰度决策审计下游导出最小版本（`rollout_decision_summary/delivery` 统一归一化导出 dashboard jsonl）
49. 灰度决策审计导出控制面内置周期化（queue 配置启用 + 热重载生效）
50. 灰度决策看板消费端内置周期化（状态快照 + 阻断告警文件）

主线入口：`novovmctl rollout-control`。`scripts/novovm-node-rollout-control.ps1` 仅保留遗留兼容壳。
决策导出主程序：`novovm-rollout-decision-dashboard-export`（Rust，脚本兜底）。
决策消费主程序：`novovm-rollout-decision-dashboard-consumer`（Rust，脚本兜底）。

## 1.1 Gap-A 收口口径（2026-04-05）

1. 本手册与 `config/runtime/lifecycle/rollout.queue.json` 共同定义 Gap-A 的生产基线。  
2. 主线按“单控制面模板 + 热重载参数”治理，不再扩展并行运维链路。  
3. 后续变更仅允许在现有模板内做参数优化与区域覆盖细化。  
4. 以上口径与 `docs_CN/NOVOVM-MAINLINE-STATUS-GAP-2026-03-22.md` 的 Gap-A 收口状态保持一致。  

## 1.2 Gap-C 口径对齐约束（2026-04-05）

1. 控制面手册不重复定义覆盖层参数实现细节，但必须与主线 Gap-C 口径保持一致。  
2. 生产默认覆盖层模式按统一入口收口：`NOVOVM_OVERLAY_ROUTE_MODE=secure`（可显式切 `fast`）。  
3. 分流与候选集轮换参数口径：`NOVOVM_OVERLAY_ROUTE_REGION`、`NOVOVM_OVERLAY_ROUTE_RELAY_BUCKETS`、`NOVOVM_OVERLAY_ROUTE_RELAY_SET_SIZE`、`NOVOVM_OVERLAY_ROUTE_RELAY_ROTATE_SECONDS`、`NOVOVM_OVERLAY_ROUTE_RELAY_CANDIDATES`。  
4. 落标字段口径：`overlay_route_mode/overlay_route_region/overlay_route_relay_bucket/overlay_route_relay_set_size/overlay_route_relay_round/overlay_route_relay_index/overlay_route_relay_id`（锚点/gateway/plugin 同口径）。  
5. 若控制面策略或模板影响覆盖层参数，必须先更新 `docs_CN/NOVOVM-UNIFIED-ENTRYPOINT-AND-RUNBOOK-2026-03-22.md` 与 `docs_CN/NOVOVM-MAINLINE-STATUS-GAP-2026-03-22.md` 后再生效。  
6. 控制面计划支持覆盖层 runtime 下发：`plans[].overlay_route_mode`、`plans[].overlay_route_runtime_file`、`plans[].overlay_route_runtime_profile`、`plans[].overlay_route_relay_directory_file`、`plans[].overlay_route_relay_health_min`、`plans[].overlay_route_relay_penalty_state_file`、`plans[].overlay_route_relay_penalty_delta`、`plans[].overlay_route_relay_penalty_recover_per_run`、`plans[].overlay_route_auto_penalty_enabled`、`plans[].overlay_route_auto_penalty_step`、`plans[].overlay_route_relay_health_refresh_enabled`、`plans[].overlay_route_relay_health_refresh_mode`、`plans[].overlay_route_relay_health_refresh_timeout_ms`、`plans[].overlay_route_relay_health_refresh_alpha`、`plans[].overlay_route_relay_health_refresh_cooldown_seconds`、`plans[].overlay_route_relay_discovery_enabled`、`plans[].overlay_route_relay_discovery_file`、`plans[].overlay_route_relay_discovery_http_urls`、`plans[].overlay_route_relay_discovery_http_urls_file`、`plans[].overlay_route_relay_discovery_seed_region`、`plans[].overlay_route_relay_discovery_seed_mode`、`plans[].overlay_route_relay_discovery_seed_profile`、`plans[].overlay_route_relay_discovery_seed_failover_state_file`、`plans[].overlay_route_relay_discovery_seed_priority`、`plans[].overlay_route_relay_discovery_seed_success_rate_threshold`、`plans[].overlay_route_relay_discovery_seed_cooldown_seconds`、`plans[].overlay_route_relay_discovery_seed_max_consecutive_failures`、`plans[].overlay_route_relay_discovery_region_priority`、`plans[].overlay_route_relay_discovery_region_failover_threshold`、`plans[].overlay_route_relay_discovery_region_cooldown_seconds`、`plans[].overlay_route_relay_discovery_relay_score_smoothing_alpha`、`plans[].overlay_route_relay_discovery_source_weights`、`plans[].overlay_route_relay_discovery_http_timeout_ms`、`plans[].overlay_route_relay_discovery_source_reputation_file`、`plans[].overlay_route_relay_discovery_source_decay`、`plans[].overlay_route_relay_discovery_source_penalty_on_fail`、`plans[].overlay_route_relay_discovery_source_recover_on_success`、`plans[].overlay_route_relay_discovery_source_blacklist_threshold`、`plans[].overlay_route_relay_discovery_source_denylist`、`plans[].overlay_route_relay_discovery_cooldown_seconds`、`plans[].overlay_route_relay_discovery_default_health`、`plans[].overlay_route_relay_discovery_default_enabled`、`plans[].overlay_route_relay_candidates`、`plans[].overlay_route_relay_candidates_by_region`、`plans[].overlay_route_relay_candidates_by_role`。  

## 2. 队列模板

默认队列文件：`config/runtime/lifecycle/rollout.queue.json`。

关键字段：

1. `max_concurrent_plans`：最多并发执行的计划数量
2. `poll_seconds`：进程轮询间隔
3. `dispatch_pause_seconds`：计划发车间隔
4. `plans[]`：每个区域/批次一条计划
5. `plans[].region_window`：计划级窗口（`HH:MM-HH:MM UTC`）
6. `plans[].plan_file`：底层灰度计划文件（传给 `novovmctl rollout`）
7. `enable_priority_preemption`：是否开启优先级抢占
8. `preempt_requeue_seconds`：被抢占计划回队列等待秒数
9. `region_capacities`：区域并发配额（例如 `CN/EU/DEFAULT`）
10. `plans[].priority`：计划优先级（数字越大优先级越高）
11. `plans[].preemptible`：是否允许被抢占
12. `plans[].retry_max_attempts`：失败重试次数上限
13. `plans[].retry_backoff_seconds`：重试初始退避秒数
14. `plans[].retry_backoff_factor`：重试退避倍数
15. `plans[].overlay_route_mode`：本计划覆盖层模式（`secure|fast`，可选）
16. `plans[].overlay_route_runtime_file`：本计划覆盖层模板文件（可选，默认走统一入口默认路径）
17. `plans[].overlay_route_runtime_profile`：本计划覆盖层模板 profile（可选，默认跟随统一入口 profile）
18. `plans[].overlay_route_relay_candidates`：本计划显式中继候选集（数组或逗号分隔字符串），通过 rollout/lifecycle 下发为 `NOVOVM_OVERLAY_ROUTE_RELAY_CANDIDATES`
19. `plans[].overlay_route_relay_candidates_by_region`：本计划区域级候选集映射（JSON 对象，键为区域，支持 `default`）
20. `plans[].overlay_route_relay_candidates_by_role`：本计划角色级候选集映射（JSON 对象，键为 `full|l1|l2|l3`，支持 `default`）
21. `plans[].overlay_route_relay_directory_file`：本计划中继目录文件路径（JSON）
22. `plans[].overlay_route_relay_health_min`：本计划目录筛选健康阈值（`0..1`）
23. `plans[].overlay_route_relay_penalty_state_file`：惩罚状态文件路径（保存 relay 惩罚分）
24. `plans[].overlay_route_relay_penalty_delta`：惩罚增量映射（`{ relay_id: delta }`，失败可增罚）
25. `plans[].overlay_route_relay_penalty_recover_per_run`：每次运行恢复步长（`0..1`，越大恢复越快）
26. 候选集优先级：`overlay_route_relay_candidates` > `overlay_route_relay_candidates_by_region` > `overlay_route_relay_candidates_by_role` > `overlay_route_relay_directory_file/overlay_route_relay_health_min`（经惩罚修正）> 模板候选集
补充：`plans[].overlay_route_auto_penalty_enabled`：失败重试时自动对目标 relay 追加惩罚增量（写入 `overlay_route_relay_penalty_delta`）。
补充：`plans[].overlay_route_auto_penalty_step`：自动惩罚步长（`0..1`），与 `overlay_route_relay_penalty_recover_per_run` 共同形成惩罚-恢复闭环。
补充：若计划未显式设置 `overlay_route_auto_penalty_*`，且计划指定了 `overlay_route_runtime_file + overlay_route_runtime_profile`，控制面会回退读取 profile 的 `auto_penalty_enabled/auto_penalty_step` 作为默认值。
补充：失败重试时有效惩罚步长 = `base_step × streak_boost × health_factor`，其中 `streak_boost` 随连续失败次数上升，`health_factor` 基于中继目录 `health` 联动调节（低健康加速降权，高健康减缓惩罚）。
补充：`plans[].overlay_route_relay_health_refresh_*`：控制面在派发前按冷却周期调用 Rust health refresh 二进制（`novovm-overlay-relay-health-refresh`）刷新目录 health；若二进制缺失则回退 `scripts/novovm-overlay-relay-health-refresh.ps1`（失败不阻断派发，只写审计）。  
补充：`plans[].overlay_route_relay_health_refresh_binary_path`：可选指定 health refresh Rust 二进制路径（未指定时控制面按 `target/release` 与 `target/debug` 默认路径探测）。  
补充：若计划未显式设置 `overlay_route_relay_health_refresh_*`，且计划指定了 `overlay_route_runtime_file + overlay_route_runtime_profile`，控制面会回退读取 profile 默认值。
补充：`plans[].overlay_route_relay_discovery_*`：控制面在派发前按冷却周期调用 Rust discovery merge 二进制（`novovm-overlay-relay-discovery-merge`）将发现源合并到目录；若二进制缺失则回退 `scripts/novovm-overlay-relay-discovery-merge.ps1`（失败不阻断派发，只写审计）。  
补充：`plans[].overlay_route_relay_discovery_http_urls`：可选 HTTP 发现源列表（逗号/分号分隔），与本地 `discovery_file` 一起并入目录。
补充：`plans[].overlay_route_relay_discovery_source_weights`：可选源权重映射（JSON 对象）；支持按 `source` 或 URL `host` 配置权重，用于同 relay 多源冲突时的优先与加权健康计算。
补充：`plans[].overlay_route_relay_discovery_http_timeout_ms`：HTTP 发现源拉取超时（毫秒）。
补充：`plans[].overlay_route_relay_discovery_source_reputation_file`：来源信誉状态文件（持久化 source->score）。
补充：`plans[].overlay_route_relay_discovery_source_decay`：来源信誉每周期向 1.0 衰减恢复步长（`0..1`）。
补充：`plans[].overlay_route_relay_discovery_source_penalty_on_fail`：HTTP 来源拉取失败惩罚步长（`0..1`）。
补充：`plans[].overlay_route_relay_discovery_source_recover_on_success`：HTTP 来源拉取成功恢复步长（`0..1`）。
补充：`plans[].overlay_route_relay_discovery_source_blacklist_threshold`：来源信誉黑名单阈值（低于阈值直接跳过该来源）。
补充：`plans[].overlay_route_relay_discovery_source_denylist`：来源拒绝名单（支持 source 名与 URL host）。  
补充：`plans[].overlay_route_relay_discovery_binary_path`：可选指定 discovery merge Rust 二进制路径（未指定时控制面按 `target/release` 与 `target/debug` 默认路径探测）。  
补充：`plans[].overlay_route_relay_discovery_http_urls_file`：发现源 seed 文件（运行中可热更新，控制面每次触发 discovery 读取最新内容）。  
补充：`plans[].overlay_route_relay_discovery_seed_region`：seed 分层选择 region（为空时默认使用计划 `region`）。
补充：`plans[].overlay_route_relay_discovery_seed_mode`：seed 分层选择 mode（为空时默认使用计划 `overlay_route_mode`）。
补充：`plans[].overlay_route_relay_discovery_seed_profile`：seed 分层选择 profile（为空时默认使用计划 `overlay_route_runtime_profile`）。
补充：`plans[].overlay_route_relay_discovery_seed_failover_state_file`：seed 故障切换状态文件（持久化成功/失败/连续失败/冷却截止）。
补充：`plans[].overlay_route_relay_discovery_seed_priority`：seed 优先级映射（JSON，对 `source` 或 `host` 生效，支持 `__default__`）。
补充：`plans[].overlay_route_relay_discovery_seed_success_rate_threshold`：成功率阈值（低于阈值触发降级）。
补充：`plans[].overlay_route_relay_discovery_seed_cooldown_seconds`：降级冷却时长（秒）。
补充：`plans[].overlay_route_relay_discovery_seed_max_consecutive_failures`：连续失败上限（达到后触发降级）。
补充：seed 故障切换行为：优先级选择 -> 阈值/连败降级 -> 冷却后恢复候选；审计透出 `overlay_route_relay_discovery_seed_selected/seed_failover_reason/seed_recover_at_unix_ms`。
补充：`plans[].overlay_route_relay_discovery_region_priority`：区域优先级映射（JSON，支持 `__default__`）。
补充：`plans[].overlay_route_relay_discovery_region_failover_threshold`：区域降级阈值（按区域 relay_score 均值判定）。
补充：`plans[].overlay_route_relay_discovery_region_cooldown_seconds`：区域降级冷却时长（秒）。
补充：`plans[].overlay_route_relay_discovery_relay_score_smoothing_alpha`：relay_score 平滑系数（`0.01..1`，越大越敏捷，越小越稳态）。
补充：区域故障切换行为：按 `region_priority` 选区，同区按 `relay_score` 选中继；区域降级后进入冷却，冷却到期恢复候选；审计透出 `overlay_route_relay_discovery_relay_selected/relay_score/region_failover_reason/region_recover_at_unix_ms`。
补充：若计划未显式设置 `overlay_route_relay_discovery_*`，且计划指定了 `overlay_route_runtime_file + overlay_route_runtime_profile`，控制面会回退读取 profile 默认值。
27. `adaptive_policy.enabled`：启用学习化调度
28. `adaptive_policy.state_file`：策略状态文件（持久化 EMA）
29. `adaptive_policy.alpha`：EMA 衰减系数
30. `adaptive_policy.high_failure_rate`：高失败率阈值（触发降并发）
31. `adaptive_policy.low_failure_rate`：低失败率阈值（结合拥塞触发升并发）
32. `adaptive_policy.max_cap_boost`：区域并发最大增量
21. `controller_governance.primary_id`：主控制器 ID
22. `controller_governance.standby_ids`：备控制器 ID 列表
23. `controller_governance.allow_standby_takeover`：是否允许备接管
24. `controller_governance.lease_file`：主备仲裁租约文件
25. `controller_governance.dedupe_file`：去重执行状态文件
26. `controller_governance.dedupe_ttl_seconds`：去重状态 TTL
27. `site_consensus.enabled`：启用跨站点共识
28. `site_consensus.site_id`：当前站点 ID
29. `site_consensus.required_sites`：共识最小站点数
30. `site_consensus.vote_ttl_seconds`：投票有效期
31. `site_consensus.retry_seconds`：未达成仲裁时回队列等待秒数
32. `site_consensus.state_file`：跨站点共识状态文件
33. `site_consensus.site_priorities`：冲突仲裁优先级（兼容旧版 `site->priority`，新版支持三级覆盖结构）
34. `state_recovery.enabled`：启用状态复制与恢复
35. `state_recovery.snapshot_file`：控制面状态快照文件
36. `state_recovery.replay_file`：冲突回放事件文件
37. `state_recovery.replay_max_entries`：回放文件最大保留条数
38. `state_recovery.snapshot_replica_files[]`：快照副本文件列表（跨机房双写）
39. `state_recovery.replay_replica_files[]`：回放副本文件列表（跨机房双写）
40. `state_recovery.enable_replica_validation`：启用副本一致性校验
41. `state_recovery.replica_validation_interval_seconds`：副本校验周期秒数
42. `state_recovery.replica_allowed_lag_entries`：回放副本允许滞后条数
43. `state_recovery.resume_from_snapshot`：启动时按快照恢复未完成计划
44. `state_recovery.replay_conflicts_on_start`：启动时重放冲突事件
45. `state_recovery.enable_replica_auto_failover`：启用副本自动切主
46. `state_recovery.replica_health_file`：副本健康状态文件
47. `state_recovery.replica_failover_cooldown_seconds`：切主冷却时间
48. `state_recovery.replica_failover_on_startup`：启动阶段允许自动切主
49. `state_recovery.enable_replica_switchback`：启用自动回切策略
50. `state_recovery.replica_switchback_stable_cycles`：回切前连续稳定校验次数
51. `state_recovery.replica_drill.enabled`：启用异地恢复演练（只产生日志，不改写状态）
52. `state_recovery.replica_drill.drill_id`：演练批次标识
53. `state_recovery.slo.enabled`：启用副本健康 SLO
54. `state_recovery.slo.file`：SLO 状态文件
55. `state_recovery.slo.window_samples`：滚动评分窗口样本数
56. `state_recovery.slo.min_green_rate`：最小绿灯占比阈值
57. `state_recovery.slo.max_red_in_window`：窗口内允许红灯上限
58. `state_recovery.slo.block_on_violation`：违规时是否阻断调度
59. `state_recovery.slo.circuit_breaker.enabled`：启用 SLO 分级熔断
60. `state_recovery.slo.circuit_breaker.yellow_max_concurrent_plans`：黄灯并发上限
61. `state_recovery.slo.circuit_breaker.yellow_dispatch_pause_seconds`：黄灯发车间隔秒数
62. `state_recovery.slo.circuit_breaker.red_block`：红灯是否硬阻断
63. `state_recovery.slo.circuit_breaker.matrix[]`：多级熔断矩阵（按 score 匹配）
64. `matrix[].name`：规则名
65. `matrix[].min_score/max_score`：规则命中区间（左闭右开）
66. `matrix[].max_concurrent_plans`：该档并发上限
67. `matrix[].dispatch_pause_seconds`：该档发车间隔
68. `matrix[].block_dispatch`：该档是否阻断派发
69. `state_recovery.slo.adaptive.enabled`：启用 SLO 自适应阈值
70. `state_recovery.slo.adaptive.file`：自适应状态文件
71. `state_recovery.slo.adaptive.step`：每次偏移步长（score）
72. `state_recovery.slo.adaptive.good_score`：健康阈值（高于该值偏移放宽）
73. `state_recovery.slo.adaptive.bad_score`：风险阈值（低于该值偏移收紧）
74. `state_recovery.slo.adaptive.max_shift`：最大偏移绝对值
75. `state_recovery.failover_policy.enabled`：启用自动切主策略矩阵
76. `state_recovery.failover_policy.default_allow`：矩阵未命中时是否默认放行
77. `state_recovery.failover_policy.matrix[]`：自动切主策略规则列表
78. `matrix[].name`：策略规则名
79. `matrix[].source`：触发来源（`startup|cycle|*`）
80. `matrix[].grades`：匹配健康档位（`green|yellow|red|*`）
81. `matrix[].min_site_priority`：最小站点优先级门槛
82. `matrix[].allow_auto_failover`：该规则是否允许自动切主
83. `matrix[].cooldown_seconds`：命中规则时覆盖切主冷却秒数
84. `state_recovery.replica_drill.score.enabled`：启用容灾演练自动评分
85. `state_recovery.replica_drill.score.file`：演练评分状态文件
86. `state_recovery.replica_drill.score.window_samples`：演练评分滚动窗口
87. `state_recovery.replica_drill.score.pass_score`：演练通过分阈值
88. `state_recovery.failover_policy.slo_link.enabled`：启用切主策略与 SLO 联动
89. `state_recovery.failover_policy.slo_link.min_effective_score`：允许切主的最小有效分
90. `state_recovery.failover_policy.slo_link.block_on_violation`：SLO 违规时是否直接阻断切主
91. `state_recovery.failover_policy.drill_link.enabled`：启用切主策略与演练评分联动
92. `state_recovery.failover_policy.drill_link.min_pass_rate`：演练最小通过率
93. `state_recovery.failover_policy.drill_link.min_average_score`：演练最小平均分
94. `state_recovery.failover_policy.drill_link.require_last_pass`：是否要求最近一次演练通过
95. `site_consensus.accountability.enabled`：启用跨站点冲突自动判责
96. `site_consensus.accountability.state_file`：判责降权状态文件
97. `site_consensus.accountability.max_penalty_points`：站点降权累计上限
98. `site_consensus.accountability.recovery_per_win`：获胜自动恢复点数
99. `site_consensus.accountability.matrix[]`：判责处罚矩阵
100. `matrix[].event/role/site`：规则匹配键（事件/角色/站点）
101. `matrix[].penalty_points`：处罚点（正数降权，负数恢复）
102. `site_consensus.accountability.reputation.enabled`：启用信誉分长期治理
103. `site_consensus.accountability.reputation.aging_interval_seconds`：信誉恢复周期秒数
104. `site_consensus.accountability.reputation.recover_points_per_interval`：每周期恢复点数
105. `site_consensus.accountability.reputation.recover_idle_seconds`：最近处罚静默期门槛
106. `site_consensus.accountability.risk.enabled`：启用多周期风险预测
107. `site_consensus.accountability.risk.state_file`：风险预测状态文件
108. `site_consensus.accountability.risk.ema_alpha`：风险 EMA 系数
109. `site_consensus.accountability.risk.auto_throttle.enabled`：启用风险自动限流
110. `risk.auto_throttle.yellow_max_concurrent_plans`：黄灯并发上限
111. `risk.auto_throttle.yellow_dispatch_pause_seconds`：黄灯发车间隔
112. `risk.auto_throttle.orange_max_concurrent_plans`：橙灯并发上限
113. `risk.auto_throttle.orange_dispatch_pause_seconds`：橙灯发车间隔
114. `risk.auto_throttle.red_block`：红灯是否硬阻断派发
115. `site_consensus.accountability.risk.winner_guard.enabled`：启用高风险赢家保护
116. `site_consensus.accountability.risk.winner_guard.blocked_levels`：禁止成为赢家的风险等级（可选覆盖，缺省继承 `risk_policy.blocked_levels`）
117. `site_consensus.accountability.risk.winner_guard.fallback_allow_when_all_blocked`：全阻断时是否允许回退选择
118. `state_recovery.failover_policy.risk_link.enabled`：启用切主风险联动
119. `state_recovery.failover_policy.risk_link.blocked_levels`：禁止触发切主的风险等级（可选覆盖，缺省继承 `risk_policy.blocked_levels`）
120. `risk_policy.blocked_levels`：统一风险阻断基线（`winner_guard/risk_link` 未显式配置时继承）
121. `risk_policy.winner_guard_blocked_levels`：赢家保护角色默认阻断等级（优先于 `risk_policy.blocked_levels`，低于 `winner_guard.blocked_levels`）
122. `risk_policy.failover_risk_link_blocked_levels`：切主风控角色默认阻断等级（优先于 `risk_policy.blocked_levels`，低于 `risk_link.blocked_levels`）
123. `risk_policy.action_matrix[]`：风险动作矩阵（按 `level` 定义限流与阻断动作）
124. `risk_policy.action_matrix[].level`：风险等级（`yellow|orange|red`）
125. `risk_policy.action_matrix[].cap_concurrent`：该等级并发上限（0 表示不限制）
126. `risk_policy.action_matrix[].pause_seconds`：该等级最小发车间隔秒数
127. `risk_policy.action_matrix[].block_dispatch`：该等级是否阻断派发
128. `risk_policy.action_matrix[].source`：动作来源（`startup|cycle|*`，缺省为 `*`）
129. `risk_policy.action_matrix[].min_site_priority`：该动作最小站点优先级门槛（缺省不限制）
130. `risk_policy.site_region_map`：站点到区域映射（用于 region 覆盖判定）
131. `risk_policy.region_action_matrix_overrides`：区域级动作矩阵覆盖（键为区域）
132. `risk_policy.site_action_matrix_overrides`：站点级动作矩阵覆盖（键为站点）
133. `risk_policy.region_winner_guard_blocked_levels`：区域级赢家保护阻断等级覆盖
134. `risk_policy.site_winner_guard_blocked_levels`：站点级赢家保护阻断等级覆盖
135. `risk_policy.region_failover_risk_link_blocked_levels`：区域级切主风控阻断等级覆盖
136. `risk_policy.site_failover_risk_link_blocked_levels`：站点级切主风控阻断等级覆盖
137. `state_recovery.failover_policy.region_matrix_overrides`：区域级切主策略矩阵覆盖
138. `state_recovery.failover_policy.site_matrix_overrides`：站点级切主策略矩阵覆盖
139. `state_recovery.failover_policy.slo_link.region_overrides`：区域级 SLO 联动门槛覆盖
140. `state_recovery.failover_policy.slo_link.site_overrides`：站点级 SLO 联动门槛覆盖
141. `state_recovery.failover_policy.drill_link.region_overrides`：区域级演练联动门槛覆盖
142. `state_recovery.failover_policy.drill_link.site_overrides`：站点级演练联动门槛覆盖
143. `state_recovery.failover_policy.risk_link.region_overrides`：区域级风险联动开关覆盖
144. `state_recovery.failover_policy.risk_link.site_overrides`：站点级风险联动开关覆盖
145. `site_consensus.site_priorities.global_default`：全局默认站点优先级
146. `site_consensus.site_priorities.region_overrides`：区域级站点优先级覆盖
147. `site_consensus.site_priorities.site_overrides`：站点级站点优先级覆盖
148. `risk_policy.alert_channel_targets`：告警通道到通知目标 ID 的映射（用于 `decision_alert_target`）
149. `risk_policy.alert_target_delivery_types`：通知目标 ID 到投递类型的映射（用于 `decision_delivery_type`）
150. `risk_policy.delivery_webhook_endpoints`：目标 ID 到 webhook 地址映射（投递类型 `webhook`）
151. `risk_policy.delivery_im_endpoints`：目标 ID 到 IM 机器人 webhook 地址映射（投递类型 `im`）
152. `risk_policy.delivery_email_targets`：目标 ID 到收件邮箱映射（投递类型 `email`）
153. `risk_policy.delivery_email.smtp_server/smtp_port/from/use_ssl`：邮件投递 SMTP 基本配置
154. `risk_policy.delivery_email.smtp_user/smtp_password_env`：邮件投递认证配置（密码从环境变量读取）
155. `risk_policy.active_profile`：当前启用策略模板名（未命中回退 `base`）
156. `risk_policy.policy_profiles`：策略模板集合（可覆盖 blocked/action_matrix/告警通道与投递配置）
157. `risk_policy.hot_reload.enabled/check_seconds`：运行中热重载开关与轮询秒数（按 queue 文件变更触发）
158. `state_recovery.failover_converge.enabled`：启用 failover 模式下的收敛治理
159. `state_recovery.failover_converge.max_concurrent_plans`：failover 收敛并发上限
160. `state_recovery.failover_converge.min_dispatch_pause_seconds`：failover 收敛最小发车间隔秒数
161. `state_recovery.failover_converge.block_on_snapshot_red`：snapshot 为 red 时是否触发收敛阻断判定
162. `state_recovery.failover_converge.block_on_replay_red`：replay 为 red 时是否触发收敛阻断判定
163. `state_recovery.failover_converge.region_overrides`：区域级 failover 收敛参数覆盖
164. `state_recovery.failover_converge.site_overrides`：站点级 failover 收敛参数覆盖
165. `risk_policy.policy_profiles.<name>.failover_converge`：按策略模板覆盖收敛参数（优先级高于 `state_recovery.failover_converge`）
166. `risk_policy.decision_dashboard_export.enabled`：是否启用控制面内置决策导出
167. `risk_policy.decision_dashboard_export.check_seconds`：导出轮询周期秒数
168. `risk_policy.decision_dashboard_export.mode`：导出类型（`delivery|summary|both`）
169. `risk_policy.decision_dashboard_export.tail`：导出输入审计尾部条数（0 表示全量）
170. `risk_policy.decision_dashboard_export.since_utc`：按 UTC 时间下限过滤
171. `risk_policy.decision_dashboard_export.audit_file/output_file/binary_file/script_file`：导出输入/输出/二进制/脚本路径（Rust 优先，脚本兜底）
172. `risk_policy.policy_profiles.<name>.decision_dashboard_export`：按策略模板覆盖导出参数（随 `active_profile` 热切换）
173. `risk_policy.decision_dashboard_consumer.enabled`：是否启用控制面内置看板消费
174. `risk_policy.decision_dashboard_consumer.check_seconds`：看板消费轮询周期秒数
175. `risk_policy.decision_dashboard_consumer.mode`：消费模式（`all|blocked`）
176. `risk_policy.decision_dashboard_consumer.tail`：消费输入尾部条数（0 表示全量）
177. `risk_policy.decision_dashboard_consumer.input_file`：消费输入文件（通常为 decision dashboard jsonl）
178. `risk_policy.decision_dashboard_consumer.output_file`：看板状态快照输出文件
179. `risk_policy.decision_dashboard_consumer.alerts_file`：阻断告警输出文件（jsonl）
180. `risk_policy.decision_dashboard_consumer.binary_file/script_file`：看板消费二进制/脚本路径（Rust 优先，脚本兜底）
181. `risk_policy.policy_profiles.<name>.decision_dashboard_consumer`：按策略模板覆盖看板消费参数（随 `active_profile` 热切换）
182. `risk_policy.decision_route.binary_file`：灰度决策级别/通道/目标/投递类型路由二进制路径（Rust 优先，脚本兜底）
183. `risk_policy.policy_cli.binary_file`：统一策略入口二进制路径，指向 `novovm-rollout-policy`；未显式配置单工具二进制时，控制面默认通过它分发 `dashboard/rollout/risk/overlay/failover` 策略子命令。
补充：当前 `policy_cli` 同时支持两种入口形态。
1. 新树形入口：`novovm-rollout-policy overlay relay-discovery-merge ...`
2. 旧兼容入口：`novovm-rollout-policy overlay-relay-discovery-merge ...`
控制面当前仍可继续走旧兼容入口，后续再逐步切到树形子命令。
184. `risk_policy.decision_delivery.binary_file`：灰度决策真实投递执行二进制路径；显式配置时优先于 `policy_cli`，未配置则走统一入口分发。
补充：`decision-route / decision-delivery / decision-dashboard-export / decision-dashboard-consumer` 的真实实现已统一迁入 `crates/novovm-rollout-policy/src/policy/rollout/*`；旧独立二进制只保留兼容包装，主路径已由统一入口直接执行共享模块。
补充：legacy 平铺 tool 名（如 `rollout-decision-route`、`risk-action-eval`、`overlay-relay-discovery-merge`）现在也只作为兼容口径存在，统一入口内部会直接分发到共享模块，不再经第二层 sibling bin 中转。
185. `risk_policy.risk_action_eval.binary_file`：风险动作矩阵评估二进制路径；显式配置时优先于 `policy_cli`。
186. `risk_policy.risk_action_matrix_build.binary_file`：风险动作矩阵规范化构建二进制路径；显式配置时优先于 `policy_cli`。
187. `risk_policy.failover_policy_matrix_build.binary_file`：切主策略矩阵规范化构建二进制路径；显式配置时优先于 `policy_cli`。
188. `risk_policy.risk_matrix_select.binary_file`：站点/区域/全局风险动作矩阵选择二进制路径；显式配置时优先于 `policy_cli`。
189. `risk_policy.risk_blocked_select.binary_file`：站点/区域/全局风险阻断集合选择二进制路径；显式配置时优先于 `policy_cli`。
190. `risk_policy.risk_blocked_map_build.binary_file`：风险阻断集合覆盖映射构建二进制路径；显式配置时优先于 `policy_cli`。
191. `risk_policy.risk_level_set.binary_file`：风险等级集合规范化二进制路径；显式配置时优先于 `policy_cli`。
192. `risk_policy.profile_select.binary_file`：风险策略模板选择二进制路径；显式配置时优先于 `policy_cli`。
193. 策略二进制产物已从 `novovm-node` 收口到独立 crate：`crates/novovm-rollout-policy`，并新增统一分发入口 `novovm-rollout-policy`。

## 3. 基础执行（升级）

```powershell
novovmctl rollout-control `
  --plan-action upgrade `
  --queue-file .\config\runtime\lifecycle\rollout.queue.json `
  --controller-id local-controller `
  --operation-id rollout-20260404-upgrade `
  --audit-file .\artifacts\runtime\rollout\control-plane.audit.jsonl
```

说明：

1. `site_consensus/state_recovery/risk_policy` 等调度参数统一写入 `config/runtime/lifecycle/rollout.queue.json`。
2. 主线路径默认通过 `novovmctl rollout-control -> novovm-rollout-policy -> novovmctl rollout -> novovmctl lifecycle` 执行。

## 4. 跨区域窗口门控

默认强制执行窗口门控：

1. 不在 `plans[].region_window` 的计划会被阻断并记审计。
2. 计划内节点仍受 `upgrade_window` 二次门控（由底层 rollout 脚本执行）。

需要临时绕过计划窗口时，显式加：

```powershell
-IgnoreUpgradeWindow
```

## 5. 认证与凭据

1. 控制器准入：`rollout.plan.json` 的 `controllers.allowed_ids`。
2. SSH：支持 `SshIdentityFile`、`SshKnownHostsFile`、`SshStrictHostKeyChecking`。
3. WinRM：优先从环境变量读取账号密码（不落文件）：
   - `NOVOVM_WINRM_USER`
   - `NOVOVM_WINRM_PASS`

## 6. 并发与失败策略

1. 计划级并发：`-MaxConcurrentPlans` 或队列文件 `max_concurrent_plans`。
2. 节点级失败阈值：由每个 `rollout.plan.json` 的 `groups[].max_failures` 控制。
3. 计划失败策略：
   - 默认失败即停后续计划
   - `-ContinueOnPlanFailure` 允许继续后续计划
4. 抢占策略：高优计划可抢占低优且 `preemptible=true` 的运行计划。
5. 区域配额：同一区域并发运行数不会超过 `region_capacities` 对应上限。
6. 退避重试：失败后按 `retry_backoff_seconds * retry_backoff_factor^(attempt-1)` 回队列。
7. 学习化调度：基于区域失败率 EMA 动态收敛并发与重试节奏。

## 7. 审计文件

1. 控制面审计：默认 `artifacts/runtime/rollout/control-plane-audit.jsonl`
2. 计划内审计：由队列中每条计划的 `audit_file` 指定

建议固定 `ControllerId + OperationId`，便于全链路追踪。

## 8. 多控制器一致性治理

1. 主备仲裁：通过 `controller_governance.primary_id/standby_ids` 定义角色。
2. 租约锁：控制面启动时抢占 `lease_file` 独占锁，未抢到即退出，避免双主并发。
3. 备接管：`allow_standby_takeover=true` 时，备用控制器可在主不活跃时接管。
4. 去重执行：`dedupe_file` 对同一计划动作做幂等保护，避免重复派发。

## 9. 跨站点控制器共识

1. 启用 `site_consensus.enabled=true` 后，计划派发前会执行站点投票。
2. 达到 `required_sites` 后按 `site_priorities` 选出胜出操作并提交全局幂等记录。
3. 未达成票数会返回 `consensus_wait` 并按 `retry_seconds` 回队列。
4. 非胜出站点会收到 `consensus_blocked`，避免异地重复执行。
5. `site_consensus.accountability.enabled=true` 时，控制面会对冲突票据自动判责。
6. 默认事件包括：`consensus_conflict_loser`、`consensus_committed_other`、`consensus_winner`。
7. 判责命中矩阵后会更新站点 `penalty_points`，并实时影响后续投票优先级（降权）。
8. 每次判责会落审计 `consensus_accountability`，可追踪 rule/delta/effective_priority。
9. 启用 `accountability.reputation.enabled=true` 后，会按周期对“静默站点”自动恢复 penalty_points。
10. 恢复事件会落审计 `consensus_accountability_reputation_aging`，并同步更新 reputation_score。
11. 启用 `accountability.risk.enabled=true` 后，会基于 penalty/reputation 计算多周期风险 EMA。
12. 启用 `risk.auto_throttle.enabled=true` 后，按 `green/yellow/orange/red` 自动收紧并发与发车间隔。
13. 红灯且 `red_block=true` 时会产生日志 `site_risk_throttle_blocked` 并阻断新派发。
14. `risk.winner_guard.enabled=true` 时，`blocked_levels` 命中的站点不能成为共识赢家。
15. 若所有候选站点都被风险保护命中，且 `fallback_allow_when_all_blocked=true`，允许回退选择赢家。
16. `failover_policy.risk_link.enabled=true` 时，高风险站点会被阻断自动切主，日志含 `risk_gate`。
17. 未显式设置 `winner_guard.blocked_levels` 或 `risk_link.blocked_levels` 时，会继承 `risk_policy.blocked_levels`。
18. 若设置 `risk_policy.winner_guard_blocked_levels` / `risk_policy.failover_risk_link_blocked_levels`，则按角色默认覆盖统一基线。
19. 若设置 `risk_policy.action_matrix`，风险限流动作按矩阵执行；未设置时保持黄/橙/红固定参数兼容逻辑。
20. 若 `action_matrix` 同时存在 `source=*` 与 `source=startup/cycle`，同等级下优先命中来源专用规则。
21. `source=startup` 规则在启动阶段风险预测后先执行，可直接限制首轮并发/发车或阻断首轮派发。
22. 覆盖优先级：`site_action_matrix_overrides` > `region_action_matrix_overrides` > `action_matrix`。
23. 区域覆盖依赖 `site_region_map` 把 `worst_site_id` 映射到区域后生效。
24. 阻断等级覆盖优先级：`site_*_blocked_levels` > `region_*_blocked_levels` > 角色默认/全局默认。
25. 控制面会读取 `worst_site_id` 的风险 `trend` 并外推 `forecast_score/forecast_level`（上行趋势按保守系数放大）。
26. 当预测等级高于当前等级且进入 `orange/red`，会输出 `consensus_accountability_risk_forecast` 审计事件。
27. 预测风险同样会命中风险动作矩阵提前治理；若命中阻断规则，会输出 `site_risk_forecast_throttle_blocked` 并阻断派发。
28. 控制面在 `failover mode` 下按 `state_recovery.failover_converge` 自动收敛派发节奏（默认保守值为并发 1 + 冷却秒数）。
29. 收敛参数覆盖优先级：`risk_policy.policy_profiles.<active>.failover_converge`（可含 site/region 覆盖）> `state_recovery.failover_converge`（site/region/global）。
30. 收敛策略变化会输出审计 `replica_failover_converge`（去重，含 `converge_scope` 字段）便于追踪收敛轨迹。
31. 若 snapshot/replay 命中收敛阻断条件且 `red_block=true`，会输出 `replica_failover_converge_blocked` 并阻断派发。

## 10. 跨站点状态复制与恢复

1. 控制面会把当前 `pending/running/done` 状态写入 `state_recovery.snapshot_file`。
2. 控制面重启后可按 `resume_from_snapshot=true` 恢复未完成计划，并把上次运行中的计划按重试回队。
3. 控制面会把 `dedupe_blocked/consensus_wait/consensus_blocked` 写入 `state_recovery.replay_file`。
4. 启用 `replay_conflicts_on_start=true` 后，重启时会读取回放文件，把冲突计划重新进入调度。
5. 回放文件启动时按 `replay_max_entries` 截断，避免无限增长。
6. 配置 `snapshot_replica_files/replay_replica_files` 后，控制面会对状态文件做跨机房双写。
7. 启用 `enable_replica_validation=true` 后，控制面会按周期校验主副本一致性。
8. `replica_allowed_lag_entries` 仅对回放文件生效，可容忍少量条目滞后。
9. 快照副本校验不允许哈希不一致，不一致会中止调度。
10. `enable_replica_auto_failover=true` 时，校验失败会触发自动切主并二次校验。
11. `replica_failover_cooldown_seconds` 控制连续切主频率，避免抖动。
12. `replica_health_file` 会持续输出 `green/yellow/red` 健康等级与错误详情。
13. `enable_replica_switchback=true` 时，控制面在故障后进入 failover 模式并累计稳定周期。
14. 达到 `replica_switchback_stable_cycles` 后，会执行回切同步并退出 failover 模式。
15. `replica_drill.enabled=true` 时，控制面会输出演练日志（`replica_drill_ok/error`），用于演练模板化。
16. `slo.enabled=true` 时，控制面会按滚动窗口计算 `score/green_rate/red_count`。
17. `slo.block_on_violation=true` 时，触发 SLO 违规会直接阻断后续调度。
18. `slo.circuit_breaker.enabled=true` 时会启用分级熔断。
19. 黄灯时按 `yellow_max_concurrent_plans/yellow_dispatch_pause_seconds` 自动限流。
20. 红灯时若 `red_block=true`，停止新计划派发并记录 `replica_circuit_blocked`。
21. 配置 `matrix[]` 后，控制面按 `score` 命中矩阵规则，优先于固定黄/红参数。
22. `slo.adaptive.enabled=true` 时，控制面会把当前 score 映射为 `effective_score=score+bias`。
23. `score >= good_score` 时，bias 按 `step` 向正方向偏移（最多 `+max_shift`），减少误触发熔断。
24. `score <= bad_score` 时，bias 按 `step` 向负方向偏移（最多 `-max_shift`），更快收紧熔断。
25. 每轮会落审计 `replica_adaptive_update` 与 `replica_circuit_state`（含 effective_score/bias）。
26. `failover_policy.enabled=true` 时，自动切主会先匹配 `failover_policy.matrix`，再决定放行或阻断。
27. 命中阻断规则会落审计 `replica_failover_policy_blocked`，不会执行切主同步。
28. `replica_drill.score.enabled=true` 时，演练会生成 `score/grade/pass/pass_rate` 并持久化。
29. 演练评分低于 `pass_score` 时落 `replica_drill_warn`，用于提前暴露容灾退化。
30. `failover_policy.slo_link.enabled=true` 时，切主前会校验 `effective_score` 与 `violation`。
31. `failover_policy.drill_link.enabled=true` 时，切主前会校验演练 `pass_rate/average_score/last_pass`。
32. 联动门槛不满足会落审计 `replica_failover_policy_blocked`，并附带 `slo_gate/drill_gate`。
33. 切主策略矩阵覆盖优先级：`site_matrix_overrides` > `region_matrix_overrides` > `matrix`。
34. 切主联动门槛覆盖优先级：`site_overrides` > `region_overrides` > 全局配置。
35. 站点优先级覆盖优先级：`site_priorities.site_overrides` > `site_priorities.region_overrides` > `site_priorities.global_default`。
36. 风险动作 `min_site_priority` 生效规则：同等级同来源下，优先命中“门槛 <= 当前站点优先级”且门槛最高的规则；若无可命中规则则按保守阻断处理。
37. 控制面会输出 `site_risk_throttle_policy` 审计事件，并在同一来源策略未变化时抑制重复审计。
38. 控制面会输出 `rollout_decision_summary` 审计事件，汇总 risk + policy + effective 并发/停顿 + 是否阻断（同来源去重）。
39. `rollout_decision_summary` 的正常主路径告警路由由 Rust `decision-route` 决定；脚本兜底已削为保守默认：`dispatch_blocked=false -> info`，`dispatch_blocked=true -> high`。
40. 脚本兜底的告警通道/目标/投递类型已削为保守默认：未阻断固定 `ops-observe -> ops-observe -> im`，阻断固定 `ops-oncall -> ops-oncall -> webhook`；若配置 `DecisionDeliveryEndpointMap`，仅用它把 `ops-observe/ops-oncall` 解析到实际端点。
41. `rollout_decision_summary` 的告警目标映射：优先取 `risk_policy.alert_channel_targets[channel]`，未配置时回退为通道名本身。
42. `rollout_decision_summary` 的投递类型映射：正常主路径优先取统一 Rust `decision-route` 结果；仅在脚本兜底时保留最小口径，且本地投递只支持显式 endpoint 的 webhook/im。
43. 控制面会输出 `rollout_decision_delivery` 审计事件：记录真实投递执行结果（`delivery_status/delivery_ok/error`）。
44. 启用 `risk_policy.hot_reload` 后，控制面会在 queue 文件变化时自动刷新策略模板，并输出 `risk_policy_hot_reload/risk_policy_hot_reload_error` 审计事件。
45. 同一热重载链路会同步刷新 `state_recovery.failover_converge`，并叠加 `risk_policy.policy_profiles.*.failover_converge` 覆盖；参数变化时输出 `replica_failover_converge_hot_reload` 审计事件。
46. `rollout_decision_summary` 会输出 `risk_policy_active_profile/failover_converge_scope/failover_converge_enabled/failover_converge_max_concurrent/failover_converge_min_dispatch_pause_seconds` 等字段，直接反映当前生效口径。
47. `rollout_decision_delivery`（webhook/im/email 审计）会同步输出上述 `risk_policy_active_profile + failover_converge_*` 字段，告警落地与摘要口径一致。
48. 若配置 `risk_policy.decision_delivery.binary_file` 且二进制存在，控制面会优先调用 `novovm-rollout-decision-delivery`；调用失败时自动回退极简 PowerShell 投递逻辑（仅 webhook/im + 显式 http(s) endpoint，不执行本地 SMTP 邮件发送）。
49. 若配置 `risk_policy.policy_cli.binary_file` 且二进制存在，控制面会优先把未单独覆盖的策略动作统一分发到 `novovm-rollout-policy <tool>`，热更新同口径生效。
50. 若显式配置 `risk_policy.decision_route.binary_file` 且二进制存在，控制面会优先调用 `novovm-rollout-decision-route`；否则走统一入口分发；失败自动回退脚本逻辑。
51. 若显式配置 `risk_policy.risk_action_eval.binary_file` 且二进制存在，控制面会优先调用 `novovm-risk-action-eval`；否则走统一入口分发；失败自动回退脚本逻辑。
52. 若显式配置 `risk_policy.risk_action_matrix_build.binary_file` 且二进制存在，控制面会优先调用 `novovm-risk-action-matrix-build`；否则走统一入口分发；失败自动回退脚本逻辑。
53. 若显式配置 `risk_policy.failover_policy_matrix_build.binary_file` 且二进制存在，控制面会优先调用 `novovm-failover-policy-matrix-build`；否则走统一入口分发；失败自动回退脚本逻辑。
54. 若显式配置 `risk_policy.risk_matrix_select.binary_file` 且二进制存在，控制面会优先调用 `novovm-risk-matrix-select`；否则走统一入口分发；失败自动回退脚本逻辑。
55. 若显式配置 `risk_policy.risk_blocked_select.binary_file` 且二进制存在，控制面会优先调用 `novovm-risk-blocked-select`；否则走统一入口分发；失败自动回退脚本逻辑。
56. 若显式配置 `risk_policy.risk_blocked_map_build.binary_file` 且二进制存在，控制面会优先调用 `novovm-risk-blocked-map-build`；否则走统一入口分发；失败自动回退脚本逻辑。
57. 若显式配置 `risk_policy.risk_level_set.binary_file` 且二进制存在，控制面会优先调用 `novovm-risk-level-set`；否则走统一入口分发；失败自动回退脚本逻辑。
58. 若显式配置 `risk_policy.profile_select.binary_file` 且二进制存在，控制面会优先调用 `novovm-risk-policy-profile-select`；否则走统一入口分发；失败自动回退脚本逻辑。
59. `overlay relay-discovery-merge`、`overlay auto-profile`、`overlay relay-health-refresh` 已完成统一实现内收：树形子命令直接执行共享 Rust 模块，旧 `novovm-overlay-*` 二进制仅作为兼容薄包装保留。

## 13. SLO 多级熔断矩阵示例

```json
{
  "state_recovery": {
    "slo": {
      "circuit_breaker": {
        "enabled": true,
        "matrix": [
          { "name": "green",  "min_score": 95, "max_score": 101, "max_concurrent_plans": 2, "dispatch_pause_seconds": 1, "block_dispatch": false },
          { "name": "yellow", "min_score": 80, "max_score": 95,  "max_concurrent_plans": 1, "dispatch_pause_seconds": 3, "block_dispatch": false },
          { "name": "orange", "min_score": 60, "max_score": 80,  "max_concurrent_plans": 1, "dispatch_pause_seconds": 6, "block_dispatch": false },
          { "name": "red",    "min_score": 0,  "max_score": 60,  "max_concurrent_plans": 1, "dispatch_pause_seconds": 8, "block_dispatch": true }
        ]
      }
    }
  }
}
```

## 14. SLO 自适应阈值示例

```json
{
  "state_recovery": {
    "slo": {
      "adaptive": {
        "enabled": true,
        "file": "artifacts/runtime/rollout/control-plane-replica-adaptive.json",
        "step": 2,
        "good_score": 95,
        "bad_score": 70,
        "max_shift": 20
      }
    }
  }
}
```

## 15. 自动切主策略矩阵示例

```json
{
  "state_recovery": {
    "failover_policy": {
      "enabled": true,
      "default_allow": false,
      "region_matrix_overrides": {
        "CN": [
          { "name": "cn-cycle-red-allow", "source": "cycle", "grades": ["red"], "min_site_priority": 120, "allow_auto_failover": true, "cooldown_seconds": 35 }
        ]
      },
      "site_matrix_overrides": {
        "CN-SH-1": [
          { "name": "cn-sh-startup-red-block", "source": "startup", "grades": ["red"], "allow_auto_failover": false }
        ]
      },
      "matrix": [
        { "name": "startup-red-primary", "source": "startup", "grades": ["red"], "min_site_priority": 150, "allow_auto_failover": true, "cooldown_seconds": 20 },
        { "name": "cycle-red-primary", "source": "cycle", "grades": ["red"], "min_site_priority": 150, "allow_auto_failover": true, "cooldown_seconds": 30 },
        { "name": "fallback-block", "source": "*", "grades": ["*"], "allow_auto_failover": false }
      ]
    }
  }
}
```

## 16. 容灾演练自动评分示例

```json
{
  "state_recovery": {
    "replica_drill": {
      "enabled": true,
      "drill_id": "drill-2026-04-template",
      "score": {
        "enabled": true,
        "file": "artifacts/runtime/rollout/control-plane-replica-drill-score.json",
        "window_samples": 20,
        "pass_score": 70
      }
    }
  }
}
```

## 17. 切主策略与 SLO/演练评分联动示例

```json
{
  "state_recovery": {
    "failover_policy": {
      "enabled": true,
      "default_allow": false,
      "slo_link": {
        "enabled": true,
        "min_effective_score": 65,
        "block_on_violation": true,
        "region_overrides": {
          "CN": { "enabled": true, "min_effective_score": 70, "block_on_violation": true }
        },
        "site_overrides": {
          "CN-SH-1": { "enabled": true, "min_effective_score": 75, "block_on_violation": true }
        }
      },
      "drill_link": {
        "enabled": true,
        "min_pass_rate": 0.6,
        "min_average_score": 70,
        "require_last_pass": false,
        "region_overrides": {
          "CN": { "enabled": true, "min_pass_rate": 0.7, "min_average_score": 75, "require_last_pass": true }
        },
        "site_overrides": {
          "CN-SH-1": { "enabled": true, "min_pass_rate": 0.8, "min_average_score": 80, "require_last_pass": true }
        }
      },
      "risk_link": {
        "enabled": true,
        "region_overrides": {
          "CN": { "enabled": true }
        },
        "site_overrides": {
          "CN-SH-1": { "enabled": true }
        }
      }
    }
  }
}
```

## 18. 跨站点冲突自动判责与降权矩阵示例

```json
{
  "site_consensus": {
    "enabled": true,
    "site_id": "CN-SH-1",
    "accountability": {
      "enabled": true,
      "state_file": "artifacts/runtime/rollout/site-consensus-accountability.json",
      "max_penalty_points": 200,
      "recovery_per_win": 1,
      "matrix": [
        { "name": "conflict-loser-demote", "event": "consensus_conflict_loser", "role": "loser", "site": "*", "penalty_points": 5 },
        { "name": "committed-other-self-demote", "event": "consensus_committed_other", "role": "self", "site": "*", "penalty_points": 3 },
        { "name": "winner-recover", "event": "consensus_winner", "role": "winner", "site": "*", "penalty_points": -1 }
      ]
    },
    "site_priorities": {
      "global_default": 80,
      "region_overrides": {
        "CN": 120
      },
      "site_overrides": {
        "CN-SH-1": 200
      }
    }
  }
}
```

## 19. 跨站点信誉分长期治理示例

```json
{
  "site_consensus": {
    "enabled": true,
    "accountability": {
      "enabled": true,
      "reputation": {
        "enabled": true,
        "aging_interval_seconds": 3600,
        "recover_points_per_interval": 1,
        "recover_idle_seconds": 1800
      }
    }
  }
}
```

## 20. 跨站点风险预测与自动限流联动示例

```json
{
  "site_consensus": {
    "enabled": true,
    "accountability": {
      "enabled": true,
      "risk": {
        "enabled": true,
        "state_file": "artifacts/runtime/rollout/site-consensus-risk.json",
        "ema_alpha": 0.2,
        "auto_throttle": {
          "enabled": true,
          "yellow_max_concurrent_plans": 1,
          "yellow_dispatch_pause_seconds": 3,
          "orange_max_concurrent_plans": 1,
          "orange_dispatch_pause_seconds": 6,
          "red_block": true
        }
      }
    }
  }
}
```

## 21. 高风险站点赢家保护与切主风险联动示例

```json
{
  "site_consensus": {
    "enabled": true,
    "accountability": {
      "enabled": true,
      "risk": {
        "enabled": true,
        "winner_guard": {
          "enabled": true,
          "blocked_levels": ["red"],
          "fallback_allow_when_all_blocked": true
        }
      }
    }
  },
  "state_recovery": {
    "failover_policy": {
      "risk_link": {
        "enabled": true,
        "blocked_levels": ["red"]
      }
    }
  }
}
```

## 22. 统一风险阻断矩阵基线示例（推荐）

```json
{
  "risk_policy": {
    "active_profile": "production",
    "hot_reload": {
      "enabled": true,
      "check_seconds": 2
    },
    "policy_profiles": {
      "production": {
        "blocked_levels": ["red"],
        "winner_guard_blocked_levels": ["red"],
        "failover_risk_link_blocked_levels": ["red"],
        "failover_converge": {
          "enabled": true,
          "max_concurrent_plans": 1,
          "min_dispatch_pause_seconds": 30,
          "block_on_snapshot_red": true,
          "block_on_replay_red": true
        }
      },
      "release_guard": {
        "blocked_levels": ["orange", "red"],
        "winner_guard_blocked_levels": ["yellow", "orange", "red"],
        "failover_risk_link_blocked_levels": ["orange", "red"],
        "failover_converge": {
          "enabled": true,
          "max_concurrent_plans": 1,
          "min_dispatch_pause_seconds": 45,
          "block_on_snapshot_red": true,
          "block_on_replay_red": true
        },
        "action_matrix": [
          { "source": "*", "level": "yellow", "cap_concurrent": 1, "pause_seconds": 4, "block_dispatch": false },
          { "source": "*", "level": "orange", "cap_concurrent": 1, "pause_seconds": 8, "block_dispatch": true },
          { "source": "*", "level": "red", "cap_concurrent": 1, "pause_seconds": 10, "block_dispatch": true }
        ]
      }
    },
    "blocked_levels": ["red"],
    "winner_guard_blocked_levels": ["orange", "red"],
    "failover_risk_link_blocked_levels": ["red"],
    "site_region_map": {
      "CN-SH-1": "CN",
      "EU-FRA-1": "EU"
    },
    "region_action_matrix_overrides": {
      "CN": [
      { "source": "cycle", "level": "yellow", "cap_concurrent": 1, "pause_seconds": 4, "block_dispatch": false }
      ]
    },
    "site_action_matrix_overrides": {
      "CN-SH-1": [
      { "source": "cycle", "level": "red", "min_site_priority": 120, "cap_concurrent": 1, "pause_seconds": 8, "block_dispatch": true }
      ]
    },
    "region_winner_guard_blocked_levels": {
      "CN": ["orange", "red"]
    },
    "site_winner_guard_blocked_levels": {
      "CN-SH-1": ["yellow", "orange", "red"]
    },
    "region_failover_risk_link_blocked_levels": {
      "CN": ["red"]
    },
    "site_failover_risk_link_blocked_levels": {
      "CN-SH-1": ["orange", "red"]
    },
    "action_matrix": [
      { "source": "*", "level": "yellow", "cap_concurrent": 1, "pause_seconds": 3, "block_dispatch": false },
      { "source": "*", "level": "orange", "cap_concurrent": 1, "pause_seconds": 6, "block_dispatch": false },
      { "source": "startup", "level": "red", "min_site_priority": 150, "cap_concurrent": 1, "pause_seconds": 4, "block_dispatch": false },
      { "source": "cycle", "level": "red", "min_site_priority": 120, "cap_concurrent": 1, "pause_seconds": 6, "block_dispatch": true }
    ],
    "alert_channel_targets": {
      "l1-pager": "oncall:l1:finality",
      "l2-oncall": "oncall:l2:execution",
      "l3-oncall": "oncall:l3:edge",
      "ops-oncall": "oncall:ops:default",
      "l1-observe": "observe:l1:finality",
      "l2-observe": "observe:l2:execution",
      "l3-observe": "observe:l3:edge",
      "ops-observe": "observe:ops:default"
    },
    "alert_target_delivery_types": {
      "oncall:l1:finality": "webhook",
      "oncall:l2:execution": "webhook",
      "oncall:l3:edge": "webhook",
      "oncall:ops:default": "webhook",
      "observe:l1:finality": "im",
      "observe:l2:execution": "im",
      "observe:l3:edge": "im",
      "observe:ops:default": "im"
    },
    "delivery_webhook_endpoints": {
      "oncall:l1:finality": "https://ops.example.com/novovm/l1/pager",
      "oncall:l2:execution": "https://ops.example.com/novovm/l2/oncall",
      "oncall:l3:edge": "https://ops.example.com/novovm/l3/oncall",
      "oncall:ops:default": "https://ops.example.com/novovm/ops/oncall"
    },
    "delivery_im_endpoints": {
      "observe:l1:finality": "https://im.example.com/bot/l1-observe",
      "observe:l2:execution": "https://im.example.com/bot/l2-observe",
      "observe:l3:edge": "https://im.example.com/bot/l3-observe",
      "observe:ops:default": "https://im.example.com/bot/ops-observe"
    },
    "delivery_email_targets": {
      "oncall:ops:default": "ops-alert@example.com"
    },
    "delivery_email": {
      "smtp_server": "smtp.example.com",
      "smtp_port": 587,
      "from": "novovm-alert@example.com",
      "use_ssl": true,
      "smtp_user": "novovm-alert@example.com",
      "smtp_password_env": "NOVOVM_SMTP_PASSWORD"
    },
    "decision_delivery": {
      "binary_file": "target/release/novovm-rollout-decision-delivery.exe"
    },
    "decision_route": {
      "binary_file": "target/release/novovm-rollout-decision-route.exe"
    },
    "profile_select": {
      "binary_file": "target/release/novovm-risk-policy-profile-select.exe"
    },
    "risk_action_eval": {
      "binary_file": "target/release/novovm-risk-action-eval.exe"
    },
    "risk_action_matrix_build": {
      "binary_file": "target/release/novovm-risk-action-matrix-build.exe"
    },
    "failover_policy_matrix_build": {
      "binary_file": "target/release/novovm-failover-policy-matrix-build.exe"
    },
    "risk_matrix_select": {
      "binary_file": "target/release/novovm-risk-matrix-select.exe"
    },
    "risk_blocked_select": {
      "binary_file": "target/release/novovm-risk-blocked-select.exe"
    },
    "risk_blocked_map_build": {
      "binary_file": "target/release/novovm-risk-blocked-map-build.exe"
    },
    "risk_level_set": {
      "binary_file": "target/release/novovm-risk-level-set.exe"
    }
  },
  "site_consensus": {
    "enabled": true,
    "accountability": {
      "enabled": true,
      "risk": {
        "enabled": true,
        "winner_guard": {
          "enabled": true,
          "fallback_allow_when_all_blocked": true
        }
      }
    }
  },
  "state_recovery": {
    "failover_policy": {
      "risk_link": {
        "enabled": true
      }
    }
  }
}
```

## 11. 异地恢复演练模板（drill）

1. 在 `state_recovery.replica_drill` 设置 `enabled=true` 并填写 `drill_id`。
2. 启动控制面后检查审计日志中的 `replica_drill_ok/error` 事件。
3. 演练阶段不会触发状态改写，只验证候选切主源可用性。
4. 演练通过后再开启真实 `enable_replica_auto_failover` 执行切主。

## 12. 副本健康 SLO 策略

1. SLO 评分窗口由 `slo.window_samples` 控制。
2. 评分规则：`green=100`、`yellow=60`、`red=0`，滚动平均生成 `score`。
3. 违规判定：
4. `green_rate < slo.min_green_rate`
5. `red_count > slo.max_red_in_window`
6. 当 `slo.block_on_violation=true`，违规会产生日志 `replica_slo_violation` 并阻断调度。

## 23. 灰度决策审计导出（dashboard jsonl）

用途：把控制面审计文件中的 `rollout_decision_summary/rollout_decision_delivery` 归一化导出成稳定字段，供 dashboard / 告警平台直接消费。

主程序：`novovm-rollout-decision-dashboard-export`（Rust）  
兜底脚本：`scripts/novovm-rollout-decision-dashboard-export.ps1`

执行模式：

1. 手工模式：直接执行导出程序（Rust，见下方命令）
2. 内置模式：在 queue 中配置 `risk_policy.decision_dashboard_export`，由控制面主循环按 `check_seconds` 周期触发
3. 内置模式支持热重载：queue 变更后自动生效并落审计 `decision_dashboard_export_hot_reload`

默认输入输出：

1. 输入：`artifacts/runtime/rollout/control-plane-audit.jsonl`
2. 输出：`artifacts/runtime/rollout/control-plane-decision-dashboard.jsonl`

核心参数：

1. `-Mode delivery|summary|both`：选择导出事件类型，默认 `both`
2. `-Tail <N>`：仅导出最近 N 条审计输入
3. `-SinceUtc <UTC时间>`：按 UTC 时间下限过滤
4. `-AuditFile <path>`：自定义输入审计文件
5. `-OutputFile <path>`：自定义输出文件

示例：

```powershell
.\target\release\novovm-rollout-decision-dashboard-export.exe `
  --mode both `
  --tail 2000 `
  --output-file .\artifacts\runtime\rollout\control-plane-decision-dashboard.jsonl
```

仅导出投递事件：

```powershell
.\target\release\novovm-rollout-decision-dashboard-export.exe `
  --mode delivery `
  --since-utc "2026-04-05T00:00:00Z"
```

导出字段包含：

1. 决策链路字段：`decision_alert_level/channel/target`、`decision_delivery_type/action`、`delivery_status/delivery_ok`
2. 风险与节流字段：`dispatch_blocked`、`effective_max_concurrent`、`effective_pause_seconds`、`worst_site_id/level/score`
3. 收敛透出字段：`risk_policy_active_profile`、`failover_converge_scope`、`failover_converge_*`

## 24. 灰度决策看板消费端（state + alerts）

用途：消费 `control-plane-decision-dashboard.jsonl`，输出看板状态快照与阻断告警文件，供 UI 或告警系统直接读取。

主程序：`novovm-rollout-decision-dashboard-consumer`（Rust）  
兜底脚本：`scripts/novovm-rollout-decision-dashboard-consumer.ps1`

默认输入输出：

1. 输入：`artifacts/runtime/rollout/control-plane-decision-dashboard.jsonl`
2. 状态输出：`artifacts/runtime/rollout/control-plane-decision-dashboard-state.json`
3. 告警输出：`artifacts/runtime/rollout/control-plane-decision-dashboard-alerts.jsonl`

核心参数：

1. `-Mode all|blocked`：`all` 生成全量状态，`blocked` 仅消费阻断事件
2. `-Tail <N>`：只消费输入尾部 N 条
3. `-InputFile/-OutputFile/-AlertsFile`：自定义输入/输出路径

手工执行示例：

```powershell
.\target\release\novovm-rollout-decision-dashboard-consumer.exe `
  --mode all `
  --tail 2000
```

控制面内置执行：

1. 在 `risk_policy.decision_dashboard_consumer` 里设置 `enabled=true`
2. 控制面会按 `check_seconds` 周期调用并落审计 `decision_dashboard_consumer`
3. 配置变更热重载后会落审计 `decision_dashboard_consumer_hot_reload`

## 25. Failover 策略统一入口（Rust）

用途：把 seed / region 故障切换规则收进统一 Rust 控制面内核，不再让 cooldown、降级、恢复规则散落在独立脚本或重复实现里。

主程序：`novovm-rollout-policy failover seed-evaluate`、`novovm-rollout-policy failover region-evaluate`

当前口径：

1. `seed-evaluate` 负责单 seed 的 success-rate / consecutive-failures / cooldown 判定与状态文件写回。
2. `region-evaluate` 负责单 region 的 score-threshold / cooldown 判定与状态文件写回。
3. `overlay relay-discovery-merge` 已复用同一套共享 failover 规则，不再内嵌第二份 seed / region 降级逻辑。

## 26. Risk 策略统一入口（Rust）

用途：把 replica SLO 窗口评分和 circuit-breaker 节流规则收进统一 Rust 入口，逐步替代控制面脚本中的固定策略分支。

主程序：

1. `novovm-rollout-policy risk slo-evaluate`
2. `novovm-rollout-policy risk circuit-breaker-evaluate`

当前口径：

1. `slo-evaluate` 负责按 `green/yellow/red` 样本窗口计算 `green_rate / score / violation / reason`，并可直接落状态文件。
2. `circuit-breaker-evaluate` 负责按 score 与 matrix 计算 `max_concurrent_plans / dispatch_pause_seconds / block_dispatch`。
3. `scripts/novovm-node-rollout-control.ps1` 的 `Apply-ReplicaSloPolicy` 已优先调用这两个统一 risk 子命令；仅当统一 CLI 不可用或调用失败时才回退本地 PowerShell 规则。
4. `risk action-eval` 与 `risk level-set` 也已进入共享 Rust 模块，旧 `novovm-risk-action-eval`、`novovm-risk-level-set` 只保留兼容薄壳。
5. `risk action-matrix-build / matrix-select / blocked-select / blocked-map-build / policy-profile-select` 也已进入共享 Rust 模块，旧独立二进制只保留兼容薄壳。
6. 现阶段统一入口已经开始接管 risk 域的固定策略逻辑。
7. `Resolve-RiskActionMatrix` 的 PowerShell 兜底已降为 emergency fallback：Rust 不可用时只保留保守全局矩阵，不再在脚本内重建完整 `source/min_site_priority` 覆盖关系。
8. `Resolve-RiskPolicyProfileSelection` 已优先走统一 `novovm-rollout-policy risk policy-profile-select` 入口，不再绕过统一 CLI 直接调用旧独立二进制；脚本仅保留 `active_profile/policy_profiles` 查表兜底。
9. `Apply-ReplicaSloPolicy` 的本地兜底也已削为 emergency fallback：Rust 不可用时仅按当前 `grade` 生成保守 `score/violation` 与 `circuit` 动作，不再在 PowerShell 内执行完整窗口评分、矩阵解析与 score 命中。
10. `Select-RiskActionMatrix / Resolve-RiskBlockedSetMap / Select-RiskBlockedSet` 的 PowerShell 兜底也已削为保守默认：Rust 不可用时不再执行 site/region 分层矩阵选择、blocked map 构建或 blocked set 选择，只回全局 baseline/default set。


## 2026-04-06 compatibility-shell update

- `scripts/novovm-rollout-decision-dashboard-export.ps1` and `scripts/novovm-rollout-decision-dashboard-consumer.ps1` are no longer independent PowerShell implementations; they now forward directly to `novovm-rollout-policy rollout ...`.
- `scripts/novovm-overlay-relay-health-refresh.ps1` and `scripts/novovm-overlay-relay-discovery-merge.ps1` are no longer independent PowerShell implementations; they now forward directly to `novovm-rollout-policy overlay ...`.
- The root-script inventory and cutlist is tracked in `docs_CN/NOVOVM-PS1-INVENTORY-AND-MIGRATION-CUTLIST-2026-04-06.md`.

## 2026-04-06 batch cleanup governance

- Mainline PowerShell cleanup is now executed by batch class, not by single-file drift.
- `scripts/migration/*` is treated as a frozen history asset pool and is outside the normal rollout-policy cleanup path.
- Current legacy bin compatibility status is tracked in `docs_CN/NOVOVM-LEGACY-BIN-RETIREMENT-AUDIT-2026-04-06.md`.

## 2026-04-07 unified CLI default path update

- `novovmctl rollout-control` is now the mainline control-plane entry; the legacy compat shell `novovm-node-rollout-control.ps1` only forwards into Rust and no longer carries mainline entry authority.
- `novovmctl up` now owns the mainline auto-profile entry path and resolves the unified `novovm-rollout-policy` CLI by default.

## 2026-04-07 wrapper-bin retirement

- Dedicated per-tool wrapper executables under `crates/novovm-rollout-policy/src/bin/*` have been retired in Batch 1.
- Compatibility for legacy tool names is now provided only by the unified `novovm-rollout-policy` entrypoint.

## 2026-04-07 implicit legacy-default removal

- Default helper resolution no longer silently searches deleted per-tool wrapper executables.
- `novovmctl up` no longer treats the legacy dedicated `overlay-auto-profile` executable as a default path.

## 2026-04-07 sealed operating model

The current rollout/control-plane policy runtime should be understood as a sealed three-layer model:

- normal main path: unified `novovm-rollout-policy`
- compatibility path: explicit shell or explicit override only
- emergency path: PowerShell startup and minimal conservative fallback only

Operators should not assume any deleted per-tool wrapper executable still exists as a default runtime path.
