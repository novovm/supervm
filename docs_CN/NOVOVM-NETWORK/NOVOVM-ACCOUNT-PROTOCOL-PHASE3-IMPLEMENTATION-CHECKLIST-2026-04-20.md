# NOVOVM Account Protocol Phase 3 实施清单（2026-04-20）

Status: AUTHORITATIVE CHECKLIST（Phase 3）  
Scope: 统一资产视图在真实主线上的后续收口门禁

## 目标

`Phase 3` 只做一件事：

`统一资产视图`

本阶段的目标不是造一个新的统一资产账本，而是：

- 让现有资产源在真实主线读面上围绕 `account_id` 聚合可见
- 逐步扩大聚合源覆盖范围
- 逐步统一余额、锁定、抵押、债务等视图语义

## Cut 顺序

1. `Cut 1`：真实主线读面建立
   - `account_balance`
   - `account_assets`
   - `ownership_subject = account_id`
   - 基于现有 `native_execution_store` 聚合
2. `Cut 2`：扩视图聚合源
   - 已成立：`pledge`
   - 已成立：`treasury exposure`
   - 只允许扩已有真实主线路径中的现有资产源
   - 只允许做读面聚合，不允许借机引入新账本
   - 当前未成立：`staking`
3. `Cut 3`：统一读面语义
   - 统一 `available / locked / collateral / debt / reserved / pending` 等视图字段语义
   - 仍然只做展示合同，不做账本重构

## 双轨兼容边界

当前阶段允许：

- 输入层继续兼容 `account_id / uca_id`
- 资产来源继续来自多个现有模块
- 视图层对不同来源做统一主体聚合

当前阶段不允许：

- 把多个来源的聚合视图包装成“已经有统一账本”
- 把临时聚合字段提升为账本承诺字段
- 让非 `account_id` 主体重新进入资产归属主语义
- 把同一资产的多来源暴露压成无来源单值真相

每一刀都必须给出一组“现有资产源 -> 聚合视图输出”的对照样例，至少说明：

- 输入主体是哪个 `account_id`
- 聚合了哪些现有资产源
- 各来源在输出中如何映射到统一字段
- 输出中的资产归属最终落到哪个 `account_id`

## 明确禁止项

本阶段明确禁止：

- 禁止新增 unified asset ledger
- 禁止引入 `asset_root`
- 禁止把映射资产主协议揉进 `Phase 3`
- 禁止把隐私子账户资产空间揉进 `Phase 3`
- 禁止把 proof-structured asset state 提前写成当前事实
- 禁止把单模块余额直接包装成“统一账户资产系统已完成”
- 禁止把 `account_assets / account_balance` 变成写路径或账本入口
- 禁止为了“看起来完整”而伪造 `staking` 或其他无真实状态源的资产视图

## 合并门禁

合并门禁只有一句话：

`Phase 3 只允许扩统一资产视图，不允许借实现视图之名偷长统一资产账本。`

PR 级最小验收必须同时满足：

- 新增或扩展的资产视图方法以 `account_id` 为归属主体
- 明确列出聚合来源
- 同一资产若来自多个 `source`，输出必须保持来源可区分，不能压成单值真相
- 明确说明输出只是视图，不是新账本承诺
- 明确保证 `account_assets / account_balance` 仍是只读接口
- 不引入 `asset_root`
- 不引入 mapped asset settlement protocol
- 不引入 privacy subaccount asset space

建议在 code review 中直接用以下判定语：

`若该变更不能证明它只是对现有资产源做 account_id 归属下的聚合展示，或它让 account_assets / account_balance 变成写路径，或它隐含引入了新账本、asset_root、映射资产主协议或隐私资产空间，则该变更不得合入主线。`
