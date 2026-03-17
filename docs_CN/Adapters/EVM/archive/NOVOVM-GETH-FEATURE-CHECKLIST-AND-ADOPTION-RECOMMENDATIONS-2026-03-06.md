# NOVOVM go-ethereum 功能清单与迁移分阶段采用建议（全功能镜像视角）- 2026-03-06

> ⚠️ 历史归档说明（2026-03-18）
>  
> 本文用于设计期能力盘点与路线建议，包含阶段性取舍，不是当前执行手册。  
> 当前配置与执行请以 `NOVOVM-EVM-PLUGIN-CONFIG-SETUP-USAGE-2026-03-16.md` 和闭环文档为准。

## 1. 文档目标

本清单用于回答一个工程问题：

- 参考 `go-ethereum`（geth）能力面，`SUPERVM` 在 EVM 插件迁移中哪些能力终局必须覆盖、哪些可阶段后置、哪些可不做。
- 明确“以太坊只是多链插件之一”的定位：后续 `BTC/Solana/...` 可复用同一治理与路由框架，但本文只审计 EVM 参考实现。

约束：

- 仅做审计与方案建议，不修改业务代码。
- 迁移原则：`SUPERVM First`（语义等价优先复用 SUPERVM 原生能力）。
- 进度口径：以生产主线接线与可运行链路为准，不以 gate 数量为准。

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

- `必须引入`：EVM 镜像主线必须具备。
- `适配引入`：建议引入，按 SUPERVM 架构重写/裁剪后纳入。
- `后置引入`：不是首批上线阻塞项，但在终局阶段需要补齐。
- `可不做`：不建议纳入 EVM 镜像主方案（仅特定场景保留）。

| 功能域 | geth 功能项 | 取舍建议 | 原因（SUPERVM 视角） |
|---|---|---|---|
| 入口协议 | HTTP/WS/IPC JSON-RPC | 适配引入 | 对外兼容 EVM 生态需保留 HTTP/WS；IPC 可后置或内网限定。 |
| API 面 | `eth/net/web3` 基础 namespace | 必须引入 | 这是钱包与 SDK 的基础连接面。 |
| API 面 | `txpool` namespace | 适配引入 | 对钱包广播与 pending 观察有价值，但可裁剪只保留核心方法。 |
| API 面 | `admin/debug` 全量能力 | 后置引入 | 生产环境暴露面过大，先保留必要的运维子集。 |
| API 面 | `engine`（auth） | 后置引入 | 首期可后置，但若目标链路需要 EL/CL 兼容，终局需补齐。 |
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
| 网络同步 | devp2p `eth` 协议栈 | 后置引入 | M2 以后应补齐，作为“全功能镜像”终局能力的一部分。 |
| 网络同步 | `snap` 协议与 snap sync | 后置引入 | M2 以后应补齐，支持镜像节点同步能力。 |
| 网络同步 | Full/Snap downloader | 后置引入 | M2 以后应补齐，形成完整同步链路。 |
| 网络同步 | discv5 节点发现 | 后置引入 | M2 以后应补齐，支持镜像节点发现与组网。 |
| 共识 | `consensus.Engine` 兼容接口 | 适配引入 | 不替换 SUPERVM 主共识，但需提供 EVM 侧兼容接口与行为。 |
| 共识 | ethash/clique 运行能力 | 可不做 | 仅在特定历史链/私链场景按需支持。 |
| 挖矿 | miner API | 可不做 | 不是当前主网路线核心能力。 |
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
2. P0 直接复用 SUPERVM；P1 必须有生产对比样例（非 gate 强绑定）后再切换；P2 明确插件兜底。
3. 对外始终遵守 EVM Persona 语义，内部路径可以是 SUPERVM 或插件。

## 6. 建议的首批上线范围（M0）

建议先做 “能稳定对外表现为 EVM 节点” 的最小闭环：

- `eth_*` 核心读写 RPC（含交易发送、回执、日志查询）。
- 交易类型：`Legacy/AccessList/DynamicFee`（M0），`Blob` 读兼容（M0.5），`Blob` 写兼容（M1）。
- 最小 txpool 兼容（pending、替换规则、基础反垃圾策略）。
- 事件过滤与订阅能力。
- 关键错误码与回执字段兼容。
- Type 4（7702）策略显式声明（支持/拒绝/降级，不允许未定义）。
- 安全边界：ABI/caps/hash/registry + 速率限制 + 最小审计证据。

## 7. 首期可后置，但终局需补齐的能力

- `devp2p + eth/snap + downloader + discv5` 网络同步能力（M2 目标）。
- `engine` 相关兼容接口（按目标链与部署模式决定细节）。
- 运维与调试高级能力（trace/debug/admin 子集，受鉴权与限流控制）。

说明：这些能力首期可后置，但若目标是“全功能镜像”，必须有明确补齐计划和阶段出口标准。

## 8. 明确可不做（按需）的能力

- 挖矿与 PoW/PoA 运行路径（ethash/clique/miner）在当前主网路线下非必需。
- 全量高风险运维接口默认开放（应改为最小白名单 + 强鉴权）。

## 9. 对后续 BTC/Solana 插件的复用建议

- 复用本次文档方法：先做“外观语义清单”，再做“P0/P1/P2 路由分类”，最后落生产链路烟测证据。
- 不复用 EVM 专属语义（如 `eth_*`、EVM precompile），只复用治理骨架（registry/ABI/caps/hash/audit）。
- 每条链都应有独立 Persona 文档，避免“EVM 语义外溢”到非 EVM 链。
