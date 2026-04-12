# NODE-ENTRY-INVENTORY (2026-04-11)

## 目标

清点所有 `novovm-node` 启动路径、手工路由 env 注入路径与旁路 worldline 风险点，建立单一生产入口治理清单。

## A. 生产入口（受控）

1. `crates/novovmctl/src/integration/node_binary.rs`
- 作用：`up/daemon` 启动主链路
- 状态：已注入 scheduler hard-lock env（source/token/strict/manual-route-lock）

2. `crates/novovmctl/src/commands/lifecycle.rs` (`spawn_effective_node`)
- 作用：lifecycle 托管启动链路
- 状态：已注入 scheduler hard-lock env（source/token/strict/manual-route-lock）

## B. 直接启动/旁路路径（需管控）

1. `crates/novovm-node/tests/queue_replay_smoke.rs`
- 类型：测试专用直启
- 结论：保留（非生产）

2. `scripts/migration/run_gateway_node_pipeline.ps1` 等 migration 脚本
- 类型：迁移/验证脚本中直启 node
- 结论：保留为非生产工具，后续统一套 `novovmctl` 包装

3. `scripts/novovm-up.ps1` 与 rollout/lifecycle 兼容壳
- 类型：脚本入口
- 结论：应保持 `novovmctl` 透传，不形成第二调度主线

## C. 手工 route env 注入热点

1. `scripts/novovm-up.ps1`
- 大量设置 `NOVOVM_OVERLAY_ROUTE_*`
- 风险：脚本层手工覆写路由语义

2. `crates/gateways/evm-gateway/src/main.rs`
- 读取 `NOVOVM_OVERLAY_ROUTE_*` 形成网关侧路由参数
- 风险：跨模块路由口径漂移

3. `crates/plugins/evm/plugin/src/lib.rs`
- 读取 `NOVOVM_OVERLAY_ROUTE_*`
- 风险：插件侧形成局部路由口径

4. `crates/novovm-node/src/bin/novovm-node.rs`
- 读取 L3 policy/profile/family/version env
- 现状：已接入 `NOVOVM_SUPERVM_MANUAL_ROUTE_ENV_LOCK` 黑名单闸门

## D. 旁路 worldline 入口（ingress）

1. `NOVOVM_TX_WIRE_FILE`
2. `NOVOVM_OPS_WIRE_FILE`
3. `NOVOVM_OPS_WIRE_DIR`
4. `NOVOVM_D1_INGRESS_MODE`

说明：这组入口是数据输入模式，不是调度入口。必须继续由 `novovmctl` 管理注入，禁止脚本层自行扩散为并行主线。

## E. 本轮整改结论

1. 生产入口硬锁已落地
- `novovmctl` 两条实际启动链均已注入 scheduler 上下文与 strict 锁

2. node 运行时闸门已落地
- scheduler gate（source/token）
- manual route env lock（L3 核心 env 黑名单）
- single-source strict gate 继续生效

3. 后续收敛项（下一轮）
- 把 migration 中仍直启 `novovm-node` 的脚本，分批改为 `novovmctl` 驱动
- 对 gateway/plugin 的 route env 读取增加“仅调度注入来源可用”的只读治理约束

## F. A/B/C 风险分级（当前）

- A（立即整改，生产有歧义）：0（已清零）
- B（保留外壳，内部透传 novovmctl）：6
  - `scripts/novovm-up.ps1`
  - `scripts/migration/run_gateway_node_pipeline.ps1`
  - `scripts/migration/run_prod_node_e2e_tps.ps1`
  - `scripts/novovm-node-rollout.ps1`
  - `scripts/novovm-node-lifecycle.ps1`
  - `scripts/novovm-node-rollout-control.ps1`
- C（非生产/归档）：其余 migration 和历史 gate 脚本（仅测试用途）

## G. 本轮消减结果

- 已消减旁路入口：5（`novovmctl` 两条生产启动链硬锁 + 3 个 A 类脚本透传化）
- 已消减手工 route env 污染面：2 组（`env_remove + node gate blacklist + A 类脚本禁注入`)
- 当前残留高风险入口：0（A 类归零）


## I. C 类归档 / 禁用状态（当前）

1. `scripts/novovm-prod-daemon.ps1`
- 状态：DISABLED（decommissioned / non-prod）
- 原因：历史兼容入口，存在与统一模板壳重复语义风险
- 现状：强制失败并指向 `novovmctl daemon` / `scripts/novovm-up.ps1`

2. `scripts/migration/run_*_gate.ps1`（已标 DISABLED 的历史 gate）
- 状态：ARCHIVED-STYLE（保留只读历史用途，不参与生产入口）
- 规则：禁止进入生产启动链，不得作为节点启动入口

## J. Scheduler Hard-Lock 专项矩阵（当前）

- 规则：`invalid source -> fail`、`missing token -> fail`、`manual env override -> fail`、`novovmctl controlled launch -> pass`
- 固化载体：
  - `scheduler_gate_matrix_*`
  - `manual_route_env_lock_matrix_*`
  - `scheduler_hard_lock_matrix_contract_is_frozen`
- 规范文档：`docs_CN/NOVOVM-NETWORK/NOVOVM-SCHEDULER-HARD-LOCK-REGRESSION-MATRIX-2026-04-11.md`

## K. M5 固化：L2->L1 语义锁（冻结契约）

