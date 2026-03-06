# NOVOVM 多链原子交易协调层规范（AOL）- 2026-03-06

## 1. 目的

定义多链原子交易能力在 SUPERVM 架构中的归属、流程与边界，避免将原子协调语义污染到 EVM Persona。

## 2. 关键结论

1. 多链原子交易属于 `web30_*` / SUPERVM-native 协议层能力。
2. `eth_*` 接口默认只提供单链 EVM 语义，不直接暴露跨链原子协议语义。
3. EVM adapter 是执行适配器，不是原子协调器本体。

## 3. 原子语义等级

| 等级 | 定义 | 适用场景 |
|---|---|---|
| A0 | 单链事务（非原子跨链） | `eth_*` 默认路径 |
| A1 | 补偿型原子（Saga） | 跨链业务、允许补偿 |
| A2 | 两阶段提交式原子（2PC-like） | 高一致性但成本高 |
| A3 | 强原子（严格） | 仅在明确可证明条件下启用 |

默认：先落地 `A1`，逐步评估 `A2/A3`。

## 4. 组件职责

| 组件 | 职责 |
|---|---|
| Atomic Coordinator | 编排多链步骤、锁定/提交/补偿决策 |
| Capability Router | 将执行步骤分发到 SUPERVM fast path 或链插件 |
| Chain Adapter（含 EVM） | 执行链内动作并返回可验证结果 |
| Audit/Gate Layer | 记录状态机迁移与失败补偿证据 |

## 5. 协议入口约束

| 入口 | 是否可触发跨链原子 | 说明 |
|---|---|---|
| `web30_*` | 是 | 标准入口，带编排上下文 |
| `eth_*` | 否（默认） | 保持 EVM 单链语义纯净 |
| `eth_*` + 显式桥接标记 | 有条件 | 必须转入 AOL 并回写桥接证据 |

## 6. 状态机（建议）

`Created -> Locked -> Prepared -> Committed`  
失败路径：`Prepared/Committing -> Compensating -> Compensated/Failed`

要求：

- 每次状态迁移必须产生日志与唯一事务 ID。
- 任何链执行失败必须可追溯到补偿策略。

## 7. 失败与回滚策略

1. 单链失败：立即中断并触发补偿。
2. 超时：进入 `Compensating`，不可静默丢弃。
3. 部分提交：必须写入异常审计并触发人工/自动补偿通道。

## 8. 与账户系统联动

- 原子事务主身份为 `UCA`（统一账户），不是某个 Persona 地址。
- 授权与签名校验由账户路由器统一判定。
- 会话密钥必须显式声明可参与的原子事务范围。

## 9. AOL 门禁最小集

- `atomic_boundary_signal`: `eth_*` 未越权触发原子编排。
- `atomic_state_machine_signal`: 状态机迁移完整无缺口。
- `atomic_compensation_signal`: 失败补偿可执行且可审计。
- `atomic_idempotency_signal`: 重试不导致重复提交。

## 10. 证据产物建议

- `artifacts/migration/evm/atomic_boundary_signal.json`
- `artifacts/migration/evm/atomic_state_machine_signal.json`
- `artifacts/migration/evm/atomic_compensation_signal.json`
