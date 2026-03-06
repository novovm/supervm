# NOVOVM go-ethereum 功能清单与迁移取舍建议（EVM Adapter 视角）- 2026-03-06

## 1. 文档目标

本清单用于回答一个工程问题：

- 参考 `go-ethereum`（geth）能力面，`SUPERVM` 在 EVM 插件迁移中哪些能力必须要、哪些可后置、哪些不应迁移。
- 明确“以太坊只是多链插件之一”的定位：后续 `BTC/Solana/...` 可复用同一治理与路由框架，但本文只审计 EVM 参考实现。

约束：

- 仅做审计与方案建议，不修改业务代码。
- 迁移原则：`SUPERVM First`（语义等价优先复用 SUPERVM 原生能力）。

## 2. 审计基线

- 审计对象：`/Users/aoem-a2/Downloads/dev_project/vsCode/WorksArea/go-ethereum`
- 分支：`master`
- 提交：`a0fb8102fefd`
- 审计日期：`2026-03-06`
- 审计方式：源码与结构审计（未运行节点）

关键证据点（源码）：

- 可执行与 RPC 入口：`README.md:33`, `README.md:166`
- API 命名空间注册：`eth/backend.go:392`, `internal/ethapi/backend.go:106`, `node/api.go:35`
- Engine API（鉴权）：`eth/catalyst/api.go:52`
- 交易类型（含 Blob/SetCode）：`core/types/transaction.go:48`, `core/types/tx_blob.go:281`, `core/types/tx_setcode.go:189`
- 预编译与 fork 选择：`core/vm/contracts.go:56`, `core/vm/contracts.go:214`
- 链配置与 fork 时间线：`params/config.go:43`, `params/config.go:459`, `params/config.go:829`
- 过滤器与订阅：`eth/filters/api.go:95`, `eth/filters/api.go:398`, `eth/filters/api.go:546`
- Tracing：`eth/tracers/api.go:212`, `eth/tracers/api.go:864`, `eth/tracers/api.go:914`
- TxPool（legacy + blob）：`core/txpool/txpool.go:65`, `core/txpool/legacypool/legacypool.go:17`, `core/txpool/blobpool/blobpool.go:179`
- 同步与协议：`eth/downloader/downloader.go:74`, `eth/protocols/eth/handler.go:100`, `eth/protocols/snap/handler.go:89`
- 账户与签名：`accounts/keystore/keystore.go:21`, `cmd/clef/main.go:625`
- GraphQL 与 DB/Snapshot 工具：`cmd/utils/flags.go:770`, `graphql/graphql.go:17`, `cmd/geth/snapshot.go:43`, `cmd/geth/dbcmd.go:76`

## 3. go-ethereum 功能全景（目录级）

| 模块域 | 代表目录 | 作用概述 |
|---|---|---|
| 节点与命令行 | `cmd/geth`, `node`, `rpc` | 节点生命周期、CLI、RPC 暴露 |
| 执行与状态 | `core`, `core/vm`, `params` | EVM 执行、交易类型、fork 规则、状态机 |
| 网络与同步 | `p2p`, `eth/protocols/*`, `eth/downloader` | 对等网络、`eth/snap` 协议、Full/Snap 同步 |
| 交易池 | `core/txpool/*` | legacy/blob 子池、替换策略、DoS 保护 |
| API 与可观测 | `ethapi`, `eth/filters`, `eth/tracers`, `graphql` | JSON-RPC、日志过滤、trace、GraphQL |
| 账户签名 | `accounts`, `signer`, `cmd/clef` | keystore、本地签名、独立签名器 |
| 运维数据工具 | `cmd/geth/snapshot.go`, `cmd/geth/dbcmd.go` | snapshot 校验、数据库检查/导入导出/压缩 |

## 4. 功能清单与取舍建议（面向 SUPERVM）

判定标签：

- `必须引入`：EVM Persona 最小可用面必须具备。
- `适配引入`：建议引入，但按 SUPERVM 架构重写/裁剪。
- `后置引入`：不是首批上线阻塞项，可后续补。
- `不迁移`：不建议纳入 SUPERVM EVM adapter 主方案。

