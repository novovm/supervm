# NOVOVM Network 生产单一路径硬锁规范（2026-04-11）

## 目标

把 SuperVM 四层网络固定为唯一主线运行口径：

- 所有生产节点由 `novovmctl` 调度启动
- `novovm-node` 仅接受带调度上下文的启动
- 禁止通过手工环境变量绕过 L3/L4/L2/L1 主线策略

## 硬锁规则

1. 启动源硬锁（Scheduler Source Lock）
- `novovmctl` 启动 `novovm-node` 时强制注入：
  - `NOVOVM_SCHED_SOURCE=novovmctl`
  - `NOVOVM_SCHED_TOKEN=<runtime token>`
  - `NOVOVM_SCHED_REQUIRED=1`
  - `NOVOVM_SINGLE_SOURCE_STRICT=1`
  - `NOVOVM_SUPERVM_MANUAL_ROUTE_ENV_LOCK=1`

2. 入口校验硬锁（Node Runtime Gate）
- `novovm-node` 在 strict 模式下必须满足：
  - 调度来源为 `novovmctl`
  - 调度 token 存在
- 不满足即启动失败。

3. 手工路由环境变量硬锁（Manual Override Lock）
- 启用 `NOVOVM_SUPERVM_MANUAL_ROUTE_ENV_LOCK=1` 时，
  `novovm-node` 禁止手工注入 L3 profile/policy/family/version 类路由环境变量。
- 命中黑名单即启动失败。

## 日志口径

运行时新增统一日志：

- `supervm_scheduler_gate: required=... source=... token_present=... context_ok=...`
- `supervm_manual_route_env_lock: enabled=... blocked_count=... blocked_keys=...`
- `supervm_single_source: ...`

用于直接判定节点是否遵循“SuperVM 调度唯一入口”。

## 适用边界

- 本规范只强化“生产单一主线入口”，不改变四层网络语义本身。
- 不引入新分叉，不新增并行 worldline，不改 F.6/F.9 冻结基线。

## 当前阶段结论

该硬锁规范用于把四层网络从“可选路径”收敛为“生产唯一路径”。
后续功能扩展必须在该入口硬锁之上进行。

## M13 固化：V2 第一阶段分布式一致裁决矩阵接入主线签收

- canonical 签收入口保持唯一：
  - `cargo run -p novovm-node --bin supervm-mainline-gate`
- V2 固定签收分组：
  - `v2_matrix_a_order_perturbation_consistency`
  - `v2_matrix_b_multi_source_conflict_consistency`
  - `v2_matrix_c_weak_network_disturbance_consistency`
  - `v2_matrix_d_multi_region_view_consistency`
- 状态源约束：
  - 四组结果必须写入 `artifacts/mainline-status.json` 的 gate 分项
  - 任一分组失败即拒绝主线签收与发布/rollout 透传
