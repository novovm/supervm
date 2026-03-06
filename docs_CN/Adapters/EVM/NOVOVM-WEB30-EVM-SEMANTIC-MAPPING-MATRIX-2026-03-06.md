# NOVOVM WEB30 统一协议与 EVM 协议语义映射矩阵 - 2026-03-06

## 1. 目的

将 `web30_*` 与 `eth_*` 的关系从“原则描述”收敛为可执行矩阵，明确哪些能力可映射、部分映射或不可映射。

## 2. 映射判定等级

| 等级 | 含义 |
|---|---|
| EQ | 语义等价，可直接映射 |
| BR | 可桥接映射，但有语义损失 |
| NA | 不可映射，应保留原协议语义 |

## 3. 核心矩阵

| WEB30 能力 | 是否可映射为 EVM | 映射入口 | 判定 | 语义损失 | 默认路由 | 备注 |
|---|---|---|---|---|---|---|
| 单链转账 | 是 | `eth_sendRawTransaction` | EQ | 低 | Persona + Router | 需 nonce/receipt 对齐 |
| 单链合约调用 | 是 | `eth_call` / tx | BR | 中 | Persona + Core/Plugin | gas/trace 规则差异 |
| 事件日志查询 | 是 | `eth_getLogs` | BR | 中 | Persona + Index | 区块范围/过滤限制差异 |
| 交易回执查询 | 是 | `eth_getTransactionReceipt` | EQ | 低 | Persona | 字段完整性需 gate |
| 多链原子交换 | 否 | `web30_*` only | NA | 高 | SUPERVM Coordinator | 不应伪装成单链 EVM tx |
| 统一账户授权 | 部分可映射 | 兼容层 | BR | 高 | Account Router | 必须引用账户规范 |
| 链外证明/统一审计 | 不建议映射 | native/web30 | NA | 中 | SUPERVM Native | EVM 无同义标准接口 |
| 治理与插件升级 | 不建议映射 | native/web30 | NA | 低 | Governance Layer | 属于平台控制面 |

## 4. 路由规则

1. `EQ`：优先使用 SUPERVM Fast Path（P0）。
2. `BR`：先走 compare gate（P1），达标再默认切换。
3. `NA`：保持 `web30_*` 或 native 路径（P2），不得强行包装进 `eth_*`。

## 5. 不可映射项处理规范

对于 `NA` 能力：

- `eth_*` 接口返回明确错误码和错误文案。
- 响应中不得伪造“已兼容”语义。
- 文档与 SDK 必须提供替代入口（通常是 `web30_*`）。

## 6. 门禁建议

- `semantic_matrix_signal`: 每个能力点映射结果与矩阵一致。
- `na_rejection_signal`: 不可映射项拒绝行为稳定。
- `bridge_loss_signal`: 桥接映射项的语义损失在阈值内。

## 7. 证据产物建议

- `artifacts/migration/evm/semantic_matrix_signal.json`
- `artifacts/migration/evm/na_rejection_signal.json`
- `artifacts/migration/evm/bridge_loss_signal.json`