- 导出基线：L2->L1 导出必须绑定同一 `L2L1ExportBaselineView`
- 语义约束：`batch / replay / watch` 三路径必须语义等价
- Anchor 约束：`fingerprint` 稳定性属于主线冻结契约
- 固定 Gate：
  - `l2_l1_export_equivalence_batch_vs_replay`
  - `l2_l1_export_equivalence_batch_vs_watch`
  - `l2_l1_anchor_fingerprint_stable`
  - `cross_node_runtime_membership_closed_loop`
  - `v2_matrix_a_order_perturbation_consistency`
  - `v2_matrix_b_multi_source_conflict_consistency`
  - `v2_matrix_c_weak_network_disturbance_consistency`
  - `v2_matrix_d_multi_region_view_consistency`
- 合同位置：
  - `crates/novovm-node/src/bin/novovm-node.rs`（上述 3 条测试）
  - `docs_CN/NOVOVM-NETWORK/NOVOVM-SCHEDULER-HARD-LOCK-REGRESSION-MATRIX-2026-04-11.md`

## L. M6 固化：固定回归门执行入口统一

- Production Gate Entry（唯一执行入口）：
  - `cargo run -p novovm-node --bin supervm-mainline-gate`
- 规则：
  - 不再用脚本或手工命令拼装局部 gate 组合进行生产签收
  - 所有主线 gate（scheduler/manual env lock/L2->L1 语义锁/relay/queue）必须经该入口执行
- Runner 位置：
  - `crates/novovm-node/src/bin/supervm-mainline-gate.rs`

## M. M7 固化：CI / 发布前置门接入

- CI 强制门：
  - `.github/workflows/ci.yml` 中 `SuperVM Mainline Gate Runner (canonical)` 步骤
  - 统一执行：`cargo run -p novovm-node --bin supervm-mainline-gate`
- 发布前置门：
  - 仅承认 Gate Runner 通过 + `artifacts/mainline-status.json`
- 规则：
  - 手工测试组合不作为主线签收口径
  - 未过 Gate Runner 不得进入发布链路

## N. M8 固化：发布/rollout 状态门冻结

- 执行位置：`scripts/_compat/Invoke-NovovmctlForward.ps1`
- 适用子命令：`up / daemon / rollout / rollout-control / lifecycle`
- 强制条件：
  - `artifacts/mainline-status.json` 必须存在
  - `schema = supervm-mainline-status/v2`
  - 主线 gate（固定 lockset）全部为 `true`
- 结果：
  - 条件不满足时拒绝透传到 `novovmctl`
  - 发布/rollout 不再接受手工组合测试作为签收依据

## O. M9 固化：状态时效闸门

- 执行位置：`scripts/_compat/Invoke-NovovmctlForward.ps1`
- 时效字段：`mainline-status.json.generated_utc`
- 默认窗口：`3600s`
- 配置项：`NOVOVM_SUPERVM_MAINLINE_STATUS_MAX_AGE_SECONDS`（`>=0` 整数）
- 拒绝逻辑：
  - `generated_utc` 缺失/非法 -> fail
  - 状态年龄超过窗口 -> fail
  - 必须先刷新 `cargo run -p novovm-node --bin supervm-mainline-gate`

## P. M10 固化：状态时效闸门冻结契约接入

- 契约测试：`mainline_status_freshness_gate_contract_is_frozen`
- 位置：`crates/novovm-node/src/bin/novovm-node.rs`
- 固定 Gate：已并入 `supervm-mainline-gate` 主线必跑链
- 冻结项：
  - `generated_utc` 时效字段语义
  - 默认窗口 `3600s`
  - 可配置项 `NOVOVM_SUPERVM_MAINLINE_STATUS_MAX_AGE_SECONDS`
  - 过期拒绝语义 `mainline gate status expired`

## Q. M11 固化：四层网络主线功能装配基线

- 冻结前提：
  - `M1-M10` 已完成并冻结，后续功能包不得重开入口/Gate/发布路线讨论
- 唯一签收口径：
  - `cargo run -p novovm-node --bin supervm-mainline-gate`
  - `artifacts/mainline-status.json`
- 统一交付格式（后续 L1/L2/L3/L4 功能包）：
  - `Milestone`
  - `Gate`
  - `Debt`

## R. M12 固化：统一交付契约产物

- 产物位置：
  - `artifacts/mainline-delivery-contract.json`
- 来源：
  - `crates/novovm-node/src/bin/supervm-mainline-gate.rs`
- 固定字段：
  - `milestone`
  - `gate_entry`
  - `status_source`
  - `overall_gate`
  - `debt`
- 作用：
  - 将后续四层功能包交付统一为 `Milestone / Gate / Debt` 可机读口径

## S. M13 固化：V2 第一阶段矩阵主线签收块

- 主线必跑分组（A/B/C/D）：
  - `v2_matrix_a_order_perturbation_consistency`
  - `v2_matrix_b_multi_source_conflict_consistency`
  - `v2_matrix_c_weak_network_disturbance_consistency`
  - `v2_matrix_d_multi_region_view_consistency`
- 状态源约束：
  - 四组结果必须进入 `artifacts/mainline-status.json` 的 gate 分项
  - 仅 `supervm-mainline-gate` 结果可作为签收依据
- 规则：
  - 不允许将 A/B/C/D 作为可选测试组
  - 任一分组失败即主线签收失败
