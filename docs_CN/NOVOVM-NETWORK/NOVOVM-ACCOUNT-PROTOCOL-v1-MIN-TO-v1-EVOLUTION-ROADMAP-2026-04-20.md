# NOVOVM Account Protocol v1-min -> v1 演进路线（2026-04-20）

Status: AUTHORITATIVE ROADMAP（Frozen）  
Scope: 统一账户从当前 `v1-min` 主体协议，演进到后续 `v1` 账户系统的冻结顺序、触发条件与禁止越级规则

## 目的

本文件用于冻结统一账户从 `v1-min` 到后续 `v1` 的演进顺序。

它回答的不是“未来还能加什么”，而是：

- 下一阶段只能做什么
- 哪些能力必须晚一点再做
- 哪些条件不满足时绝对不能提前冻结

本文件的作用是避免统一账户后续扩展再次回到“协议超前于运行时”的状态。

## 当前基线

当前统一账户公开基线见：

- `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-v1-2026-04-20.md`

当前已经成立的是：

- `account_id` 作为唯一主体方向
- 真实主线入口：`novovm-node -> mainline_query -> unified_account_surface`
- 主体创建、绑定、策略、nonce、审计与路由
- 统一资产视图读面：`account_balance / account_assets`
- `Cut A / KeyAlgo`
- `Cut C / ExecutionPolicy`

当前尚未成立的是：

- 统一资产账本
- 隐私子账户主实现
- proof-root 账户树
- 完整映射资产结算协议

因此，当前统一账户的正确定位仍然是：

`Subject Protocol`

而不是：

`Full Account System`

## 总体规则

统一账户后续演进必须遵守以下四条规则：

1. 先让系统默认以 `account_id` 运行，再扩账户对象。
2. 先做统一资产视图，再决定是否需要统一资产账本。
3. 先让隐私成为主账户子空间，再谈独立隐私账户体系。
4. 先证明运行时真的需要 proof-root，再冻结 proof-root 结构。

补充冻结规则：

5. `AccountMode` 不是主线必需层；在 capability-driven 模型下，默认由 `KeyAlgo + ExecutionPolicy` 表达能力，`AccountMode` 只有在无法替代的标签需求出现时，才允许作为可选标签进入。

## 阶段顺序（冻结）

统一账户后续演进顺序冻结如下：

1. `Phase 2`：Execution 强绑定
2. `Phase 3`：统一资产视图
3. `Phase 4`：映射资产主协议
4. `Phase 5`：隐私子账户进入主线
5. `Phase 6`：proof-root 账户树

禁止跳阶段推进。

## Phase 2：Execution 强绑定

### 目标

让系统开始默认以 `account_id` 为主体运行，而不是继续以地址为主体再反推账户。

### 本阶段允许做的事

- 新增或修改的执行入口显式接收 `account_id`
- fee 归属显式绑定到 `account_id`
- nonce 归属统一到账户主体
- 新增 trace / receipt / audit 字段时优先记录 `account_id`

### 本阶段禁止做的事

- 给 `NovoAccount` 增加 root 字段
- 引入新的统一账户资产账本
- 把隐私子账户混进主体协议主对象

### 退出条件

本阶段只有在以下条件都满足时才允许进入下一阶段：

- 新增执行入口不再扩散 `address-first` 语义
- fee 可稳定追溯到 `account_id`
- nonce 可稳定追溯到 `account_id`
- `account_id` 已成为新增执行能力的默认主体输入

## Phase 3：统一资产视图

### 目标

先让系统“看起来像一个账户”，而不是先让系统“立刻变成一个新账本”。

### 本阶段允许做的事

- 引入 `account_balance(account_id, asset_id)`
- 引入 `account_assets(account_id)`
- 聚合现有执行层、经济层、锁定态、准备金态等资产来源
- 把资产查询结果统一归属到 `account_id`

### 本阶段禁止做的事

- 引入 `asset_root`
- 改造现有经济模块为新的统一账本
- 把统一资产视图对外包装成“统一资产账本已经完成”

### 退出条件

本阶段只有在以下条件都满足时才允许进入下一阶段：

- 主流资产来源都可被统一资产视图稳定聚合
- 查询返回已经稳定以 `account_id` 为主体
- 统一资产视图已经封盘并进入公开资料面

## Phase 4：映射资产主协议

### 目标

把外链资产映射能力收敛成统一主协议，而不是每条链各写一套独立逻辑。

### 本阶段允许做的事

- 冻结 `nETH / nBTC / nUSDT` 等映射资产协议对象
- 冻结 lock / proof / mint / burn / redeem 的主流程
- 冻结映射资产的 custody / risk / audit 边界

### 本阶段禁止做的事

- 在统一资产视图稳定之前冻结映射资产主协议
- 为单条链写绕开主协议的特例路径
- 把映射资产协议与隐私子账户一次性混做

### 退出条件

本阶段只有在以下条件都满足时才允许进入下一阶段：

- 统一资产视图已稳定
- 映射资产的审计和归属都已稳定落到 `account_id`
- 至少一组主流映射资产已按统一协议封盘

