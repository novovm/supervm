# NOVOVM Account Protocol Phase 3 / Cut 1 封盘文档（2026-04-20）

Status: FINAL SEAL（Phase 3 / Cut 1）  
Scope: 统一资产视图进入真实主线读面，并以 `account_id` 为主体聚合展示现有资产源

## 本轮目标

`Phase 3 / Cut 1` 只做一件事：

`把统一资产视图接入真实主线读面`

本轮不做：

- 新的统一资产账本
- `asset_root`
- 映射资产主协议
- 隐私子账户资产空间
- proof-structured 资产状态

## 已成立能力

本轮已成立的能力如下：

- `account_balance`
  - 已进入真实 `novovm-node -> mainline_query -> unified_account_surface` 读面
- `account_assets`
  - 已进入真实 `novovm-node -> mainline_query -> unified_account_surface` 读面
- `ownership_subject`
  - 已固定为 `account_id`
- 数据来源
  - 来自现有 `native_execution_store.account_asset_balances`
  - 来自现有 `native_execution_store.credit_vaults`
- 聚合语义
  - 在不引入新账本的前提下，对现有资产源做主体级聚合展示

这意味着当前统一资产视图已经成立为：

`主线级、账户主体级、现有存储聚合的资产可见性能力`

而不是：

`新的统一资产账本`

## 明确未成立能力

本轮明确未成立、也未对外宣称成立的范围如下：

- unified asset ledger
- `asset_root`
- mapped asset settlement protocol
- privacy subaccount asset space
- proof-structured asset state

换言之，本轮成立的是：

`统一资产视图`

不是：

`统一资产系统已经完成`

## 当前主线路径

当前统一资产视图的真实主线路径为：

`novovm-node (bin) -> mainline_query -> unified_account_surface -> native_execution_store`

当前真实读面方法合同：

| 方法 | 当前语义 | 状态 |
| --- | --- | --- |
| `account_balance` | 查询指定 `account_id` 在现有资产源上的聚合余额 / 抵押 / 债务视图 | 已封盘 |
| `account_assets` | 查询指定 `account_id` 的现有聚合资产与仓位视图 | 已封盘 |

## 验证结果（2026-04-20 本地实际执行）

本轮基于以下真实执行结果封盘：

- `cargo fmt --all`
- `cargo test -p novovm-node`
- `cargo clippy -p novovm-node --all-targets -- -D warnings`
- `cargo run -p novovm-node --bin supervm-mainline-gate`
  - 结果：`supervm mainline gate passed`
  - 结果：`L1=100% L2=100% L3=100% L4=100% Overall=100%`

本轮还补了真实入口级回归，直接验证：

- `account_balance`
- `account_assets`
- `ownership_subject = account_id`
- 基于现有 `native_execution_store` 资产源聚合

## 建议对外口径

`Phase 3 / Cut 1 已完成：统一资产视图已进入真实主线读面，资产归属以 account_id 为主体聚合展示，但尚未引入统一资产账本。`
