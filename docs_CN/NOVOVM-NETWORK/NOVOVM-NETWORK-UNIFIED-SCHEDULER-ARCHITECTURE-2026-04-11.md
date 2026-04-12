# NOVOVM Network Unified Scheduler Architecture (2026-04-11)

## 生产唯一入口

`novovmctl` 是 SuperVM / NOVOVM 生产节点唯一调度入口。

## 启动链路

1. `novovmctl` 解析配置与运行角色
2. `novovmctl` 启动 `novovm-node`
3. 启动时强制注入调度上下文：
   - `NOVOVM_SCHED_SOURCE=novovmctl`
   - `NOVOVM_SCHED_TOKEN=<runtime token>`
   - `NOVOVM_SCHED_REQUIRED=1`
   - `NOVOVM_SINGLE_SOURCE_STRICT=1`
   - `NOVOVM_SUPERVM_MANUAL_ROUTE_ENV_LOCK=1`
4. `novovm-node` 启动后执行 gate：
   - scheduler source/token 校验
   - single-source strict 一致性校验
   - 手工 route env 黑名单校验

## Node Gate 规则

- `NOVOVM_SCHED_REQUIRED=1` 或 `NOVOVM_SINGLE_SOURCE_STRICT=1` 时：
  - 必须 `NOVOVM_SCHED_SOURCE=novovmctl`
  - 必须存在 `NOVOVM_SCHED_TOKEN`
  - 不满足直接失败

## Manual Route Env Lock（生产防旁路）

- 启用 `NOVOVM_SUPERVM_MANUAL_ROUTE_ENV_LOCK=1` 时，禁止手工注入：
  - L3 policy/profile/family/version 相关关键 env
- 命中黑名单直接失败

## 设计目标

- 防止“多入口并行 worldline”
- 防止“手工 env 绕过调度主线”
- 保证四层网络能力统一在同一调度主线运行

