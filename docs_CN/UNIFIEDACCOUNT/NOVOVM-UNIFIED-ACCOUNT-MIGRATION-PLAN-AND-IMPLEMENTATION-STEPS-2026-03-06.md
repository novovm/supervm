# NOVOVM 统一账户迁移方案与实施步骤（SUPERVM）- 2026-03-06

## 1. 文档目标

目标：将 `SVM2026` 统一账户设计资产迁移到 `SUPERVM` 生产架构，形成可上线、可门禁、可审计的统一账户系统。

约束：

- 本阶段只做文档规划，不改业务代码。
- 统一账户是 EVM Persona（`WP-10`）与多链插件体系的前置任务。

---

## 2. 迁移原则

1. 迁模型，不迁实验实现。
2. 账户先行，协议后接：先冻结账户契约，再接 `eth_* / web30_*`。
3. 语义隔离：`eth_*` 保持单链语义；跨链原子由 `web30_*` 协调层承担。
4. `SUPERVM First`：核心身份、映射、权限、重放防护在 SUPERVM Core 实现。

---

## 3. 目标架构

```text
[Protocol Ingress]
  web30_* / eth_* / btc_* / ...
        |
        v
[Account Router]
  协议识别 + Persona识别 + UCA定位 + 校验决策
        |
        v
[UCA Core]
  UCA主身份 + 绑定索引 + 策略引擎 + 审计事件
        |
        v
[Execution Layer]
  SUPERVM fast path / EVM adapter / 其他链 adapter
```

### 3.1 为什么先 Router 再 Execution

- 入口请求先经过 Router，是为了先确定“谁在发请求、以哪种 Persona、是否有权限、nonce 是否可接受”。
- Execution 只处理“如何执行”，不再决定账户身份与权限。
- 这样可避免各链插件各自实现账户逻辑，保证统一账户唯一主控。

### 3.2 Account Router 决策顺序（必须按序执行）

1. 识别协议入口类型：`eth_* / web30_* / 其他`。
2. 识别 Persona 类型：`evm/web30/bitcoin/solana/...`。
3. 解析并校验签名域（domain）。
4. 定位 `UCA` 主身份。
5. 校验绑定合法性（是否存在冲突/是否冷却期）。
6. 校验权限（Owner/Delegate/SessionKey）。
7. 校验 nonce/replay。
8. 路由到 `SUPERVM fast path` 或对应 adapter。
9. 记录账户审计事件（成功/拒绝均记录）。

---

## 4. 数据模型（实施最小集）

本节与 `NOVOVM-UNIFIED-ACCOUNT-SPEC-v1-2026-03-06.md` 一致。

### 4.1 `UCA`

- `uca_id`
- `primary_key_ref`
- `status`
- `created_at`
- `updated_at`

### 4.2 `PersonaBinding`

- `uca_id`
- `persona_type`
- `chain_id`
- `external_address`
- `binding_state`
- `bound_at`
- `revoked_at`
- `cooldown_until`

### 4.3 `AccountPolicy`

- `signature_domain_policy`
- `nonce_scope`
- `delegation_policy`
- `session_key_policy`
- `recovery_policy`

---

## 5. 权限与 nonce 策略（实现口径）

### 5.1 权限分层

- `Owner`：全权限（绑定/撤销/策略修改/恢复/交易）。
- `Delegate`：可交易与查询，不可修改主策略。
- `SessionKey`：受限权限（时效、额度、方法白名单）。

### 5.2 nonce 策略

默认 `nonce_scope = persona`。

原因：

1. 避免 `eth_*` 与 `web30_*` 互相污染。
2. 降低跨 Persona 并发冲突。
3. 保持链语义预期一致。
4. 便于插件故障隔离与回放治理。

---

## 6. 分阶段实施步骤

### Phase U0：基线冻结

1. 冻结术语：`UCA/PersonaBinding/AccountPolicy`。
2. 冻结边界：`eth_*` 单链，跨链原子归 `web30_*`。
3. 完成审计快照与风险登记。

出口标准：文档评审通过。

### Phase U1：账户契约设计

1. 冻结 UCA 与绑定模型。
2. 冻结签名域策略。
3. 冻结 nonce/replay 策略。
4. 冻结权限/恢复/撤销策略。

出口标准：`Spec v1` 冻结。

### Phase U2：路由与接口设计

1. 冻结 Router 决策顺序与接口。
2. 冻结与 `novovm-adapter-api` 的映射接口。
3. 冻结 `eth_* / web30_*` 账户行为差异策略。
4. 冻结 Type 4（7702）策略位。

出口标准：策略表与依赖表可执行。

### Phase U3：门禁矩阵设计

1. 定义 Gate Matrix 用例集合。
2. 补齐负向用例：重放、域串用、越权、错误 nonce、冲突绑定。
3. 固化证据路径与失败阻断级别。

出口标准：`Gate Matrix v1` 冻结。

### Phase U4：EVM/WEB30 收口

1. 与 WP-10/WP-11/WP-13 依赖闭环。
2. 输出支持/拒绝/降级兼容声明。
3. 形成 RC 候选基线。

出口标准：统一账户迁移状态达到 `ReadyForMerge`。

---

## 7. 门禁口径（输入/输出/失败语义）

详细测试矩阵见：
`NOVOVM-UNIFIED-ACCOUNT-GATE-MATRIX-v1-2026-03-06.md`

| Gate | 输入样本 | 预期输出 | 失败级别 |
|---|---|---|---|
| `ua_mapping_signal` | 合法绑定、冲突绑定 | 合法通过；冲突拒绝并记事件 | BlockMerge |
| `ua_signature_domain_signal` | 同 payload 跨域签名 | 跨域验签必须失败 | BlockMerge |
| `ua_nonce_replay_signal` | 重放交易、逆序 nonce | 重放拒绝 | BlockMerge |
| `ua_permission_signal` | Delegate/SessionKey 越权 | 越权拒绝 | BlockMerge |
| `ua_persona_boundary_signal` | `eth_*` 触发跨链原子请求 | 显式拒绝或转 `web30_*` | BlockRelease |
| `ua_type4_policy_signal` | Type 4 交易输入 | 按策略支持/拒绝/降级且错误码固定 | BlockRelease |

---

## 8. 风险与缓解

| 风险 | 描述 | 缓解 |
|---|---|---|
| R-UA-01 | 复用实验实现导致生产语义漂移 | 仅迁设计资产，重构实现 |
| R-UA-02 | 签名域未隔离导致跨协议重放 | 域隔离门禁强制阻断 |
| R-UA-03 | nonce 口径不一致导致冲突 | 固化 Persona 级 nonce |
| R-UA-04 | `eth_*` 污染跨链原子语义 | 原子协调只在 `web30_*` |
| R-UA-05 | 绑定撤销与恢复缺失 | 在 Spec 中冻结恢复/撤销流程 |

---

## 9. 产物清单

1. 审计快照：
   - `NOVOVM-UNIFIED-ACCOUNT-AUDIT-SNAPSHOT-2026-03-06.md`
2. 迁移方案（本文）：
   - `NOVOVM-UNIFIED-ACCOUNT-MIGRATION-PLAN-AND-IMPLEMENTATION-STEPS-2026-03-06.md`
3. 迁移台账：
   - `NOVOVM-UNIFIED-ACCOUNT-MIGRATION-LEDGER-2026-03-06.md`
4. 正式规范：
   - `NOVOVM-UNIFIED-ACCOUNT-SPEC-v1-2026-03-06.md`
5. 门禁矩阵：
   - `NOVOVM-UNIFIED-ACCOUNT-GATE-MATRIX-v1-2026-03-06.md`
