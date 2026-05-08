# NOVOVM Account Protocol Phase 4 失败模式清单（2026-04-20）

Status: AUTHORITATIVE FAILURE-MODE LIST（Phase 4）  
Scope: `Phase 4` 映射资产主协议在进入实现前的一票否决清单、结构性失败清单与 review 硬规则

## 1. Purpose

本文件不是设计稿，也不是路线图。

本文件只做一件事：

`把 Phase 4 最常见、最危险、最容易导致架构回退的失败模式冻结成正式否决清单。`

本文件用于：

- code review
- 设计评审
- PR 否决
- 防止 `Phase 4` 反向污染 `Phase 3`

## 2. Red-line failures

以下失败模式属于一票否决：

1. 新增全局资产状态源
   - `Phase 4` 不得成为新账本起点
2. 映射资产绕过 `account_id` 归属
   - 外链地址、桥地址、锁仓地址都不是 canonical asset owner
3. 地址重新成为 fee / nonce / asset owner
   - 这会直接破坏已经完成的 `Phase 2`
4. `Phase 4` 实现反向落成统一资产账本
   - mapped asset protocol 不是 unified asset ledger
5. 锁定证明与铸造记录不可追溯
   - 无法回答“为什么铸造、根据什么证明铸造”
6. 供给不守恒
   - 供给变化不能完整落在 `mint / burn / redeem` 主流程上
7. audit 链断裂
   - lock / proof / mint / burn / redeem 不能形成完整审计链
8. bridge / mint / ledger / privacy 混层
   - 一个设计同时承担三层以上职责，默认视为越级
9. `account_assets / account_balance` 变成写路径
   - Phase 3 读面不得反向成为账本入口
10. `asset_root` 被反向引入为前置依赖
   - 这会提前污染未完成的 proof / ledger 层

## 3. Structural failures

以下失败模式未必立刻打爆系统，但会导致后续必重构：

1. 视图接口反向变成写入口
   - 即使暂时“能跑”，也会把视图层污染成隐形账本
2. custody 边界和 risk 边界未分离
   - 后续无法单独冻结托管规则与风险规则
3. treasury / risk / audit 主线被新逻辑绕过
   - 会制造第二套真相
4. 映射资产对象没有 canonical source reference
   - 无法稳定回答资产从哪里来
5. fallback 兼容路径重新变成默认主路径
   - 会把 `account-first` 主线重新拖回 `address-first`
6. 单链特例路径先落地，再试图事后“抽象成主协议”
   - 结果通常是主协议永远收不回来
7. 聚合字段、缓存字段、报表字段被当成供给真相
   - 这会把只读层偷偷提升成状态层

## 4. Review checklist

每个 `Phase 4` 相关 PR 至少必须回答以下问题：

1. 有没有新增状态真相源？
2. 有没有破坏 `account_id` 作为 canonical ownership subject？
3. 有没有引入无法审计的 mint path？
4. 有没有把 `account_assets / account_balance` 变成写路径？
5. 有没有让 `Phase 4` 反向依赖未完成的 proof / account tree？
6. 有没有把 bridge、mint、ledger、privacy 混成一层？
7. 有没有让 treasury / risk / audit 主线失效或旁路？
8. 有没有让 fallback 兼容路径重新成为默认主路径？

只要其中任一问题回答不清，默认不得合入主线。

## 5. One-line rejection rules

以下语句可直接用于 review 否决：

- `This change introduces a new global asset state source.`
- `This change makes Phase 4 a ledger entry point.`
- `This change breaks account_id as canonical ownership subject.`
- `This change creates an unauditable mint path.`
- `This change turns account_assets/account_balance into write paths.`
- `This change reintroduces address-owned asset semantics.`
- `This change mixes bridge, mint, ledger, and privacy into one layer.`
- `This change makes Phase 4 depend on asset_root or proof trees that are not yet in mainline.`

## 建议使用方式

`Phase 4` 开始前，本文件应与以下文档一起使用：

- `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE3-IMPLEMENTATION-CHECKLIST-2026-04-20.md`
- `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-TRIGGER-CHECKLIST-2026-04-20.md`
- `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-MAPPED-ASSET-CONSTRAINT-DRAFT-2026-04-20.md`

这意味着：

`Phase 4` 的正向边界由 constraint draft 冻结，反向死法由 failure modes 冻结。
