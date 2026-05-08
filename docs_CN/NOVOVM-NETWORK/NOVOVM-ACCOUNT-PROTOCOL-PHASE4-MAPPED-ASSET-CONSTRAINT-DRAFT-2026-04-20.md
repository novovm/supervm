# NOVOVM Account Protocol Phase 4 映射资产主协议约束设计稿（2026-04-20）

Status: AUTHORITATIVE CONSTRAINT DRAFT（Phase 4）  
Scope: 进入映射资产主协议前的冻结条件、核心 invariant 与禁止混层规则

## 目的

本文件不实现 `Phase 4`，只回答三件事：

- 什么情况下才允许进入 `Phase 4`
- 映射资产主协议必须满足哪些 invariant
- 如何避免把 bridge / mint / ledger / privacy 混成一层

本文件的作用是让 `Phase 4` 在开始之前先被约束住，而不是边做边长。

## 当前前提

当前统一账户阶段状态为：

- `v1-min`：已完成
- `Phase 2`：已完成
- `Phase 3 / Cut 1`：已完成
- `Phase 3 / Cut 2`：已完成

当前已经成立的是：

- `account_id` 为 canonical subject
- execution 已 account-first
- unified asset view 已进入真实主线读面

当前尚未成立的是：

- unified asset ledger
- `asset_root`
- mapped asset master protocol
- privacy subaccount asset space
- proof-structured asset state

因此，`Phase 4` 的正确目标不是：

`重做资产系统`

而是：

`把映射资产收敛为统一主协议`

## 进入条件

只有当以下条件同时满足时，才允许进入 `Phase 4` 主实现：

1. 统一资产视图已封盘并进入公开资料面
2. 现有映射资产相关审计与归属已稳定落到 `account_id`
3. `account_balance / account_assets` 已能稳定展示映射资产相关可见性
4. custody / risk / audit 边界已可单独描述并冻结
5. 不需要依赖 `asset_root` 才能定义映射资产主协议

只要以上任一条件不满足，就不得把 `Phase 4` 写成主线实现任务。

## 全局硬约束

`Phase 4` 不得引入新的全局资产状态源。

当前允许的状态真相只能来自：

- 外链锁定证明与其可追溯引用
- 现有执行模块
- 现有清算模块
- 已封盘的现有国库 / 风险 / 审计主线

这意味着：

- `Phase 4` 不是新账本起点
- 不能借映射资产协议之名额外长出一套全局资产状态源
- 任何新增状态都必须能回到“外链锁定真相”或“现有主线模块真相”

## 核心 invariant

`Phase 4` 必须同时满足以下 invariant：

### 1）主体 invariant

- 映射资产的归属主体必须是 `account_id`
- 外链地址、桥地址、锁仓地址都不是 canonical asset owner
- 审计、mint、burn、redeem 的最终归属必须能追溯到 `account_id`

### 2）一一映射 invariant

- 每个映射资产都必须对应明确的 `source_chain + source_asset + proof_policy + custody_boundary`
- 不允许同一映射资产名义下混入多套不兼容的证明或托管语义
- `nETH / nBTC / nUSDT` 这类对象必须是协议对象，不是市场文案别名

### 3）供给 invariant

- 映射资产供给的变化必须只来自协议定义的 `mint / burn / redeem` 流
- 不允许从 `account_assets`、视图层或其他只读聚合层侧写状态
- 不允许把临时统计字段变成供给真相

### 4）审计 invariant

- lock / proof / mint / burn / redeem 必须形成完整可追溯审计链
- 任一映射资产的状态变化都必须能回答：
  - 谁拥有
  - 为什么产生
  - 根据什么证明产生
  - 根据什么规则销毁或赎回

### 5）风险 invariant

- custody policy、proof policy、redeemability、risk boundary 必须可单独冻结
- 不允许把风险控制散落到多条链专有脚本里
- 不允许让单条链特例绕开统一主协议

## 明确禁止项

`Phase 4` 明确禁止以下做法：

- 禁止把 bridge 实现直接等同于 mapped asset protocol
- 禁止把 mint 逻辑直接写成统一资产账本
- 禁止把 ledger 重构和 mapped asset protocol 绑定成同一轮
- 禁止把 privacy subaccount asset space 和 mapped asset protocol 一次性混做
- 禁止引入 `asset_root` 作为进入 `Phase 4` 的前置依赖
- 禁止在 `Phase 4` 中引入新的全局资产状态源
- 禁止把单链特例路径包装成“主协议已经成立”

## 分层规则

`Phase 4` 必须继续遵守以下分层：

1. `bridge / custody`
   - 负责锁定、证明输入、外链引用
2. `mapped asset protocol`
   - 负责统一协议对象与主流程合同
3. `asset view`
   - 只负责展示与聚合，不负责供给写入
4. `ledger / root`
   - 不在本阶段引入

如果一个设计同时修改了以上多层中的三层及以上，应默认视为越级设计。

## 最小协议对象约束

进入 `Phase 4` 时，至少应冻结以下对象语义，而不是先冻结账本：

- `mapping_id`
- `source_chain`
- `source_asset`
- `proof_policy_id`
- `custody_policy_id`
- `target_asset_id`
- `redeemable`
- `mint_flow_contract`
- `burn_flow_contract`

这些对象的作用是定义协议边界，不是提前承诺统一账本结构。

## 退出条件

`Phase 4` 只有在以下条件都满足时才允许封盘：

1. 至少一组主流映射资产家族已按统一协议跑通
2. 统一审计链已经稳定
3. 统一归属稳定落到 `account_id`
4. 视图层可以展示映射资产，但不会反向成为写路径
5. 不引入 `asset_root` 仍可完成主协议封盘

## 对 Phase 3 的保护规则

在 `Phase 4` 开始前，必须继续保护 `Phase 3`：

- `account_assets / account_balance` 只允许读
- 不允许成为写路径
- 不允许成为账本入口
- 任何试图从视图层写回资产状态的设计，默认视为违规设计

## 建议对外口径

`Phase 4` 当前只有约束设计稿，不是已完成能力。当前唯一成立的资产层能力仍是：统一资产视图已进入真实主线读面；映射资产主协议尚未进入主线实现。`

## 相关文档

- `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-TRIGGER-CHECKLIST-2026-04-20.md`
- `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-FAILURE-MODES-2026-04-20.md`