| 功能域 | geth 功能项 | 取舍建议 | 原因（SUPERVM 视角） |
|---|---|---|---|
| 入口协议 | HTTP/WS/IPC JSON-RPC | 适配引入 | 对外兼容 EVM 生态需保留 HTTP/WS；IPC 可后置或内网限定。 |
| API 面 | `eth/net/web3` 基础 namespace | 必须引入 | 这是钱包与 SDK 的基础连接面。 |
| API 面 | `txpool` namespace | 适配引入 | 对钱包广播与 pending 观察有价值，但可裁剪只保留核心方法。 |
| API 面 | `admin/debug` 全量能力 | 后置引入 | 生产环境暴露面过大，先保留必要的运维子集。 |
| API 面 | `engine`（auth） | 后置引入 | 若 SUPERVM EVM Persona 不承担 EL-CL 对接，首期可不暴露。 |
| API 面 | GraphQL | 后置引入 | JSON-RPC 已覆盖主场景，GraphQL 可作为高级读 API。 |
| 执行语义 | Legacy/AccessList/DynamicFee 交易 | 必须引入 | EVM 主流交易面，缺失会导致兼容性断层。 |
| 执行语义 | Blob 交易（EIP-4844） | 适配引入（优先级上调） | 建议 M0.5 完成读兼容、M1 完成写兼容；内部实现优先复用 SUPERVM 能力。 |
| 执行语义 | SetCode（EIP-7702） | 后置引入（需显式策略） | 与账户模型深度耦合；即使暂不支持也必须明确 `支持/拒绝/降级` 与错误码。 |
| 执行语义 | fork/profile 配置体系 | 必须引入 | 必须有 chain profile 才能解释不同 EVM 链差异。 |
| 执行语义 | 预编译集合（含 BLS/KZG/P256） | 适配引入 | 按链 profile 启用，避免一刀切启全量。 |
| 执行语义 | gas/fee 估算语义 | 必须引入 | 直接影响交易可执行性和用户费用体验。 |
| 执行语义 | receipt/log/error 语义兼容 | 必须引入 | 对外“像 EVM 节点”最核心的是返回语义一致。 |
| 过滤订阅 | `eth_newFilter/getLogs/getFilterChanges` | 必须引入 | dApp 索引与事件监听强依赖。 |
| Tracing | `traceTransaction/traceCall` 基础集 | 适配引入 | 生产排障需要，但建议限流与鉴权。 |
| Tracing | `traceBlockFromFile` 等重操作 | 后置引入 | 首期性价比低，资源开销较大。 |
| TxPool | legacy pool + 替换策略 | 必须引入 | 交易入池与替换规则是钱包行为兼容关键。 |
| TxPool | blob 专用池与持久化策略 | 适配引入 | 对 4844 兼容重要，可按业务规模分阶段完善。 |
| TxPool | 全量调参面 | 后置引入 | 首期保留核心参数，避免运维复杂度上升。 |
| 网络同步 | devp2p `eth` 协议栈 | 不迁移（首期） | SUPERVM EVM 插件目标是“节点语义兼容”，非“复制一个 geth 客户端网络层”。 |
| 网络同步 | `snap` 协议与 snap sync | 不迁移（首期） | 若不作为以太坊独立全节点参与同步，可不实现。 |
| 网络同步 | Full/Snap downloader | 不迁移（首期） | 与上同，优先做 RPC/语义适配与内部执行复用。 |
| 网络同步 | discv5 节点发现 | 不迁移（首期） | 对 Persona 模式非刚需。 |
| 共识 | `consensus.Engine` 全栈 | 不迁移（首期） | SUPERVM 已有生产级共识主路径，不应被插件反向侵入。 |
| 共识 | ethash/clique 运行能力 | 不迁移 | 历史/兼容意义大于生产价值，且与目标架构不匹配。 |
| 挖矿 | miner API | 不迁移（首期） | EVM 插件非矿工客户端定位。 |
| 账户 | keystore 账户管理 | 适配引入 | 可作为兼容层，但要对接 SUPERVM 全链唯一账户体系。 |
| 账户 | clef 独立签名器 | 后置引入 | 企业级安全场景有价值，但不是迁移第一阶段阻塞项。 |
| 运维工具 | snapshot prune/verify 系列 | 后置引入 | 对链内核运维有价值，但与插件首发能力弱相关。 |
| 运维工具 | db inspect/compact/import/export | 后置引入 | 属于客户端深运维能力，可按运维需求再接入。 |

## 5. 与 SUPERVM 重叠能力的路由建议（P0/P1/P2）

| 分级 | 通俗定义 | 默认路线 | 例子 |
|---|---|---|---|
| P0 | 这件事 SUPERVM 已经会做，而且做得更快更稳 | 直接走 SUPERVM | 限流、审计、状态读缓存、索引查询、基础入池校验 |
| P1 | SUPERVM 大体能做，但 EVM 有边角规则差异 | 双跑比对后切 SUPERVM | gas 估算细节、nonce 边界、receipt 字段映射 |
| P2 | 目标链特有规则，SUPERVM 无等价实现 | 固定走链插件 | 特定 fork 规则、链专属 precompile、reorg/finality 细节 |

执行建议：

1. 先分类（P0/P1/P2），再编码，不要边做边拍脑袋路由。
2. P0 直接复用 SUPERVM；P1 必须有 compare gate 证据后再切换；P2 明确插件兜底。
3. 对外始终遵守 EVM Persona 语义，内部路径可以是 SUPERVM 或插件。

## 6. 建议的首批上线范围（M0）

建议先做 “能稳定对外表现为 EVM 节点” 的最小闭环：

- `eth_*` 核心读写 RPC（含交易发送、回执、日志查询）。
- 交易类型：`Legacy/AccessList/DynamicFee`（M0），`Blob` 读兼容（M0.5），`Blob` 写兼容（M1）。
- 最小 txpool 兼容（pending、替换规则、基础反垃圾策略）。
- 事件过滤与订阅能力。
- 关键错误码与回执字段兼容。
- Type 4（7702）策略显式声明（支持/拒绝/降级，不允许未定义）。
- 安全门禁：ABI/caps/hash/registry + 速率限制 + 审计证据。

## 7. 明确不建议首期迁移的 geth 能力

- 完整 `devp2p + eth/snap + downloader` 网络同步栈。
- 挖矿与 PoW/PoA 运行路径（ethash/clique）。
- 全量 `admin/debug` 高风险接口默认开放。
- 全量 snapshot/db 深运维工具立即产品化。

理由：这些能力会显著增加复杂度与攻击面，但对当前“EVM 插件语义兼容 + SUPERVM 内核复用”的目标贡献有限。

## 8. 对后续 BTC/Solana 插件的复用建议

- 复用本次文档方法：先做“外观语义清单”，再做“P0/P1/P2 路由分类”，最后落 gate 证据。
- 不复用 EVM 专属语义（如 `eth_*`、EVM precompile），只复用治理骨架（registry/ABI/caps/hash/audit）。
- 每条链都应有独立 Persona 文档，避免“EVM 语义外溢”到非 EVM 链。
