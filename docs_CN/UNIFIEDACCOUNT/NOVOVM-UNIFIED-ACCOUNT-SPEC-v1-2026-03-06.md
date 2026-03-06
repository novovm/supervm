# NOVOVM Unified Account Spec v1（统一账户正式规范）- 2026-03-06

## 1. 规范目标

本规范定义 SUPERVM 统一账户系统的最小生产契约，覆盖：

- 身份模型（UCA）
- 映射模型（Persona Binding）
- 唯一性与冲突规则
- 签名域与 nonce/replay 策略
- 权限、恢复、撤销与审计事件

## 2. 模型定义

### 2.1 UCA（主身份）

| 字段 | 说明 |
|---|---|
| `uca_id` | 全局唯一主身份 ID |
| `primary_key_ref` | 主签名公钥引用 |
| `status` | `active/suspended/recovering/revoked` |
| `created_at` | 创建时间 |
| `updated_at` | 更新时间 |

### 2.2 PersonaBinding（链语义地址绑定）

| 字段 | 说明 |
|---|---|
| `uca_id` | 所属 UCA |
| `persona_type` | `web30/evm/bitcoin/solana/...` |
| `chain_id` | 链 ID |
| `external_address` | 对外地址 |
| `binding_state` | `bound/revoking/revoked` |
| `bound_at` | 绑定时间 |
| `revoked_at` | 撤销时间（可空） |
| `cooldown_until` | 冷却截止时间（可空） |

### 2.3 AccountPolicy（账户策略）

| 字段 | 说明 |
|---|---|
| `signature_domain_policy` | 签名域隔离策略 |
| `nonce_scope` | `persona`（默认）/`chain`/`global` |
| `delegation_policy` | 代理授权策略 |
| `session_key_policy` | 会话密钥策略 |
| `recovery_policy` | 恢复策略 |

## 3. 唯一性与冲突规则（强约束）

1. `1 UCA -> N PersonaBinding` 合法。
2. `1 PersonaAddress(chain_id + persona_type + address)` 在同一时刻只能绑定到 `1 UCA`。
3. 发现冲突绑定时，后写入必须失败并记录冲突事件。
4. 撤销后是否可重绑取决于冷却策略；冷却期内禁止重绑。
5. 禁止“静默覆盖绑定”；任何覆盖必须走显式撤销 + 重绑流程。

## 4. 签名域规范

| 接口/消息类型 | 域 |
|---|---|
| `eth_sign` | `domain=evm:{chain_id}` |
| `personal_sign` | `domain=evm-personal:{chain_id}` |
| `typed_data` | `domain=eip712:{chain_id}:{app}` |
| `web30_sign` | `domain=web30:{network}` |

规则：

- 不同域之间禁止验签互通。
- 同 payload 在不同域必须视为不同消息。

## 5. Nonce / Replay 策略

默认策略：`nonce_scope = persona`。

理由：

1. 避免 `eth_*` 与 `web30_*` 的交易流相互污染。
2. 降低跨 Persona 并发冲突。
3. 更符合外部链开发者预期（按链语义计数）。
4. 便于插件故障隔离与局部回放治理。

## 6. 权限模型

| 角色 | 允许操作 |
|---|---|
| `Owner` | 全权限（绑定/撤销/策略修改/交易发起/恢复） |
| `Delegate` | 可发交易 + 可查账户；不可改主策略 |
| `SessionKey` | 时效/额度/方法白名单内的受限调用 |

规则：

- 所有授权必须可撤销。
- SessionKey 必须带到期时间和范围约束。

## 7. 恢复与撤销

### 7.1 恢复

- 主密钥轮换必须记录 `key_rotated` 事件。
- 恢复流程进入 `recovering` 状态，完成后回到 `active`。

### 7.2 撤销

- 绑定撤销必须记录 `binding_revoked` 事件。
- 撤销后是否允许重绑由 `cooldown_until` 控制。

## 8. 审计事件（最小集）

- `uca_created`
- `binding_added`
- `binding_conflict_rejected`
- `binding_revoked`
- `nonce_replay_rejected`
- `domain_mismatch_rejected`
- `permission_denied`
- `key_rotated`

## 9. 与 EVM / WEB30 边界

1. `eth_*` 只消费映射结果，不主导统一身份。
2. 跨链原子协调不属于 `eth_*` 语义；由 `web30_*` 协调层承担。
3. Type 4（7702）必须显式声明：`supported/rejected/degraded`。

### 9.1 Type 4（7702）策略契约（最小要求）

| 策略项 | 要求 |
|---|---|
| 输入受理 | 明确是否受理 Type 4 输入；不可隐式忽略 |
| 签名校验 | 受理时必须按 Type 4 规则验签 |
| 拒绝语义 | 不支持时返回固定错误码（建议：`ERR_UNSUPPORTED_TX_TYPE_4`） |
| 降级语义 | 降级执行时必须记录 `type4_degraded` 事件并返回降级标记 |
| 混用约束 | Type 4 与 `SessionKey/Delegate` 混用是否允许必须显式声明；默认禁止 |

## 10. 版本治理

- 本规范版本：`v1`。
- 后续变更必须更新版本号并附兼容声明。
