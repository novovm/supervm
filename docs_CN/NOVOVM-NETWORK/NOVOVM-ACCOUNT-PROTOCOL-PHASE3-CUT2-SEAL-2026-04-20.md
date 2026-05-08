# NOVOVM Account Protocol Phase 3 / Cut 2 封盘文档（2026-04-20）

Status: FINAL SEAL（Phase 3 / Cut 2）  
Scope: 统一资产视图扩展到 `pledge` 与 `treasury exposure` 两类真实来源，并保持 `account_id` 归属、`source` 可追溯、`components` 可解释

## 本轮目标

`Phase 3 / Cut 2` 只做一件事：

`扩统一资产视图来源，但继续保持只聚合、不主导、不落账本`

本轮不做：

- `staking` 视图伪造接入
- unified asset ledger
- `asset_root`
- 映射资产主协议
- 隐私子账户资产空间
- proof-structured 资产状态

## 已成立能力

本轮已成立的能力如下：

- `account_balance`
  - 已扩展为显式 `components` 结构
  - 不再把多来源压成单值真相
- `account_assets`
  - 已扩展为结构化资产暴露视图
- `pledges`
  - 已基于现有 `credit_vaults` 进入主线读面
- `treasury_exposures`
  - 已基于现有 `treasury_settlement_journal` 进入主线读面
- `ownership_subject`
  - 继续固定为 `account_id`
- `source`
  - 每类聚合结果继续显式携带来源
- 聚合语义
  - 继续只读取现有真实状态源
  - 不引入新的全局资产状态源

这意味着当前统一资产视图已经从：

`account_id -> 单值余额查询`

升级为：

`account_id -> 一组有来源、有分类、有边界的资产暴露视图`

## 当前结构化输出事实

当前读面中已经形成以下结构化视图事实：

- `components`
  - `liquid_balance`
  - `pledge_locked_collateral`
  - `debt_outstanding`
  - `treasury_source_flow`
  - `treasury_settled_nov`
  - `treasury_reserve_bucket_exposure`
  - `treasury_fee_bucket_exposure`
  - `treasury_risk_buffer_exposure`
- `pledges`
  - 来自现有 `credit_vaults`
- `treasury_exposures`
  - 来自现有 `treasury_settlement_journal`

这里成立的是：

`可解释的统一资产视图`

不是：

`统一资产账本`

## 明确未成立能力

本轮明确未成立、也未对外宣称成立的范围如下：

- `staking`
- unified asset ledger
- `asset_root`
- mapped asset settlement protocol
- privacy subaccount asset space
- proof-structured asset state

特别说明：

- 当前没有可稳定归属到 `account_id` 的真实 `staking` 运行时状态源
- 因此本轮没有为了“看起来完整”而伪造 `staking` 视图

## 当前主线路径

当前统一资产视图的真实主线路径为：

`novovm-node (bin) -> mainline_query -> unified_account_surface -> native_execution_store`

当前真实读面方法合同：

| 方法 | 当前语义 | 状态 |
| --- | --- | --- |
| `account_balance` | 查询指定 `account_id` 在现有资产源上的结构化余额 / 抵押 / 债务 / treasury exposure 视图 | 已封盘（Cut 2） |
| `account_assets` | 查询指定 `account_id` 的结构化资产、仓位与 treasury exposure 清单 | 已封盘（Cut 2） |

## 验证结果（2026-04-20 本地实际执行）

本轮基于以下真实执行结果封盘：

- `cargo fmt --all`
- `cargo test -p novovm-node`
- `cargo clippy -p novovm-node --all-targets -- -D warnings`
- `cargo run -p novovm-node --bin supervm-mainline-gate`
  - 结果：`supervm mainline gate passed`
  - 结果：`L1=100% L2=100% L3=100% L4=100% Overall=100%`

本轮还补了真实入口级回归，直接验证：

- `pledge + treasury exposure` 进入 `account_balance / account_assets`
- `ownership_subject = account_id`
- 输出包含显式 `source`
- 输出包含显式 `components`
- 同一资产若来自多个来源，输出仍然可分辨来源，不会压成单值真相

## 建议对外口径

`Phase 3 / Cut 2 已完成：统一资产视图已扩展到 pledge 与 treasury exposure 两类真实来源，输出保持 account_id 归属、source 可追溯、components 可解释，仍未引入统一资产账本。`
