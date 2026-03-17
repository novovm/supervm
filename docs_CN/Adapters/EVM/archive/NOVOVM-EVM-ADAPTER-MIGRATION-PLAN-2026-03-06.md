# NOVOVM EVM/Adapter 迁移方案与实施步骤（SUPERVM）- 生产主线版（2026-03-11）

> ⚠️ 历史归档说明（2026-03-18）
>  
> 本文是迁移阶段计划文档，包含阶段性任务拆解与当时状态。  
> 对外发布和当前执行请以 `README.md` 的“当前有效文档”与闭环证据脚本为准。

## 1. 目标与边界

- 终局目标：`EVM Rust 全功能镜像节点`（不是兼容层终局）。
- 源规则基线：`D:\WEB3_AI\go-ethereum`。
- 并存架构前提：`SUPERVM 主网` 与 `EVM 全镜像节点` 必须同时存在，不是二选一。
- 寄宿关系：EVM 以插件/镜像域寄宿在 `SUPERVM` 内部，对外呈现以太坊节点语义；`SUPERVM` 主链职责与 EVM 镜像职责并行存在。
- 架构边界：
  - 外部：允许 `HTTP/JSON-RPC`。
  - 内部：固定二进制流水线（`opsw1 -> novovm-node -> AOEM`）。
- 迁移原则：
  - 以“规则直迁/协议直迁”为主，尽量不做二次抽象和工程化包装。
  - 仅在与 `SUPERVM/AOEM` 高性能底座对接时做必要改造。
  - `EVM 功能闭环` 与 `Ethereum mainnet live attach` 分开判定；前者完成不等于后者自动完成。

## 2. 核心原则（性能优先）

1. 不改主入口去适配插件，插件必须适配主入口和二进制内核。
2. D1/D2/D3/AOEM 内部不引入 RPC/HTTP/JSON 二次传输。
3. 非必要观测、门禁脚本、包装层不进入生产主线。
4. 进度判定只看生产代码接线与可复现实跑闭环。

## 3. 实施策略（按 go-ethereum 能力面拆解）

## 3.1 协议与执行面（先做）

- 交易类型与签名规则：按 go-ethereum 行为迁移。
- EVM 执行语义与状态变更：按链规则对齐。
- precompile/gas/错误码：按链配置与 fork 规则迁移。
- 结果要求：插件对外呈现与目标链一致的执行行为。

## 3.2 交易池与打包面

- txpool 入池、替换、排序、淘汰规则迁移。
- nonce、费用与冲突处理按以太坊语义实现。
- 结果要求：本地交易生命周期与以太坊节点一致。

## 3.3 网络与同步面

- P2P/discovery/sync 逐步迁移到 Rust 插件侧。
- 区块/交易传播、同步状态机按上游规则实现。
- 结果要求：节点具备真实镜像节点能力，而非仅 RPC 兼容。

## 3.4 查询与接口面

- `eth_*` 常用读写接口按上游语义补齐。
- 边界层保持 JSON-RPC 兼容，内部不改变二进制主线。
- 结果要求：钱包/SDK 可直接使用，且不牺牲内部性能。

## 3.5 节点收益与原子能力面

- 收益归集 -> 换汇 -> 发放：形成最小可运行闭环。
- 原子跨链 intent：本地检查通过后再广播。
- 结果要求：交易收入可对账，原子流程可控可审计。

## 3.6 高性能流水线改造面（不改协议/共识）

- 插件执行路径：采用“并发预处理 + 串行状态提交”。
- 插件状态根：采用“脏标记 + 按需刷新缓存”，避免批内每笔全量重算状态根。
- 插件运行态：按链/发送方分片锁，降低全局锁竞争。
- gateway 边界：请求处理并发化，保持对外 JSON-RPC 语义不变。
- network 传输：减少阻塞重试与全局状态锁，保持同步口径同源。
- AOEM/GPU：仅用于可并行且不影响共识判定的环节（哈希/签名校验/批量过滤）；当前已把 `ed25519 single/batch verify` 直连进插件热路径，并把 `secp256k1 recover` 直连进 `eth_sendRawTransaction` 的 raw sender 恢复主线，以及 `eth_sendTransaction` 的 recoverable signature sender 校验主线。

