# NOVOVM 全链唯一账户体系与 EVM Persona 映射规范 - 2026-03-06

## 1. 目的与范围

目的：定义 `SUPERVM` 全链唯一账户体系在 `eth_*` Persona 下的账户映射、签名域、nonce 与权限边界，避免迁移阶段出现隐式语义分叉。

范围：

- 适用于 `SUPERVM` 的 EVM adapter/persona。
- 不覆盖 BTC/Solana 等非 EVM 链的具体账户细节（仅复用治理框架）。

## 2. 术语与对象

| 术语 | 定义 |
|---|---|
| UCA（Unified Chain Account） | SUPERVM 全链唯一账户主身份 |
| IAI（Internal Account Identity） | UCA 在内核执行层的内部身份句柄 |
| EVM Persona Address | 在 `eth_*` 接口下暴露给钱包/dApp 的 EVM 地址视图 |
| Persona Session Key | 受限时效/权限的会话密钥（可选） |

## 3. 映射原则

1. `UCA` 是唯一主身份；`EVM Persona Address` 是视图，不是独立主身份。
2. 默认关系为 `1 UCA -> N Persona Address`（按链 profile / 地址策略可配置）。
3. 所有映射关系必须可审计、可撤销、可回放验证（事件与配置双留痕）。
4. 禁止插件私自生成“脱离 UCA 治理”的地址体系。

## 4. 地址与可逆性策略

| 项目 | 规则 |
|---|---|
| 地址生成 | 由统一账户路由器按 profile 规则生成/绑定 |
| 可逆性 | 必须支持 `Persona Address -> UCA` 可追溯查询 |
| 暴露边界 | 对外仅暴露 Persona 地址，不暴露 IAI |
| 迁移兼容 | 历史外部地址可导入，但导入后必须绑定 UCA |

## 5. 签名域隔离

签名域必须显式隔离，避免跨协议重放：

| 签名接口 | 域定义 | 重放策略 |
|---|---|---|
| `eth_sign` | EVM Persona 域 | 仅允许在该 Persona 生效 |
| `personal_sign` | EVM Persona 域 | 必须包含域前缀与链上下文 |
| EIP-712 typed data | EVM Persona + chain profile 域 | 跨链/跨 Persona 禁止重放 |
| `web30_*` 原生签名 | WEB30 主链域 | 不与 `eth_*` 共享重放空间 |

## 6. Nonce 与授权边界

| 项目 | 规范 |
|---|---|
| nonce 主体 | 默认为 Persona 级 nonce，不使用全局共享 nonce |
| 冲突处理 | 同一 UCA 的不同 Persona nonce 空间隔离 |
| 授权撤销 | 撤销事件必须同步影响会话密钥与代理权限 |
| 代理调用 | 必须声明代理链路与最大权限范围 |

## 7. 7702（Type 4）策略位

本规范要求必须显式声明状态，不允许“未定义默认行为”：

- `supported`: 支持 Type 4，进入完整 gate。
- `rejected`: 显式拒绝，返回标准化错误码与文案。
- `degraded`: 受限支持，需声明降级范围。

当前建议：迁移初期可 `rejected` 或 `degraded`，但必须在 profile 与 gate 中一致。

## 8. 统一账户与多链原子交易关系

1. 原子交易主账户主体是 `UCA`，不是单个 Persona 地址。
2. `eth_*` 单链请求默认不直接触发跨链原子语义。
3. 若通过桥接触发原子流程，必须进入 `web30_*` 协调层并产生日志证据。

## 9. 必测门禁（Account Behavior Gate）

- EOA 与 contract 账户行为一致性。
- Persona 地址映射追溯正确性。
- 签名域隔离与反重放。
- nonce 隔离与回放保护。
- Type 4 策略一致性（支持/拒绝/降级）。

## 10. 证据产物建议

- `artifacts/migration/evm/account_mapping_signal.json`
- `artifacts/migration/evm/signature_domain_signal.json`
- `artifacts/migration/evm/nonce_isolation_signal.json`
- `artifacts/migration/evm/type4_policy_signal.json`
