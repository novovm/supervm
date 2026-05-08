# NOVOVM Account Protocol Cut A / KeyAlgo 实施清单（2026-04-20）

Status: AUTHORITATIVE CHECKLIST（Cut A / KeyAlgo）  
Scope: 统一账户 `KeyAlgo` 最小实现门禁；后续只允许扩算法元数据、校验与审计可见性，不允许顺手进入 `Cut B / Cut C`

## 目标

`Cut A` 只做一件事：

`为统一账户补齐密钥算法元数据、持有证明校验和审计可见性`

本阶段的目标不是：

- 创建新的账户主体分类
- 引入账户模式层
- 引入执行策略层
- 引入隐私账户行为
- 引入任何资产账本或资产视图变化

## 当前已封盘范围

当前 `Cut A` 已封盘的最小实现范围如下：

- `primary_key_binding`
- `UcaKeyAlgo`
- `UcaKeyProofType`
- `UcaPrimaryKeyBinding`
- `ua_createUca` 的 `声明 -> 校验 -> 绑定`
- `ua_rotatePrimaryKey` 的 `声明 -> 校验 -> 绑定`
- `secp256k1 / ed25519 / mldsa87` 最小支持
- `ua_getAccount` 与审计事件中的 `key_algo` 可见性

## 双轨兼容边界

当前阶段允许：

- 继续兼容旧 `primary_key_ref` 输入
- 在不提供 `KeyAlgo` 元数据时保留旧创建/轮换路径
- 在提供 `KeyAlgo` 元数据时执行真实校验并写入绑定元数据

当前阶段不允许：

- 因为 `key_algo` 重算或分裂 `account_id`
- 把 `key_algo` 当成统一账户主体分类
- 把隐私语义直接挂在 `key_algo`
- 借 `KeyAlgo` 之名修改 `account_balance / account_assets`
- 借 `KeyAlgo` 之名引入新状态真相源

每次新增或扩展 `KeyAlgo` 相关实现，都必须给出一组“输入声明 -> 校验 -> 绑定 -> 查询/审计可见”的对照样例，至少说明：

- 输入声明的 `key_algo`
- 输入公钥和 proof 的类型
- 校验是否通过
- 绑定后在 `ua_getAccount` 中如何可见
- 绑定后在审计中如何可见

## 明确禁止项

本阶段明确禁止：

- 禁止顺手引入 `AccountMode`
- 禁止顺手引入 `ExecutionPolicy`
- 禁止把 `KeyAlgo` 变成主体分裂条件
- 禁止把隐私语义挂到 `KeyAlgo`
- 禁止把 `mldsa87` 提升成强制账户主路径
- 禁止引入多算法主密钥迁移状态机
- 禁止修改 `account_balance / account_assets` 语义
- 禁止引入任何统一资产账本、映射资产协议、隐私资产空间或 proof-root 账户树

## 合并门禁

合并门禁只有一句话：

`KeyAlgo 仅描述“绑定了什么算法”，不得决定 account_id、资产归属、隐私语义或统一账户主体分裂。`

PR 级最小验收必须同时满足：

- `key_algo` 只作为绑定元数据和校验依据出现
- `account_id` 不重算、不分裂
- `ua_createUca / ua_rotatePrimaryKey` 维持“声明 -> 校验 -> 绑定”闭环
- 绑定失败不会污染账户主体状态
- `ua_getAccount` 和审计事件可见 `key_algo`
- 不引入 `AccountMode`
- 不引入 `ExecutionPolicy`
- 不引入任何资产层变化

建议在 code review 中直接用以下判定语：

`若该变更试图通过 KeyAlgo 改变 account_id、资产归属、隐私语义、统一账户主体分裂，或它顺手引入了 AccountMode、ExecutionPolicy、隐私账户能力、统一资产账本、映射资产协议或 proof-root 账户树，则该变更不得合入主线。`

另一条长期门禁：

`任何试图通过 KeyAlgo 直接触发 PostQuantum / Privacy / Hybrid 账户行为的改动，均视为越级进入 Cut B / Cut C。`
