# NOVOVM Account Protocol Cut A / KeyAlgo 封盘文档（2026-04-20）

Status: FINAL SEAL（Cut A / KeyAlgo）  
Scope: 统一账户补齐 `KeyAlgo` 元数据、持有证明校验与审计可见性，但不进入 `AccountMode`、`ExecutionPolicy` 或任何资产层改动

## 本轮目标

`Cut A` 只做一件事：

`让统一账户知道绑定了什么密钥算法，并在创建/轮换时完成声明 -> 校验 -> 绑定闭环`

本轮不做：

- `AccountMode`
- `ExecutionPolicy`
- 多算法主密钥迁移状态机
- 隐私账户能力
- 统一账户层“默认抗量子”
- 任何资产层变化

## 已成立能力

本轮已成立的能力如下：

- `primary_key_binding`
  - 统一账户已具备显式主密钥绑定元数据
- `UcaKeyAlgo`
  - 已成立最小算法集合：
    - `secp256k1`
    - `ed25519`
    - `mldsa87`
- `UcaKeyProofType`
  - 已成立最小证明类型：
    - `signature_v1`
- `UcaPrimaryKeyBinding`
  - 已成立最小绑定对象：
    - `key_algo`
    - `public_key`
    - `proof_type`
    - `proof_payload`
    - `verified_at`
- `ua_createUca`
  - 已接入 `声明 -> 校验 -> 绑定`
- `ua_rotatePrimaryKey`
  - 已接入 `声明 -> 校验 -> 绑定`
- 审计与查询
  - `ua_getAccount` 可见 `primary_key_binding`
  - 审计事件可见 `key_algo`
- 主体保持不变
  - `account_id` 仍是唯一 canonical subject
  - `key_algo` 不会重算或分裂 `account_id`

这里成立的是：

`统一账户已经具备密钥算法元数据 + 持有证明校验 + 审计可见性`

不是：

`统一账户已经进入账户模式层或执行策略层`

## 明确未成立能力

本轮明确未成立、也未对外宣称成立的范围如下：

- `AccountMode`
- `ExecutionPolicy`
- 多算法主密钥迁移状态机
- 隐私账户能力
- 统一账户层“默认抗量子”
- 任何资产层变化
- `account_balance / account_assets` 语义变化

特别说明：

- `mldsa87` 不是“假支持”
- 当前成立的是：
  - 当 AOEM 校验能力可用时，`mldsa87` 证明可真实通过
  - 当 AOEM 校验能力不可用时，请求会被明确拒绝
- 当前没有把隐私语义挂在 `key_algo` 上

## 当前主线路径

当前 `Cut A` 的真实主线路径为：

`novovm-node (bin) -> mainline_query -> unified_account_surface -> UnifiedAccountRouter`

当前真实方法合同：

| 方法 | 当前语义 | 状态 |
| --- | --- | --- |
| `ua_createUca` | 可选接收 `key_algo + public_key + proof_type + proof_payload`，校验后再绑定到账户 | 已封盘（Cut A） |
| `ua_rotatePrimaryKey` | 可选接收 `key_algo + public_key + proof_type + proof_payload`，校验后再完成主密钥轮换 | 已封盘（Cut A） |
| `ua_getAccount` | 返回 `primary_key_binding` 与 `key_algo` 可见性 | 已封盘（Cut A） |
| `ua_getAuditEvents` | 返回 `key_algo` 审计可见性 | 已封盘（Cut A） |

## 验证结果（2026-04-20 本地实际执行）

本轮基于以下真实执行结果封盘：

- `cargo fmt --all`
- `cargo test -p novovm-adapter-api`
- `cargo test -p novovm-node`
- `cargo clippy -p novovm-node --all-targets -- -D warnings`
- `powershell -ExecutionPolicy Bypass -File scripts/migration/run_unified_account_gate.ps1`
  - 结果：`16/16` 全通过
- `cargo run -p novovm-node --bin supervm-mainline-gate`
  - 结果：`supervm mainline gate passed`
  - 结果：`L1=100% L2=100% L3=100% L4=100% Overall=100%`

本轮还补了最小回归，直接验证：

- `ed25519` key binding 成功
- `secp256k1` key binding 成功
- 无效 proof 被拒绝，且不会污染账户主体状态
- `mldsa87` key rotation 在 AOEM 可用时可真实通过

## 建议对外口径

`Cut A 已完成：统一账户已经具备“密钥算法元数据 + 持有证明校验 + 审计可见性”的最小闭环，但尚未进入账户模式或执行策略层。`