## Phase 5：隐私子账户进入主线

### 目标

把隐私能力定义为主账户下的受控子空间，而不是新开一套平行账户体系。

### 本阶段允许做的事

- 主账户到隐私子账户的转入
- 隐私子账户内部转移
- 隐私子账户受控转回主账户
- view policy / audit policy 的最小合同

### 本阶段禁止做的事

- 把隐私空间做成与主账户并列的第二主体体系
- 在 fee / nonce 还未稳定归属到账户主体前引入隐私子账户
- 用隐私子账户替代当前主体协议

### 退出条件

本阶段只有在以下条件都满足时才允许进入下一阶段：

- `account_id` 主体绑定已经稳定
- fee / nonce 已稳定账户化
- 统一资产视图已成立
- 隐私收支可稳定审计、可稳定回到主账户归属

## Phase 6：proof-root 账户树

### 目标

只在运行时确实需要可证明账户结构时，再把统一账户推进到 proof-root 形态。

### 本阶段允许做的事

- 冻结 `identity_root / key_root / asset_root / permission_root` 等结构
- 冻结 root 更新规则
- 冻结 root 证明、同步和消费方合同

### 本阶段禁止做的事

- 在没有真实 root 更新主线前先把 root 写进公开协议
- 在没有明确 proof 消费方前引入 proof-root 账户树
- 用“未来可能需要 zk”作为提前冻结 root 的理由

### 退出条件

本阶段只有在以下条件都满足时才允许进入后续更高版本：

- 账户状态更新语义已经稳定
- root 更新路径已经单一且可验证
- proof 消费方已经明确存在
- root 结构进入主线后不会与现有 snapshot / audit 真相分叉

## 四个关键能力的引入条件

### 1）什么时候可以引入 `asset_root`

只有当以下条件同时满足时才允许引入：

- 统一资产视图已封盘
- 主流资产来源都已稳定账户化
- root 更新路径已清晰
- 确实存在 proof / sync / cross-domain 消费方

### 2）什么时候可以让隐私子账户进入主线

只有当以下条件同时满足时才允许引入：

- `account_id` 主体绑定已稳定
- fee / nonce 已账户化
- 统一资产视图已成立
- 隐私资金进出主账户的审计边界已稳定

### 3）什么时候可以做映射资产主协议

只有当以下条件同时满足时才允许引入：

- 统一资产视图已成立
- 主体归属已稳定落到 `account_id`
- custody / risk / audit 规则已可封盘

### 4）什么时候可以引入 proof 账户树

只有当以下条件同时满足时才允许引入：

- 运行时已存在真实 root 更新需求
- 状态更新语义已稳定
- 证明消费方已经明确
- 引入 root 后不会制造第二套真相

## 当前阶段结论

当前统一账户应被视为：

`已完成 v1-min，已完成 Phase 2，已完成 Phase 3 / Cut 1，已完成 Phase 3 / Cut 2`

补充当前能力分层结论：

- `Cut A / KeyAlgo`：已实现并封盘
- `Cut B / AccountMode`：不是主线必需层，当前默认 `No-Go`
- `Cut C / ExecutionPolicy`：已实现并封盘（最小 execution policy slice）

当前统一账户/资产线的正确状态是：

`统一账户主线已完成主体层、资产视图层、密钥能力层与执行策略层的最小生产闭环；AccountMode / Cut B 保持为非核心可选标签层，默认 No-Go；Phase 4 继续保持触发式治理，当前 No-Go。`

这意味着当前主线已经进入稳定基线：

- 可长期运行
- 不再默认做结构性推进
- 后续工程动作只分为业务接入与触发式扩展治理

对应当前实施门禁见：

- `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-CUTB-TRIGGER-CHECKLIST-2026-04-20.md`
- `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-CUTC-EXECUTIONPOLICY-SEAL-2026-04-20.md`
- `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-CUTC-EXECUTIONPOLICY-IMPLEMENTATION-CHECKLIST-2026-04-20.md`
- `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE2-IMPLEMENTATION-CHECKLIST-2026-04-20.md`
- `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE3-CUT1-SEAL-2026-04-20.md`
- `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE3-CUT2-SEAL-2026-04-20.md`
- `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE3-IMPLEMENTATION-CHECKLIST-2026-04-20.md`
- `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-TRIGGER-CHECKLIST-2026-04-20.md`
- `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-MAPPED-ASSET-CONSTRAINT-DRAFT-2026-04-20.md`
- `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-FAILURE-MODES-2026-04-20.md`

而不是：

- 扩对象
- 扩 root
- 扩统一资产账本
- 扩隐私主实现
- 直接做统一资产账本
- 顺手推进 `Cut B`
- 借 `Cut C` 进入 `Phase 4`

## 建议对外口径

`NOVOVM 统一账户主线已完成主体层、资产视图层、密钥能力层与执行策略层的最小生产闭环；AccountMode / Cut B 保持为非核心可选标签层，默认 No-Go；Phase 4 继续保持触发式治理，当前 No-Go。这条线已进入可长期运行的稳定基线，不需要再做默认结构性推进。`