## 4. 当前工作包（生产口径）

| WP | 名称 | 状态 | 代码锚点 | 下一步 |
|---|---|---|---|---|
| WP-01 | EVM 外部入口归一化与主线消费 | Done | `crates/gateways/evm-gateway/src/main.rs` + `crates/novovm-node/src/bin/novovm-node.rs` | 主线已收口，后续仅随主网联调做必要语义对拍。 |
| WP-02 | 插件执行主路径（apply_v2/self-guard） | Done | `crates/plugins/evm/plugin/src/lib.rs` | 主线已收口，后续仅做性能与边界回归。 |
| WP-03 | 内存 ingress 队列与策略数据面 | Done | `crates/plugins/evm/plugin/src/lib.rs` | txpool/ingress 生产语义已收口，后续仅做实网联调校准。 |
| WP-04 | 收益归集/换汇/发放闭环 | Done | `crates/plugins/evm/plugin/src/lib.rs` | 生产闭环已接通，后续仅保留实单核验。 |
| WP-05 | 原子 intent 门控后广播 | Done | `crates/plugins/evm/plugin/src/lib.rs` | 门控与补偿主线已收口，后续仅做实网压测与参数定标。 |
| WP-06 | go-ethereum 能力直迁（网络/同步/txpool） | Done | `D:\WEB3_AI\go-ethereum` 对照实现 | 生产主线能力已收口，后续仅保留与上游实现的差异回归与边界对拍。 |
| WP-07 | 高性能流水线并发改造（专项） | Done | `crates/plugins/evm/plugin` + `crates/gateways/evm-gateway` + `crates/novovm-network` | 插件并发预处理、txpool 分片、`DashSet` 并发验签缓存、AOEM `ed25519 single/batch verify` 热路径直连已落地；gateway 已补 `AOEM secp256k1 recover`，`eth_sendRawTransaction` 现在优先从 raw 原文恢复 sender，显式 `from` 改为一致性约束/兼容回退；`eth_sendTransaction` 若携带 recoverable `signature/raw_signature/signed_tx`，现也会按同一 AOEM `secp256k1 recover` 主线校验 sender 与 `from` 一致性，并对 `nonce/to/value/gas/fee/data/accessList/blob` 等有效字段执行硬一致校验；其余 gateway 已落地 `public-broadcast status / receiptBatch / txByHashBatch / replayPublicBroadcastBatch / lifecycleBatch / logsBatch / filterLogsBatch / filterChangesBatch / upstreamTxStatusBundle / upstreamEventBundle / upstreamFullBundle / upstreamConsumerBundle / publicSend*Batch` 收口（含 bundle helper 直连去递归分发层），并完成 native sync-pull peer 选路“单次快照发送 + fanout 小快照优先 + 失败回退”、tracker 原地更新、phase 自适应重发、同步期短 tick、public-broadcast 热路径去 discovery 附带发送、按 fanout 同窗分段并发拉取（多窗口并发抓取）以及并发参数可调（`FANOUT_MAX/SEGMENTS_MAX/SEGMENT_MIN_BLOCKS`，支持 phase 后缀 `_HEADERS/_BODIES/_STATE/_FINALIZE`，支持 `_CHAIN_0x{id}` 大小写，并新增 `RESEND_MS` 同源链级+phase 覆盖与 `50..60000ms` 钳制；runtime peer-head 选路预算按本轮 fanout 收口）；network 已落地非阻塞重试 + runtime 观测/更新路径直连 reconcile 去重复锁 + StateSync/ShardState 单次进锁更新 + 收包去双调用 + 无变化快返 + sync-pull 目标表 `DashMap` 化 + followup 出站 track 去双写 + UDP/TCP 回包续拉 `send_internal` 去 clone + sync-pull 回包区间流式组装 + stale 到期短路 + peer heads Top-K 选路 + dirty/stale 到期触发重算短路 + UDP/TCP 收包缓冲区复用 + UDP 发送首包注册化 + local-head/native-snapshot 无变化快返 + pull-target 单次读取 + src->peer 单次遍历 + try_recv 懒反查 + 非 sync 报文跳过 header 解码 + 消息来源 id 单次解析复用 + sync window phase 预取触发 + UDP/TCP 收包短锁（锁外解码/读包）；下一步仅保留 AOEM/GPU 参数定标与压测复核。 |
| WP-08 | Ethereum mainnet live attach（状态源+广播+实网校验） | InProgress | `crates/gateways/evm-gateway` + `crates/novovm-network` + `scripts/migration/run_evm_mainnet_write_canary.ps1` + `scripts/migration/run_evm_mainnet_connectivity_canary.ps1` + `scripts/migration/run_evm_mainnet_funded_write_canary.ps1` | 这是独立于 EVM 功能闭环的新工作线：把当前“主网语义壳 + 内部执行主线”升级为“真实主网状态 + 真实主网广播 + 最小实网一致性验证”的可部署 mainnet RPC；当前读路径与广播出口代码已落地并完成最小读侧实网对拍：`evm-gateway` 核心只读接口可通过 `NOVOVM_GATEWAY_ETH_UPSTREAM_RPC`（支持 `_CHAIN_{id}` / `_CHAIN_0x{id}`）优先直连真实 Ethereum 上游 RPC，并在失败或 `null` 时回退本地视图，已覆盖 `eth_blockNumber/eth_syncing/eth_gasPrice/eth_maxPriorityFeePerGas/eth_feeHistory/eth_getBalance/eth_getBlockByNumber/eth_getBlockByHash/eth_getTransactionByBlockNumberAndIndex/eth_getTransactionByBlockHashAndIndex/eth_getBlockTransactionCountByNumber/eth_getBlockTransactionCountByHash/eth_getBlockReceipts/eth_getUncleCountByBlockNumber/eth_getUncleCountByBlockHash/eth_getLogs/eth_call/eth_estimateGas/eth_getCode/eth_getStorageAt/eth_getProof/eth_getTransactionCount/eth_getTransactionByHash/eth_getTransactionReceipt`；同时 `eth_sendRawTransaction` 在未配置外部执行器时，已可通过 `NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_UPSTREAM_RPC`（未显式配置时回退复用 `NOVOVM_GATEWAY_ETH_UPSTREAM_RPC`）直接调用上游 `eth_sendRawTransaction` 做真实广播，并在失败后再回退 native transport。当前写侧已补“一键 canary 执行脚本”（真实 raw 广播 + receipt 轮询 + summary 落盘）；链路可达 canary 已固化并实测通过（`artifacts/migration/evm-mainnet-connectivity-canary-summary.json`）；新增 funded 一键脚本可本机私钥自动签名 type2 交易后直连写侧主线。只剩“有余额账户实单上链并拿到 receipt”最后验证，完成前仍不得把 `chainId=1` 视为已完全接入主网。 |

