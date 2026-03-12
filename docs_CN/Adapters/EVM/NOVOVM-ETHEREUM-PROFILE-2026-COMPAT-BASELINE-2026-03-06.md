# NOVOVM Ethereum Profile 2026 兼容基线（EVM Persona，面向全功能镜像）- 2026-03-06

## 1. 目标

定义 2026 年 EVM Persona 的分阶段兼容基线，用于指导 profile 配置、门禁优先级与上线范围，并服务于 `M3 全功能镜像` 终局。

说明：

- 外部网络升级时间点以以太坊官方公告为准。
- 本文件不固化具体激活日期，只定义必须覆盖的能力面与阶段目标。

## 2. Profile 必填字段

每条 EVM 链 profile 至少包含：

- `chain_id`
- `hardfork_schedule`
- `enabled_tx_types`（0/1/2/3/4）
- `blob_params`
- `precompile_set`
- `fee_model`
- `finality/reorg_policy`
- `rpc_compat_level`
- `unsupported_eips`
- `persona_mode`

## 3. 交易类型兼容基线

| Tx Type | 含义 | 基线要求 | 阶段建议 |
|---|---|---|---|
| 0 | Legacy | 必须兼容 | M0 |
| 1 | AccessList | 必须兼容 | M0 |
| 2 | DynamicFee | 必须兼容 | M0 |
| 3 | Blob（4844） | 至少读兼容；写兼容按阶段开启 | M0.5/M1 |
| 4 | SetCode（7702） | 必须显式声明策略（支持/拒绝/降级） | M1 |

## 4. 分阶段要求

| 阶段 | 目标 | 必须项 |
|---|---|---|
| M0 | Persona 最小可用 | Type 0/1/2、核心 RPC、receipt/log/error 基础兼容 |
| M0.5 | 现代链面读兼容 | Type 3 识别与读取、blob 字段与错误码兼容 |
| M1 | 现代链面写兼容 | Type 3 发送/校验/pool 策略；Type 4 策略落地 |
| M2 | 网络与同步镜像阶段 | txpool/sync/discovery 等网络运行能力分阶段补齐 |
| M3 | 全功能镜像阶段 | 对齐 EVM 全能力面（在 SUPERVM/AOEM 底座上完成 Rust 镜像实现） |

## 5. Type 4（7702）策略模板

每条链必须声明如下策略字段：

- `type4_policy`: `supported | rejected | degraded`
- `type4_error_code`: 不支持时返回的标准错误码
- `type4_compare_gate`: 开启 compare gate 的条件
- `type4_account_constraints`: 与统一账户权限模型的约束

## 6. 门禁矩阵（最低）

| Gate | 目标 |
|---|---|
| `evm_tx_type_signal` | Type 0/1/2/3/4 兼容或拒绝行为一致 |
| `evm_receipt_log_signal` | receipt/log/blob 相关字段语义一致 |
| `evm_error_code_signal` | unsupported/nonce/replacement/intrinsic 错误码一致 |
| `evm_filter_subscribe_signal` | 过滤器与订阅兼容 |
| `evm_reorg_finality_signal` | reorg 下回执与最终性策略一致 |
| `evm_account_behavior_signal` | Type 4 与统一账户规则一致 |

## 7. 与多链插件体系关系

- Ethereum profile 是首个重点 profile，不代表唯一 profile。
- BTC/Solana 等后续 profile 复用治理框架，不复用 EVM 专属语义。
- profile 变更必须走台账、门禁与证据流程。
- 本文件中的 M0/M1/M2 均为阶段口径，不可替代 M3 终局要求。

## 8. 证据产物建议

- `artifacts/migration/evm/profile_compat_baseline_signal.json`
- `artifacts/migration/evm/tx_type_compat_signal.json`
- `artifacts/migration/evm/type4_policy_signal.json`
- `artifacts/migration/evm/full_mirror_gap_signal.json`
