# NOVOVM Account Protocol Cut C / ExecutionPolicy 封盘文档（2026-04-20）

Status: FINAL SEAL（Cut C / ExecutionPolicy）  
Scope: 把 `KeyAlgo + ExecutionPolicy` 收成真实执行强约束，但不进入 `AccountMode`、统一资产账本、映射资产协议或隐私资产账本

## 本轮目标

`Cut C` 只做一件事：

`让客户声明的执行策略变成主线唯一 resolve + enforcement 下的不可绕过规则`

本轮不做：

- `AccountMode`
- `Confidential` 完整路径
- 隐私资产账本
- 映射资产协议
- `asset_root`
- 任何自动降级或 silent fallback
- `account_balance / account_assets` 语义变化

## 已成立能力

本轮已成立的能力如下：

- `execution_policy`
  - 已成立最小枚举：
    - `Standard`
    - `PqRequired`
    - `PrivacyRequired`
- 主线唯一 resolve + enforcement
  - 当前唯一 enforcement 位置为：
    - `tx_ingress`
    - `mainline_query`
  - 当前不会在 gateway / adapter 再长第二套 policy 语义
- gateway 只透传
  - gateway 只承载和透传 `execution_policy`
  - gateway 不发明第二套主体或策略决策
- `PqRequired`
  - 已成立强约束：
    - 要求 `key_algo == mldsa87`
    - 不满足时明确拒绝：
      - `ERR_PQ_REQUIRED_BUT_KEY_NOT_PQ`
- `PrivacyRequired`
  - 已成立强约束：
    - 要求隐私路径可用
    - 要求显式走隐私路径
    - 不满足时明确拒绝：
      - `ERR_PRIVACY_REQUIRED_BUT_PATH_NOT_AVAILABLE`
- 明确无 silent fallback
  - 当前 policy 校验失败只会拒绝，不会偷偷降级到普通路径
- 审计可见性
  - `receipt`
  - `trace`
  - `audit`
  当前都可见：
    - `execution_policy`
    - `policy_enforced`
    - `policy_rejection_reason`
- 真实产品语义
  - `KeyAlgo + ExecutionPolicy` 现在会真实决定执行是否允许
  - `account_id` 仍是唯一 canonical subject

这里成立的是：

`统一账户已经具备密钥能力 + 执行强约束的最小闭环`

不是：

`统一账户已经进入账户模式层或隐私资产层`

## 明确未成立能力

本轮明确未成立、也未对外宣称成立的范围如下：

- `AccountMode`
- `Confidential` 完整路径
- 隐私资产账本
- 映射资产协议
- `asset_root`
- 统一资产账本
- 任何自动降级机制
- `account_balance / account_assets` 的写路径或账本语义

特别说明：

- `ExecutionPolicy` 不会重算或分裂 `account_id`
- `ExecutionPolicy` 不会修改资产归属
- `ExecutionPolicy` 不会把 `account_assets / account_balance` 变成写路径
- `ExecutionPolicy` 当前成立的是执行约束，不是新的状态真相源

## 验证结果（2026-04-20 本地实际执行）

本轮基于以下真实执行结果封盘：

- `cargo fmt --all`
- `cargo clippy -p novovm-protocol --all-targets -- -D warnings`
- `cargo clippy -p novovm-adapter-api --all-targets -- -D warnings`
- `cargo clippy -p novovm-node --all-targets -- -D warnings`
- `cargo clippy -p novovm-evm-gateway --all-targets -- -D warnings`
- `cargo test -p novovm-protocol`
- `cargo test -p novovm-adapter-api`
- `cargo test -p novovm-node`
- `cargo test -p novovm-evm-gateway`
- `cargo run -p novovm-node --bin supervm-mainline-gate`
  - 结果：`supervm mainline gate passed`
  - 结果：`L1=100% L2=100% L3=100% L4=100% Overall=100%`

本轮还补了最小回归，直接验证：

- `ed25519 + Standard` 成功
- `mldsa87 + PqRequired` 成功
- `secp256k1 + PqRequired` 拒绝
- `mldsa87 + PrivacyRequired` 且隐私路径可用时成功
- `mldsa87 + PrivacyRequired` 但隐私路径不可用时明确拒绝

## 建议对外口径

`Cut C 已完成：KeyAlgo + ExecutionPolicy 现在会真实决定执行是否允许，且失败路径明确拒绝并可审计。`
