# NOVOVM Account Protocol Cut C / ExecutionPolicy 实施清单（2026-04-20）

Status: AUTHORITATIVE CHECKLIST（Cut C / ExecutionPolicy）  
Scope: `ExecutionPolicy` 最小实现门禁；后续只允许扩执行强约束，不允许顺手进入 `AccountMode`、映射资产或隐私资产账本

## 目标

`Cut C` 只做一件事：

`让 KeyAlgo + ExecutionPolicy 成为主线中不可绕过的执行许可条件`

本阶段的目标不是：

- 创建新的账户主体类别
- 引入 `AccountMode`
- 引入新的资产账本或隐私资产空间
- 让 gateway / adapter 自己定义 policy 语义

## 当前已封盘范围

当前 `Cut C` 已封盘的最小实现范围如下：

- `execution_policy`
  - `Standard`
  - `PqRequired`
  - `PrivacyRequired`
- 主线唯一 resolve + enforcement
- gateway 只透传 policy
- `PqRequired` 的明确拒绝路径
- `PrivacyRequired` 的明确拒绝路径
- `receipt / trace / audit` 中的 policy 可见性

## 双轨兼容边界

当前阶段允许：

- 显式传入 `execution_policy`
- 未显式传入时默认 `Standard`
- gateway / `TxIR` 透传 policy 到主线
- 在主线用 `KeyAlgo + ExecutionPolicy` 做真实校验

当前阶段不允许：

- 让 `execution_policy` 重算或分裂 `account_id`
- 让 `execution_policy` 修改 `account_balance / account_assets`
- 让 policy 失败变成 silent fallback 或自动降级
- 在 gateway / adapter 再长第二套 resolve 或 enforcement
- 借 `ExecutionPolicy` 之名引入新的状态真相源

每次扩展 `ExecutionPolicy` 相关实现，都必须给出至少一组“输入 policy -> resolve -> enforcement -> receipt/trace/audit 可见”的对照样例，至少说明：

- 输入的 `execution_policy`
- 当前绑定的 `key_algo`
- enforcement 是否通过
- 若失败，失败原因是什么
- 结果如何在 `receipt / trace / audit` 中可见

## 明确禁止项

本阶段明确禁止：

- 禁止引入 `AccountMode`
- 禁止把 `ExecutionPolicy` 变成主体分裂条件
- 禁止让 `ExecutionPolicy` 触碰 `account_balance / account_assets`
- 禁止把 policy 失败改成降级执行
- 禁止让 gateway / adapter 长出第二套 policy 语义
- 禁止借 `Cut C` 顺手引入 `Cut B`
- 禁止借 `Cut C` 顺手引入 `Phase 4` 行为
- 禁止引入统一资产账本、映射资产协议、隐私资产空间或 `asset_root`

## 合并门禁

合并门禁只有一句话：

`ExecutionPolicy 只负责执行强约束，不得决定 account_id、资产归属、隐私资产账本或统一账户主体分裂。`

PR 级最小验收必须同时满足：

- `execution_policy` 只影响执行路由与拒绝路径
- `account_id` 不重算、不分裂
- `PqRequired / PrivacyRequired` 失败时明确拒绝
- 不存在 silent fallback
- `receipt / trace / audit` 可见：
  - `execution_policy`
  - `policy_enforced`
  - `policy_rejection_reason`
- gateway 只透传，不做第二套 resolve
- 不引入 `AccountMode`
- 不引入任何资产层变化

建议在 code review 中直接用以下判定语：

`若该变更试图让 ExecutionPolicy 改变 account_id、资产归属、隐私账本、统一账户主体分裂，或它引入了第二套 resolve/enforcement、silent fallback、AccountMode、映射资产协议、asset_root 或其他 Phase 4 行为，则该变更不得合入主线。`

另一条长期门禁：

`任何让 ExecutionPolicy 反向触发统一资产账本、隐私资产空间、mapped asset 或 proof-root 的改动，均视为越级进入 Phase 4 / 5 / 6。`
