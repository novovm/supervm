# NOVOVM Account Protocol Cut B / AccountMode 触发条件清单（2026-04-20）

Status: AUTHORITATIVE TRIGGER CHECKLIST（Cut B / AccountMode）  
Scope: 定义统一账户在极少数情况下引入 `AccountMode` 可选标签层的唯一触发条件；在条件未满足前，统一账户必须保持 `Cut A` 已封盘，且 `Cut B` 默认 `No-Go`

## 目的

本文件只回答一件事：

`什么时候才允许统一账户引入 AccountMode（Cut B）这个可选标签层。`

本文件不是 `Cut B` 的设计稿，也不是 `Cut B` 的实现计划。

更重要的是：

`Cut B` 不是当前统一账户架构中的必需层。

本文件的作用是：

- 把 `Cut B` 从“未来也许要做”收成“默认不做，只有满足条件才允许做”
- 防止用“接口更统一”或“结构更优雅”之类理由提前进入 `Cut B`
- 保持当前统一账户的稳定状态：
  - `account_id` 主体层已完成
  - `asset view` 视图层已完成
  - `KeyAlgo / Cut A` 已实现并封盘
  - 主线能力由 `KeyAlgo + ExecutionPolicy` 表达

## 必须全部满足（ALL REQUIRED）

只有以下条件全部满足，才允许进入 `Cut B`：

### 1）`KeyAlgo / Cut A` 已稳定运行

必须满足：

- 多算法绑定元数据已在真实主线稳定运行
- `secp256k1 / ed25519 / mldsa87` 的最小支持未污染主体语义
- `Cut A` 未出现要求回滚的兼容性问题
- `account_id` 仍然是唯一 canonical subject

### 2）出现真实“默认行为需求”

必须满足：

- 至少出现一个真实生产场景，要求：
  - `账户需要一个稳定的展示标签或控制面提示`
- 该需求来自真实产品、真实运营或真实控制面
- 不是因为“设计更优雅”或“接口更统一”
- 不是为了让 `AccountMode` 参与执行、资产或安全路径

### 3）当前需求不能仅靠 `KeyAlgo` 表达

必须满足：

- 经过评估，当前需求既不能由 `KeyAlgo` 表达，也不属于 `ExecutionPolicy`
- 如果 `KeyAlgo` 已足以表达该需求，则不得进入 `Cut B`
- 如果 `ExecutionPolicy` 已足以表达该需求，则不得进入 `Cut B`
- `AccountMode` 不能只是 `KeyAlgo` 的别名包装层
- `AccountMode` 不能成为 `ExecutionPolicy` 的前置代理层

### 4）不违反既有阶段约束

必须满足：

- 不引入新的全局资产状态源
- 不触碰统一资产账本
- 不触碰映射资产供给或结算主协议
- 不触碰隐私资产空间
- 不改变 supply / audit / ownership invariant
- 不违反 `Phase 4` 的 `Constraint Draft` 与 `Failure Modes`

## 明确 No-Go 条件

以下任一情况成立，即禁止进入 `Cut B`：

- 仅为“统一接口”而推进
- 仅为“未来可能需要”而提前设计
- 当前需求已经能被 `KeyAlgo` 表达
- 当前需求已经能被 `ExecutionPolicy` 表达
- 进入 `Cut B` 需要引入新账本或新状态源
- 进入 `Cut B` 会改变 `account_id` 语义
- 进入 `Cut B` 会让 `KeyAlgo` 间接触发隐私、PQ 或混合行为
- 进入 `Cut B` 会让 `AccountMode` 参与执行路由、资产语义或安全约束
- 进入 `Cut B` 会影响 `account_balance / account_assets` 的只读聚合边界

## 一票否决语句（用于 code review）

以下语句可直接用于 code review：

- `This change introduces AccountMode without satisfying trigger conditions.`
- `This change attempts to encode behavior in AccountMode that belongs to KeyAlgo or ExecutionPolicy.`
- `This change advances Cut B without a real production requirement.`
- `This change turns AccountMode into a backdoor for privacy, PQ, or hybrid execution behavior.`
- `This change makes AccountMode a mainline semantic layer instead of an optional label.`

## 当前结论

当前统一账户对 `Cut B / AccountMode` 的正式判定是：

`No-Go`

原因不是 `Cut B` 永远不做，而是：

- `Cut A` 已封盘
- 当前没有真实、不可由 `KeyAlgo + ExecutionPolicy` 表达的“标签型需求”
- 当前系统应继续保持“冻结核心 + 条件触发扩展”的稳定阶段

因此，当前对 `Cut B` 的正确定位是：

`可选标签层，不是主线语义层。`

## 建议对外口径

`Cut B / AccountMode 目前尚未进入实现，也不是当前主线必需层。只有当统一账户出现真实的标签型需求，且该需求不能由 KeyAlgo 或 ExecutionPolicy 表达，同时不违反既有阶段约束时，才允许以可选标签形式进入。`
