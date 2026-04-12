# NOVOVM Scheduler Hard-Lock Regression Matrix (2026-04-11)

## Scope

This matrix is a production baseline gate for node entry governance.
It must remain enabled and must not be removed from CI/regression flow.

## Required lock behavior

1. invalid source -> fail
2. missing token -> fail
3. manual route env override -> fail
4. controlled launch via novovmctl -> pass

## L2->L1 semantic lock behavior (mainline)

1. batch/replay export equivalence -> pass
2. batch/watch export equivalence -> pass
3. anchor fingerprint stability -> pass

## Concrete test anchors

- `scheduler_gate_matrix_invalid_source_fails`
- `scheduler_gate_matrix_missing_token_fails`
- `scheduler_gate_matrix_novovmctl_with_token_passes`
- `scheduler_hard_lock_matrix_contract_is_frozen`
- `manual_route_env_lock_matrix_detects_overlay_and_profile_keys`
- `manual_route_env_lock_matrix_keyset_contract_is_frozen`
- `l2_l1_export_equivalence_batch_vs_replay`
- `l2_l1_export_equivalence_batch_vs_watch`
- `l2_l1_anchor_fingerprint_stable`
- `mainline_status_freshness_gate_contract_is_frozen`
- `cross_node_runtime_membership_closed_loop`
- `v2_matrix_a_order_perturbation_consistency`
- `v2_matrix_b_multi_source_conflict_consistency`
- `v2_matrix_c_weak_network_disturbance_consistency`
- `v2_matrix_d_multi_region_view_consistency`

## Fixed regression gate (mainline)

Single entry (canonical):

- `cargo run -p novovm-node --bin supervm-mainline-gate`

Locked execution order (inside gate runner):

1. `cargo check -p novovm-network`
2. `cargo check -p novovm-node`
3. `cargo test -p novovm-node scheduler_gate_matrix`
4. `cargo test -p novovm-node manual_route_env_lock_matrix`
5. `cargo test -p novovm-node l2_l1_export_equivalence_batch_vs_replay`
6. `cargo test -p novovm-node l2_l1_export_equivalence_batch_vs_watch`
7. `cargo test -p novovm-node l2_l1_anchor_fingerprint_stable`
8. `cargo test -p novovm-node mainline_status_freshness_gate_contract_is_frozen`
9. `cargo test -p novovm-node cross_node_runtime_membership_closed_loop`
10. `cargo test -p novovm-node v2_matrix_a_order_perturbation_consistency`
11. `cargo test -p novovm-node v2_matrix_b_multi_source_conflict_consistency`
12. `cargo test -p novovm-node v2_matrix_c_weak_network_disturbance_consistency`
13. `cargo test -p novovm-node v2_matrix_d_multi_region_view_consistency`
14. `cargo test -p novovm-node relay_path_tests`
15. `cargo test -p novovm-node queue_replay_smoke`

## Governance rule

Any change to scheduler source/token/manual-env-lock semantics must update tests first,
and cannot bypass this matrix in production.

## M7 固化：CI / 发布前置门（唯一入口）

- CI 主线签收入口：`cargo run -p novovm-node --bin supervm-mainline-gate`
- 主线状态唯一来源：`artifacts/mainline-status.json`
- 规则：
  - 手工 `cargo test` 组合只用于调试定位，不作为主线签收口径
  - 未通过 Gate Runner，不得进入发布/rollout

## M8 固化：发布 / rollout 前置门冻结

- 入口约束：
  - `scripts/_compat/Invoke-NovovmctlForward.ps1` 对 `up/daemon/rollout/rollout-control/lifecycle` 强制执行主线状态校验
- 校验规则：
  - 必须存在 `artifacts/mainline-status.json`
  - `schema` 必须为 `supervm-mainline-status/v2`
  - 主线 gate（固定 lockset）必须全部为 `true`
- 拒绝规则：
  - 任一条件不满足即拒绝透传，不允许以手工测试结果替代状态文件

## M9 固化：状态时效闸门冻结

- 时效来源：
  - `artifacts/mainline-status.json` 的 `generated_utc`
- 默认窗口：
  - `3600s`（1 小时）
- 可配置项：
  - `NOVOVM_SUPERVM_MAINLINE_STATUS_MAX_AGE_SECONDS`
  - 必须为 `>= 0` 的整数
- 规则：
  - `generated_utc` 缺失/非法 -> 拒绝透传
  - 状态年龄超过窗口 -> 拒绝透传
  - 必须刷新 Gate Runner 后才能继续发布/rollout

## M10 固化：状态时效闸门冻结契约接入

- 冻结契约测试：`mainline_status_freshness_gate_contract_is_frozen`
- 归属：`crates/novovm-node/src/bin/novovm-node.rs`
- 固定 Gate：已并入 `supervm-mainline-gate` 主线必跑链（第 8 步）
- 冻结语义：
  - 时效字段：`generated_utc`
  - 默认窗口：`3600s`
  - 可配置项：`NOVOVM_SUPERVM_MAINLINE_STATUS_MAX_AGE_SECONDS`
  - 过期拒绝：`mainline gate status expired`

## M11 固化：四层网络主线功能装配基线

- 治理前提（冻结）：
  - `M1-M10` 已冻结完成，不再重开入口/Gate/发布路线讨论
- 后续四层网络功能包唯一签收口径：
  - `cargo run -p novovm-node --bin supervm-mainline-gate`
  - `artifacts/mainline-status.json`
- 后续功能包统一交付格式（强制）：
  - `Milestone`
  - `Gate`
  - `Debt`
- 约束：
  - 不允许绕开 canonical gate runner 进行主线签收
  - 不允许以散装测试组合作为发布依据

## M12 固化：Milestone / Gate / Debt 统一交付产物

- canonical gate runner 运行后额外产物：
  - `artifacts/mainline-delivery-contract.json`
- 产物语义（冻结）：
  - `milestone`：当前主线里程碑标识
  - `gate_entry`：唯一主线 Gate 入口
  - `status_source`：唯一状态源（`mainline-status.json`）
  - `overall_gate`：主线 Gate 完成度
  - `debt`：主线 Debt（当前固定为 `0`）
- 约束：
  - 不改既有 Gate 语义
  - 不改 `mainline-status.json` schema 语义
  - 仅新增统一交付契约产物，供后续四层功能包复用

## M13 固化：V2 第一阶段分布式一致裁决矩阵（主线必跑）

- 固定分组：
  - A 组：顺序扰动一致性（`v2_matrix_a_order_perturbation_consistency`）
  - B 组：多来源冲突一致性（`v2_matrix_b_multi_source_conflict_consistency`）
  - C 组：弱网扰动一致性（`v2_matrix_c_weak_network_disturbance_consistency`）
  - D 组：多区域视角一致性（`v2_matrix_d_multi_region_view_consistency`）
- 签收要求：
  - 四组必须全部通过，且进入 `supervm-mainline-gate` 固定执行链
  - 四组状态必须写入 `artifacts/mainline-status.json` 的 gate 分项
  - 任一分项失败即主线签收失败
