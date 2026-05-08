# NOVOVM Account Protocol Phase 4 触发条件清单（2026-04-20）

Status: AUTHORITATIVE TRIGGER CHECKLIST（Phase 4）  
Scope: 决定 `Phase 4` 是否允许从“约束冻结态”进入“主线实现态”的唯一开关清单

## 目的

本文件不是设计稿，不是路线图，也不是实现任务单。

本文件只做一件事：

`冻结 Phase 4 何时允许正式进入实现。`

这意味着后续是否开启 `Phase 4`，不再依赖口头判断，而依赖可检查、可否决的触发条件。

## 当前状态

当前统一账户/资产线状态为：

- `Phase 2`：已完成
- `Phase 3 / Cut 1`：已完成
- `Phase 3 / Cut 2`：已完成
- `Phase 4`：约束与 failure modes 已双向冻结，未进入实现

因此，当前默认结论仍然是：

`Phase 4 不启动。`

只有当本清单全部满足时，才允许把 `Phase 4` 升级为实现阶段。

## 唯一触发规则

`Phase 4` 只有在以下条件全部满足时，才允许进入主线实现：

1. 存在至少一类真实的映射资产运行时来源
   - 不是占位对象
   - 不是文档对象
   - 不是伪造测试态来源
2. 该来源可以稳定归属到 `account_id`
   - 归属不能落回外链地址、桥地址或托管地址
3. 不需要引入新的全局资产状态源
   - `Phase 4` 不能成为新账本起点
4. `Phase 3` 现有视图已足以承载该来源的可见性
   - 至少能在 `account_balance / account_assets` 中表达来源、分类和归属
5. custody / risk / audit 三条边界可单独描述并冻结
   - 不能混成一个实现块
6. supply invariant 可通过单一主流程验证
   - 必须能解释 `lock / proof / mint / burn / redeem`
7. 不依赖下列未完成层：
   - `asset_root`
   - unified asset ledger
   - privacy subaccount asset space
   - proof-root account tree
8. 该实现切片不会破坏已冻结约束
   - 不违反 Phase 4 constraint draft
   - 不触发 Phase 4 failure modes

只要以上任一项不成立，结论就是：

`Phase 4 不得启动。`

## 必备证据

每次试图开启 `Phase 4` 时，至少必须提供以下证据：

1. 真实来源说明
   - 来源对象是什么
   - 为什么它是运行时真来源
2. 主体归属说明
   - 为什么最终 owner 是 `account_id`
3. 视图承载说明
   - 该来源如何被 `account_balance / account_assets` 只读展示
4. invariant 说明
   - 如何保证供给守恒
   - 如何保证 audit 链连续
5. 分层说明
   - bridge / custody
   - mapped asset protocol
   - asset view
   - ledger / root
   必须仍然分层

若以上任一证据缺失，则默认不得从文档冻结态进入实现态。

## 一票否决条件

以下任一情况出现时，即使其他条件满足，也不得开启 `Phase 4`：

1. 需要新增全局资产状态源
2. 需要让地址重新成为 canonical owner
3. 需要让 `account_assets / account_balance` 变成写路径
4. 需要把 bridge / mint / ledger / privacy 混成一层
5. 需要先引入 `asset_root` 才能工作
6. 无法给出完整审计链
7. 无法证明 supply invariant

## 建议评审问法

在进入 `Phase 4` 实现评审前，必须先回答这几问：

1. 新来源是真来源，还是为了推进阶段而制造的伪来源？
2. 最终资产 owner 是 `account_id`，还是某个地址对象？
3. 当前 `Phase 3` 视图能否只读承载它？
4. 这个实现会不会变相长出新账本？
5. 这个实现是否绕开了 treasury / risk / audit 主线？
6. 这个实现是否依赖未完成的 root / privacy / proof 层？

只要其中任一问题回答不清，默认 `No-Go`。

## 最终判定

本文件只给两种结论：

- `Go`：全部条件满足，可进入 `Phase 4` 最小实现切片
- `No-Go`：任一条件未满足，继续停留在 `Phase 3` 与 `Phase 4` 约束冻结态

当前结论是：

`No-Go`

## 相关文档

- `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-MAPPED-ASSET-CONSTRAINT-DRAFT-2026-04-20.md`
- `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-FAILURE-MODES-2026-04-20.md`
- `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE3-IMPLEMENTATION-CHECKLIST-2026-04-20.md`
- `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-MVP-SLICE-TEMPLATE-2026-04-21.md`