## 5. 完成定义（不再工程化）

以下条件同时成立才算“完成”：

1. 生产代码已接线到主路径。
2. 能在本地/集群复现真实运行闭环。
3. 行为与目标链规则一致（以 go-ethereum 为基线）。
4. 不依赖额外 gate 脚本或临时观测层才能成立。

补充口径：

1. 本文档中的“完成”优先指 `EVM 功能闭环 / 全镜像语义闭环`。
2. `Ethereum mainnet live attach` 需要单独完成 `真实状态源 + 真实广播 + 实网校验` 三项，不随功能闭环自动成立。

## 6. 非目标（本阶段）

- 不把脚本产物通过当作主完成标准。
- 不在 `novovm-node` 内恢复多入口/多模式分叉。
- 不为“看起来完整”而增加高开销中间层。

## 7. 文档关系

- 进度台账：`NOVOVM-EVM-ADAPTER-MIGRATION-LEDGER-2026-03-06.md`
- 全功能镜像规范：`NOVOVM-EVM-FULL-MIRROR-NODE-MODE-SPEC-2026-03-11.md`
- 100% 收口清单：`NOVOVM-EVM-FULL-MIRROR-100P-CLOSURE-CHECKLIST-2026-03-13.md`
- 上游缺失能力对照：`NOVOVM-EVM-UPSTREAM-REQUIRED-CAPABILITY-CHECKLIST-2026-03-11.md`
- 边界铁律：`NOVOVM-EVM-PLUGIN-BOUNDARY-IRON-LAWS-2026-03-13.md`
