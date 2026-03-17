# NOVOVM EVM/Adapter 迁移进度台账（SUPERVM）- 生产主线版（2026-03-12）

> ⚠️ 历史归档说明（2026-03-18）
>  
> 本文用于迁移过程追溯，包含阶段性“完成度/百分比/当时状态”口径，**不作为当前开源发布判定依据**。  
> 当前请以 `README.md` 中“当前有效文档”与 `NOVOVM-EVM-FULL-LIFECYCLE-CLOSURE-TARGET-2026-03-17.md` 为准。

## 1. 执行口径（去工程化）

- 迁移完成度只看生产代码是否接线，不再以 `gate/signal/snapshot/rc` 数量判定。
- 对外可以 `HTTP/JSON-RPC`；对内必须 `ops_wire_v1/.opsw1 -> novovm-node -> AOEM` 二进制流水线。
- EVM 终局目标是 `Rust 全功能镜像节点`，不是“兼容层”或“脚本通过”。
- `SUPERVM 主网` 与 `EVM 全镜像` 必须并存：EVM 是寄宿在 superVM 内的镜像域，不替代 `SUPERVM` 主网职责。
- `EVM 功能闭环` 与 `Ethereum mainnet live attach` 必须分开记账；前者完成不等于后者自动完成。
- 铁律-1：`gateway` 只作为外部边界组件，不得成为 superVM 内部层间通信依赖；内部必须保持原生/二进制直连。
- 铁律-2：除非明确必须，禁止新增工程化包装层；旧有工程化内容如影响主线认知与性能，必须持续清理。
- 铁律-3：性能优先与极致静默。默认不输出可观测噪音日志；仅在显式开关下输出 `warn/summary`（当前 gateway 开关：`NOVOVM_GATEWAY_WARN_LOG`、`NOVOVM_GATEWAY_SUMMARY_LOG`，默认关闭）。
- 铁律-4：`novovm-node` 等公共入口/公共方法的修改，必须先经负责人手工同意；未获同意不得改动。
- 铁律-5：EVM 目录边界固定为 `crates/gateways/evm-gateway`、`crates/plugins/evm/core`、`crates/plugins/evm/plugin`；历史混合目录 `crates/novovm-edge-gateway` 已下线，不得回退。
- 铁律文档：`docs_CN/Adapters/EVM/NOVOVM-EVM-PLUGIN-BOUNDARY-IRON-LAWS-2026-03-13.md`。

## 2. 当前主线能力状态（只看生产路径）

- 总体进度（EVM 生产主线）：`100%`
- 当前阶段进度（P07 全功能镜像主线）：`100%`
- 本轮收口进度（一次性完工收口）：`100%`
- 独立进度（Ethereum mainnet live attach）：`97%`

说明：

- 上述 `100%` 只代表 `EVM 插件/gateway/语义/生命周期/内部二进制主线` 已收口。
- 它不代表当前默认部署已经具备“真实 Ethereum mainnet 状态 + 真实 mainnet 广播 + 实网一致性验证”。
- 原生协议兼容专项进度请单独查看：`docs_CN/Adapters/EVM/archive/NOVOVM-EVM-NATIVE-PROTOCOL-COMPAT-PROGRESS-2026-03-16.md`。

| ID | 能力 | 当前状态 | 生产代码锚点 | 说明 |
|---|---|---|---|---|
| EVM-P01 | EVM 外部入口归一化（raw+non-raw） | Done | `crates/gateways/evm-gateway/src/main.rs` | `eth_sendRawTransaction`、`eth_sendTransaction`、`web30_sendTransaction` 已接到统一编码与主线消费路径；gateway 已接入插件 txpool 接收摘要判定，若被丢弃则直接失败并阻止 `.opsw1` 继续入主线，并按插件摘要 `reason/reasons`（统一枚举）返回稳定 JSON-RPC 码（underpriced/nonce-too-low/nonce-gap/capacity）与 geth 风格错误文案，同时在 `error.data` 返回结构化拒绝原因与计数。`eth_sendRawTransaction` + `eth_sendTransaction` 现共用可配置公网广播执行主路径（`NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_EXEC`）；执行器请求统一携带 `tx_ir_bincode`（`raw` 路径同时附带 `raw_tx`），配置后广播失败即直接拒绝写入并返回稳定错误语义（`public broadcast failed`）。 |
| EVM-P02 | D1 统一二进制入口（唯一生产 bin） | Done | `crates/novovm-node/src/bin/novovm-node.rs` | 仅保留 `novovm-node` 生产入口，消费 `.opsw1` 并对接 AOEM。 |
| EVM-P03 | EVM 插件执行主路径（apply_v2 + self-guard） | Done | `crates/plugins/evm/plugin/src/lib.rs` | 插件执行与 guard 主路径已在生产代码中。 |
| EVM-P04 | 内存 ingress 队列（插件侧） | Done | `crates/plugins/evm/plugin/src/lib.rs` | 已落地 txpool 最小语义：同 `sender+nonce` 的 price-bump 替换（默认 10%，可配）+ per-sender pending 上限（默认 64，可配）+ nonce gap 丢弃（默认 1024，可配）+ `pending -> executable` 连续 nonce 提升队列；并补齐显式 `executable drain`、`pending drain`、`pending sender bucket snapshot`，以及 pending 出队的 sender 轮转调度。 |
| EVM-P05 | 收益归集/换汇/发放最小闭环 | Done | `crates/plugins/evm/plugin/src/lib.rs` + `crates/gateways/evm-gateway/src/main.rs` | 已接入 `settlement -> payout_instruction` 生产链路；gateway 不再丢弃 settlement 记录，已将 settlement 按账本键直接写入 `.opsw1`（`ledger:evm:settlement:v1:*`、`ledger:evm:settlement_reserve_delta:v1:*`、`ledger:evm:settlement_payout_delta:v1:*`、`ledger:evm:settlement_status:v1:*`），并将 payout 直接编码为完整账本 op（`ledger:evm:payout:v1:*`、`ledger:evm:reserve_delta:v1:*`、`ledger:evm:payout_delta:v1:*`、`ledger:evm:payout_status:v1:*`、`ledger:evm:reserve_debit:v1:*`、`ledger:evm:payout_credit:v1:*`）后写入 `.opsw1`，不再依赖 `novovm-node` 通用入口做 EVM 专属投影。gateway 已补齐最小对账查询面（`evm_getSettlementById`、`evm_getSettlementByTxHash`）和失败补偿最小路径：payout 持久化失败时状态进入 `compensate_pending_v1` 并落待补偿记录，可通过 `evm_replaySettlementPayout` 重放；自动补偿现支持上限/冷却/阈值三参数（`NOVOVM_GATEWAY_EVM_PAYOUT_AUTOREPLAY_MAX`、`NOVOVM_GATEWAY_EVM_PAYOUT_AUTOREPLAY_COOLDOWN_MS`、`NOVOVM_GATEWAY_EVM_PAYOUT_PENDING_WARN_THRESHOLD`）并支持启动按上限回填 pending（`NOVOVM_GATEWAY_EVM_PAYOUT_PENDING_HYDRATE_MAX`）。 |
| EVM-P06 | 原子跨链 intent 本地检查后广播门控 | Done | `crates/plugins/evm/plugin/src/lib.rs` + `crates/gateways/evm-gateway/src/main.rs` | 插件侧已产出 `receipt + broadcast_ready` 队列；gateway 侧已改为严格原子门控：仅在 `wants_cross_chain_atomic=true` 时消费原子队列，并要求“无 rejected + 当前 tx 命中 ready intent”才放行，否则直接拒绝并返回稳定错误码/结构化错误。门控通过后，命中的 `atomic_ready` 已落 `.opsw1`（`ledger:evm:atomic_ready:v1:*`）并自动写入广播队列键（`ledger:evm:atomic_broadcast_queue:v1:*`）；状态推进到 `broadcast_queued_v1`，并同步进入 pending 广播票据（统一由执行器消费，避免“已排队但无执行来源”）。同时保留最小补偿闭环：落盘失败转 `compensate_pending_v1`，支持 `evm_replayAtomicReady` 手工重放为 `compensated_v1`，以及 `evm_getAtomicReadyByIntentId` 查询状态。广播侧新增 `evm_queueAtomicBroadcast`（手工重入队）、`evm_markAtomicBroadcastFailed`（失败标记+挂起重放票据）、`evm_replayAtomicBroadcastQueue`（失败后重放入队）、`evm_markAtomicBroadcasted`（广播完成确认），并新增 `evm_executeAtomicBroadcast`（单 intent 实播）+ `evm_executePendingAtomicBroadcasts`（pending 批量实播）；执行器由 `NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC` 指定，支持最小重试/超时/退避策略（`retry`、`timeout_ms`、`retry_backoff_ms` 及默认环境变量 `NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_RETRY`、`NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_TIMEOUT_MS`、`NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_RETRY_BACKOFF_MS`，已定版默认 `retry=1`、`timeout_ms=5000`、`retry_backoff_ms=25`），并已定版批量策略参数（`NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_BATCH_DEFAULT`、`NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_BATCH_HARD_MAX`，请求值会被硬上限钳制）；执行器请求在可用时会附带 `tx_ir_bincode`（来自 `atomic_ready` 匹配 leg，并按 `intent_id` 缓存，减少二次外部查询），执行器输出若为 JSON，将进行最小一致性校验（`broadcasted/intent_id/tx_hash/chain_id`）。默认走内联原生执行路径（直接写入 EVM ingress `.opsw1` 并复用同一结算/发放主线）；支持请求参数 `native=true`（或 `force_native=true`）显式锁定原生路径，也支持 `use_external_executor=true`（或 `exec=true`）切换到外部执行器路径。成功置 `broadcasted_v1`，失败置 `broadcast_failed_v1` 并保留重放票据。 |
| EVM-P07 | EVM Rust 全功能镜像（网络/同步/txpool/运维） | Done | `crates/gateways/evm-gateway/src/main.rs` + `docs_CN/Adapters/EVM/NOVOVM-EVM-FULL-MIRROR-NODE-MODE-SPEC-2026-03-11.md` | 查询面继续扩展到主线：`eth_chainId`、`net_version`、`web3_clientVersion`、`eth_protocolVersion`、`net_listening`、`net_peerCount`、`eth_accounts`、`eth_coinbase`、`eth_mining`、`eth_hashrate`、`eth_maxPriorityFeePerGas`、`eth_feeHistory`、`eth_syncing`、`eth_pendingTransactions`、`eth_blockNumber`、`eth_getBalance`、`eth_getBlockByNumber`、`eth_getBlockByHash`、`eth_getTransactionByBlockNumberAndIndex`、`eth_getTransactionByBlockHashAndIndex`、`eth_getBlockTransactionCountByNumber`、`eth_getBlockTransactionCountByHash`、`eth_getBlockReceipts`、`eth_getUncleCountByBlockNumber`、`eth_getUncleCountByBlockHash`、`eth_getUncleByBlockNumberAndIndex`、`eth_getUncleByBlockHashAndIndex`、`eth_getLogs`、`eth_newFilter`、`eth_getFilterChanges`、`eth_getFilterLogs`、`eth_uninstallFilter`、`eth_newBlockFilter`、`eth_newPendingTransactionFilter`、`txpool_content`、`txpool_contentFrom`、`txpool_inspect`、`txpool_inspectFrom`、`txpool_status`、`txpool_statusFrom`、`eth_call`、`eth_getCode`、`eth_getStorageAt`。数据源统一来自 gateway EVM tx 索引（内存 + RocksDB 回补扫描），保持“对外 RPC、对内二进制流水线”口径。`eth_getCode/eth_getStorageAt` 已从占位返回升级为基于索引交易的最小状态投影读取（部署代码、调用槽位、部署 code-hash@slot0）。`eth_call` 已补最小读路径（空 calldata 读代码、32B calldata 读槽位、`balanceOf(address)` 读本地索引余额）。`eth_getTransactionCount` 已补 `latest/pending` 分离语义：默认按索引计算 `latest` nonce，并在 `pending` 标签下叠加 UA 路由 nonce（若存在绑定）。`eth_getBlockReceipts` 边界已固定：命中 hash/number 返回 receipts；`number <= latest` 且当前伪块无交易返回 `[]`；未知 hash 或超前块返回 `null`。`eth_getTransactionReceipt` 边界已固定：命中 tx hash 返回稳定块字段（`blockHash/blockNumber/transactionIndex/status`），未知 tx hash 返回 `null`。`eth_getTransactionByHash` 边界已固定：命中 tx hash 返回稳定块字段（`blockHash/blockNumber/transactionIndex` 且 `pending=false`），未知 tx hash 返回 `null`。`eth_getBlockByNumber/eth_getBlockTransactionCountByNumber/eth_getUncleCountByBlockNumber` 的空块语义已统一：`number <= latest` 且该伪块无交易时分别返回“空块对象 / 0x0 / 0x0”，超前块返回 `null`。`eth_*Filter` 当前为生产最小语义：filter id 本地注册、增量拉取和卸载；`txpool_content/contentFrom/inspect/inspectFrom/status/statusFrom` 优先投影 EVM 插件运行时快照（executable=>pending、nonce-gap=>queued），无运行时快照时回落 gateway 索引态。`eth_syncing/net_peerCount/eth_blockNumber` 已统一收口到运行时状态（`novovm-network runtime sync`）+ 本地索引兜底，不再使用 snapshot/env 覆盖链路。 |
| EVM-MA01 | Ethereum mainnet live attach（真实状态/真实广播/实网校验） | InProgress | `crates/gateways/evm-gateway/src/main.rs` + `crates/gateways/evm-gateway/src/rpc_eth_upstream.rs` + `crates/gateways/evm-gateway/src/rpc_gateway_exec_cfg.rs` + `crates/novovm-network/src/*` + `scripts/migration/run_evm_mainnet_write_canary.ps1` + `scripts/migration/run_evm_mainnet_connectivity_canary.ps1` + `scripts/migration/run_evm_mainnet_funded_write_canary.ps1` | 当前仓库已具备 `chainId=1` 默认口径、读写 RPC 主线、public-broadcast 框架与 native transport 骨架；已完成真实状态源读路径与真实广播出口接入，并完成固定确认块的读侧实网对拍：gateway 通过 `NOVOVM_GATEWAY_ETH_UPSTREAM_RPC`（支持 `_CHAIN_{id}` / `_CHAIN_0x{id}`）优先直连真实 Ethereum 上游 RPC，已覆盖 `eth_blockNumber/eth_syncing/eth_gasPrice/eth_maxPriorityFeePerGas/eth_feeHistory/eth_getBalance/eth_getBlockByNumber/eth_getBlockByHash/eth_getTransactionByBlockNumberAndIndex/eth_getTransactionByBlockHashAndIndex/eth_getBlockTransactionCountByNumber/eth_getBlockTransactionCountByHash/eth_getBlockReceipts/eth_getUncleCountByBlockNumber/eth_getUncleCountByBlockHash/eth_getLogs/eth_call/eth_estimateGas/eth_getCode/eth_getStorageAt/eth_getProof/eth_getTransactionCount/eth_getTransactionByHash/eth_getTransactionReceipt`，并在失败或 `null` 时回退本地视图；本轮已补上游读热路径 3 次短重试（100ms backoff），降低公开 upstream `no response` 抖动对 live-attach 的影响，并新增写侧 canary 一键脚本（真实 raw 广播 + receipt 轮询 + summary 落盘）。同时新增“链路可达 canary”一键脚本并已实测通过（summary: `artifacts/migration/evm-mainnet-connectivity-canary-summary.json`），验证 `gateway -> upstream mainnet sendRawTransaction` 到达主网广播边界；并新增“本机私钥自动签名+写侧 canary”脚本，自动生成 type2 签名交易后执行同一写侧验证主线。`eth_sendRawTransaction` 在未配置外部执行器时，已可通过 `NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_UPSTREAM_RPC`（未显式配置时回退复用 `NOVOVM_GATEWAY_ETH_UPSTREAM_RPC`）直接调用上游 `eth_sendRawTransaction` 做真实广播，并在失败后再回退 native transport；剩余唯一待收口项是“真实已签名且有余额 raw tx canary 被主网接受并成功回查 receipt”。 |

## 3. 已落地主线（生产意义）

- 边界层请求已可归一化进入内部二进制流水线，不需要在内部再走 RPC/HTTP。
- `eth_sendRawTransaction` 与 `eth_sendTransaction` 已共用可配置公网广播执行主路径（`NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_EXEC`）；启用后失败即拒绝并返回 `public broadcast failed`，避免“已接收但未广播”。
- 公网广播主路径新增原生网络回退：当未配置 `NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_EXEC` 时，若配置了 `NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_NATIVE_{TRANSPORT,NODE_ID,LISTEN,PEERS}`（支持按链覆盖），`eth_send*` 会直接走 `novovm-network` 向配置 peer 广播（`UDP/TCP`）；`require_public_broadcast` 场景下若执行器和原生 peer 都未配置会硬失败，避免“提交成功但不可广播”。
- `novovm-node` 生产入口已固定在 `src/bin/novovm-node.rs`。
- EVM 插件已具备执行/ingress/收益/原子门控的最小运行骨架（持续完善中）。
- EVM 插件 ingress 已补齐 txpool 最小语义（同 sender+nonce 的 price-bump 替换、per-sender 上限、nonce gap 约束、pending->executable 连续提升）并提供显式 pending/executable 分流接口。
- EVM 插件收益链路已从“仅结算记录”升级为“结算记录 + 发放指令”双队列，gateway 侧已将发放指令落入 `.opsw1`，并由 `novovm-node` 投影为账本键写入（含 `status/debit/credit`）交 AOEM 执行。
- gateway 侧 settlement 记录已不再丢弃，改为直接入账本键（`settlement/reserve_delta/payout_delta/status`）形成最小对账闭环。
- gateway 侧已补齐最小对账查询入口：`evm_getSettlementById`、`evm_getSettlementByTxHash`（只读查询，不进入内部写入流水线）。
- gateway 侧已补齐失败补偿最小路径：`compensate_pending_v1 -> evm_replaySettlementPayout -> compensated_v1`。
- gateway 侧已补齐自动补偿最小调度：`NOVOVM_GATEWAY_EVM_PAYOUT_AUTOREPLAY_MAX > 0` 时按上限自动重放 pending payout，并支持冷却窗口与积压阈值（`NOVOVM_GATEWAY_EVM_PAYOUT_AUTOREPLAY_COOLDOWN_MS`、`NOVOVM_GATEWAY_EVM_PAYOUT_PENDING_WARN_THRESHOLD`）。
- gateway 启动阶段已支持从后端按上限回填待补偿指令（`NOVOVM_GATEWAY_EVM_PAYOUT_PENDING_HYDRATE_MAX`）。
- 原子跨链门控已在 gateway 生产路径启用严格校验（按当前 tx 命中 ready intent），不再仅做“有 reject 才失败”的弱门控。
- 原子门控通过后，gateway 已将命中的 `atomic_ready` 写入 `.opsw1` 账本键（`ledger:evm:atomic_ready:v1:*`），保持内部二进制流水线。
- gateway 已补齐 `atomic-ready` 最小补偿路径：`compensate_pending_v1 -> evm_replayAtomicReady -> compensated_v1`，并可用 `evm_getAtomicReadyByIntentId` 查询状态。
- gateway 已补齐 `atomic-ready` 广播队列接线：自动写入 `ledger:evm:atomic_broadcast_queue:v1:*` 并更新状态 `broadcast_queued_v1`，支持 `evm_queueAtomicBroadcast` 手工重入队、`evm_markAtomicBroadcastFailed` 失败标记、`evm_replayAtomicBroadcastQueue` 失败后重放入队、`evm_markAtomicBroadcasted` 广播完成确认，以及 `evm_executeAtomicBroadcast`（单 intent）/`evm_executePendingAtomicBroadcasts`（批量）执行路径（默认原生内联；显式 `use_external_executor=true` 时可切外部执行器）。外部执行器最小 Rust 二进制已提供：`crates/gateways/evm-gateway/src/bin/evm_atomic_broadcast_executor.rs`（环境变量 `NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC`）。
- gateway 已补 EVM 查询面主线路径：`eth_chainId`、`net_version`、`web3_clientVersion`、`eth_protocolVersion`、`net_listening`、`net_peerCount`、`eth_accounts`、`eth_coinbase`、`eth_mining`、`eth_hashrate`、`eth_maxPriorityFeePerGas`、`eth_feeHistory`、`eth_syncing`、`eth_pendingTransactions`、`eth_blockNumber`、`eth_getBalance`、`eth_getBlockByNumber`、`eth_getBlockByHash`、`eth_getTransactionByBlockNumberAndIndex`、`eth_getTransactionByBlockHashAndIndex`、`eth_getBlockTransactionCountByNumber`、`eth_getBlockTransactionCountByHash`、`eth_getBlockReceipts`、`eth_getUncleCountByBlockNumber`、`eth_getUncleCountByBlockHash`、`eth_getUncleByBlockNumberAndIndex`、`eth_getUncleByBlockHashAndIndex`、`eth_getLogs`、`eth_newFilter`、`eth_getFilterChanges`、`eth_getFilterLogs`、`eth_uninstallFilter`、`eth_newBlockFilter`、`eth_newPendingTransactionFilter`、`txpool_content`、`txpool_contentFrom`、`txpool_inspect`、`txpool_inspectFrom`、`txpool_status`、`txpool_statusFrom`、`eth_call`、`eth_getCode`、`eth_getStorageAt`；查询数据源统一来自 EVM tx 索引（内存 + RocksDB 回补扫描），并保持对象参数/数组参数最小兼容入口。`eth_getCode/eth_getStorageAt/eth_call` 已具备最小状态读投影（部署代码、槽位值、`balanceOf` 读路径）。`eth_getTransactionCount` 已支持 `latest/pending/earliest` 最小语义。`eth_*Filter` 已具备生产最小语义（增量变化/全量 logs/卸载），`txpool_content/contentFrom/inspect/inspectFrom/status/statusFrom` 已优先接到插件运行时快照（fallback 到索引态）；`eth_syncing/net_peerCount/eth_blockNumber` 已从硬编码占位切换为运行时同源状态（`novovm-network runtime sync`）+ 本地索引兜底；`eth_getTransactionReceipt` 与 `eth_getTransactionByHash` 命中时都会返回稳定块字段，未命中返回 `null`；按块号查询的空块语义已统一（空块对象/0x0/0x0）。
- 本轮补 mainnet attach 前两段真实代码：`evm-gateway` 已新增 `rpc_eth_upstream.rs`，通过 `NOVOVM_GATEWAY_ETH_UPSTREAM_RPC`（支持 `_CHAIN_{id}` / `_CHAIN_0x{id}`）把 `eth_blockNumber/eth_syncing/eth_gasPrice/eth_maxPriorityFeePerGas/eth_getBalance/eth_getBlockByNumber/eth_getBlockByHash/eth_getCode/eth_getStorageAt/eth_getProof/eth_getTransactionCount/eth_getTransactionByHash/eth_getTransactionReceipt` 优先直连真实 Ethereum 上游 RPC；上游失败或返回 `null` 时回退本地视图。同时 `eth_sendRawTransaction` 在未配置外部执行器时，已可通过 `NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_UPSTREAM_RPC`（未显式配置时回退复用 `NOVOVM_GATEWAY_ETH_UPSTREAM_RPC`）直接调用上游 `eth_sendRawTransaction` 做真实广播，并在失败后再回退 native transport，保持 `SUPERVM 主网 + EVM 镜像域` 并存架构不变。
- 本轮补 `eth_chainId/net_version` 链参数兼容：两者已统一支持 `chain_id/chainId`（含 `tx` 嵌套对象）并保持默认链回退，语义与其他 `eth_*` 查询入口一致。
- 本轮补 `eth_getTransactionByHash/eth_getTransactionReceipt` 的默认链隔离：未显式传 `chain_id` 时按默认链过滤（不再跨链命中 runtime/index），显式 `chain_id/chainId`（含 `tx.chainId`）可覆盖默认链，确保与其余查询接口链语义一致。
- 本轮补 `eth_syncing` 多链状态源隔离：同步快照文件支持按链读取（`chains.{chain_id}` 或顶层 `{chain_id}`），并新增按链环境覆盖键（如 `NOVOVM_GATEWAY_ETH_SYNC_CURRENT_BLOCK_CHAIN_137`）；同时新增按链快照路径键（`NOVOVM_GATEWAY_ETH_SYNC_STATUS_PATH_CHAIN_<id>`）可覆盖全局快照路径。未命中链专属键时自动回退全局键，避免多链节点共用同步源时链间串值。
- 本轮修正 `eth_syncing` 状态源优先级：由“env -> snapshot 覆盖”改为“snapshot 基线 + env 最终覆盖”，确保运维在不改快照文件的情况下可用环境变量即时覆盖同步高度。
- 本轮补齐 runtime pending 查询语义：`eth_getTransactionByHash` 在索引未命中时会回落到插件运行时快照（executable + queued）返回 pending 交易对象；`eth_pendingTransactions` 优先返回运行时快照（无快照再回落索引）；`eth_getTransactionCount(tag=pending)` 已叠加运行时 txpool nonce 视图（与 UA 路由 nonce / 索引 latest 取最大）。
- 本轮补齐 pending block 语义：`eth_getBlockByNumber("pending")` 与 `eth_getBlockTransactionCountByNumber("pending")` 已直接对接运行时 txpool 快照（executable + queued），不再复用 `latest` 视图。
- 本轮进一步补齐 pending block 扩展语义：`eth_getBlockByHash`、`eth_getTransactionByBlockNumberAndIndex("pending")`、`eth_getTransactionByBlockHashAndIndex`、`eth_getBlockTransactionCountByHash` 已接入同一 pending 视图；`eth_newPendingTransactionFilter/eth_getFilterChanges` 也已优先消费运行时 txpool（运行时为空时回落索引态）。
- 本轮继续补齐 pending 对账边界：`eth_getBlockReceipts`（by number/by hash）与 `eth_getUncleCountByBlockNumber/eth_getUncleCountByBlockHash` 已接入 pending block 视图；`eth_newPendingTransactionFilter/eth_getFilterChanges` 的增量判定已收敛为 hash 集合差分，避免计数偏移带来的漏报。
- 本轮成组收口：`eth_getBlockReceipts(pending)` 与 `eth_getTransactionReceipt` 的 pending 语义已统一为 `pending=true + status=null + blockHash/blockNumber/transactionIndex=null`；`eth_getTransactionReceipt` 现已在索引未命中时回落插件运行时 txpool（按 hash 命中返回 pending receipt）；`eth_syncing` 在返回同步对象时补齐 pending block 边界一致性（不会把 pending 当已同步块，同时 `highestBlock` 不低于 pending 边界）。
- 本轮继续收口 pending 交易对象语义：`eth_getBlockByNumber("pending", true)`、`eth_getBlockByHash(pending-hash, true)`、`eth_getTransactionByBlockNumberAndIndex("pending")`、`eth_getTransactionByBlockHashAndIndex(pending-hash)` 统一返回 `pending=true + blockHash/blockNumber/transactionIndex=null`；`eth_getTransactionByHash` 在索引未命中时也会优先按 runtime pending block 返回同口径 pending 交易对象。
- 本轮新增回归锁定：`eth_syncing` 的“对象态 + pending 边界”已抽成纯函数并加专门测试，固定 `startingBlock<=currentBlock<=highestBlock` 且 pending 场景下 `highestBlock>=local_current+1` 的语义，避免后续改动回退。
- 本轮补端到端一致性样例：同一 runtime 链下，当存在 pending block 时，`eth_blockNumber` 保持已确认高度（如 `0x0`），`eth_getBlockByNumber("pending")` 返回下一高度（如 `0x1`），`eth_syncing` 进入对象态并将 `highestBlock` 抬升到 pending 边界（如 `0x1`），统一“已确认高度 / pending 视图 / 同步状态”三者边界。
- 本轮继续收口 pending/runtime 边界：新增统一入口 `resolve_gateway_eth_pending_block_for_runtime_view`，把 `eth_getBlockByNumber/Hash(pending)`、`eth_getTransactionByBlock*`、`eth_getBlockReceipts`、`eth_getTransactionReceipt`、`eth_getFilterChanges`（logs pending 窗口）全部改为复用 runtime+local 合并链高（与 `eth_syncing/eth_blockNumber` 同源）；并补回归 `eth_pending_block_and_receipts_follow_runtime_current_when_index_lags` 锁定“runtime 链高领先时 pending 块号=runtime_current+1”语义。
- 本轮继续收口 receipt 语义一致性：`eth_getTransactionReceipt`（runtime pending 命中）与 `eth_getBlockReceipts(pending)` 对同一交易的 `pending/status/blockNumber/blockHash/transactionIndex/cumulativeGasUsed/gasUsed` 已加回归对齐断言，避免接口间 pending 字段语义漂移。
- 本轮补“pending -> confirmed”切换优先级回归：同 hash 交易一旦进入索引（已确认），`eth_getTransactionByHash` 与 `eth_getTransactionReceipt` 必须优先返回已确认视图（`pending=false`，receipt `status=0x1`），即使 runtime pending 快照仍暂时存在该交易。
- 本轮补 `eth_getBlockReceipts` 的 confirmed-first 边界：按 `block_hash` 查询时先匹配已确认索引块，再回退 pending 视图；并新增共存回归（同链已确认块 + runtime pending 同时存在）验证：确认块 `receipts` 返回 `pending=false/status=0x1`，pending 块仍返回 `pending=true/status=null`。
- 本轮继续补 hash 查询的 confirmed-first 边界：`eth_getBlockByHash` 与 `eth_getTransactionByBlockHashAndIndex` 已统一先匹配已确认索引块，再回退 runtime pending 视图；新增“已确认 + runtime pending 共存”回归，锁定同 hash 场景下优先返回确认态（`pending=false`）。
- 本轮继续补 hash 计数语义收口：`eth_getBlockTransactionCountByHash` 与 `eth_getUncleCountByBlockHash` 已统一 confirmed-first（先匹配已确认索引块，再回退 pending 视图），并补“已确认 + runtime pending 共存”回归断言。
- 本轮补 `eth_getLogs` 的 hash 边界语义：`blockHash` 查询在未命中已确认索引时会回退 runtime pending block hash（与其他 hash 查询口径一致）；同时固定 `blockHash` 与 `fromBlock/toBlock` 互斥（同时提供直接返回错误），并补运行时 pending hash 与互斥报错回归断言。
- 本轮补 `eth_getLogs(fromBlock/toBlock)` 的 pending 范围语义：`toBlock="pending"`（或范围触达 pending）会纳入 runtime pending block；并同步修正 logs filter 增量分支（`eth_getFilterChanges`）的 pending 上界，补 `latest..pending` 首次返回/二次为空的回归断言。
- 本轮补 `eth_feeHistory` 的 pending 边界语义：`newestBlock="pending"` 在 runtime pending block 存在时会覆盖 pending 区间；`gasUsedRatio` 改为按区块交易 `gas_limit` 汇总计算（默认块 gas 上限 `NOVOVM_GATEWAY_ETH_BLOCK_GAS_LIMIT=30000000`），`reward` 改为按区块 `gas_price` 百分位最小实现，不再固定常量占位。
- 本轮补 receipt 语义收口：`eth_getTransactionReceipt` 与 `eth_getBlockReceipts`（confirmed/pending）的 `cumulativeGasUsed` 改为块内累计值（不再等于单笔 `gasUsed` 占位），并补多笔块与 pending/confirmed 切换回归断言。
- 本轮补 `eth_gasPrice` 生产化语义：不再固定返回默认值，改为三层估算（优先 runtime pending txpool 中位数，其次最近链上样本中位数，最后才回退 `NOVOVM_GATEWAY_ETH_DEFAULT_GAS_PRICE`），并补回归断言锁定优先级。
- 本轮补 `safe/finalized` 块标签兼容：统一映射到当前已确认 `latest` 语义，已覆盖 `eth_getBlockByNumber` 与 `eth_getBlockTransactionCountByNumber` 回归断言（保持“外部 RPC 兼容、内部二进制流水线”不变）。
- 本轮补 receipt 字段兼容：`logsBloom` 由 `0x0` 收口为标准空 bloom（256-byte，`0x`+512 hex），并在合约创建交易（`to=null`）时填充 `contractAddress`（基于 `from+nonce` 可复现）；已补独立回归断言。
- 本轮补区块对象核心字段兼容：`eth_getBlockByNumber/eth_getBlockByHash` 返回的 `gasUsed` 改为按块内交易 `gas_limit` 汇总，`gasLimit` 统一对齐 `NOVOVM_GATEWAY_ETH_BLOCK_GAS_LIMIT`（默认 30000000），并将 `transactionsRoot/stateRoot/receiptsRoot` 与 `sha3Uncles` 从占位 `0x0` 收口为稳定 32-byte 形态字段。
- 本轮补 `eth_getLogs` `topics` 全位置语义：从仅 `topics[0]` 过滤升级为完整位置匹配（每槽 `null/string/string[]`，支持 OR 与多槽严格匹配）；并补回归断言覆盖 `topic[0]` OR、第二槽通配、第二槽严格不匹配与非法槽类型报错。
- 本轮补 `web3_sha3` 兼容入口：新增标准 Keccak-256 哈希接口（支持数组参数与对象参数 `data/input`），不改内部执行链路，仅作为外部 RPC 读接口补齐。
- 本轮补 `eth_getProof` 最小镜像语义：接入账户/存储 proof 读接口（`address/balance/nonce/codeHash/storageHash/storageProof`）；账户与存储值复用现有索引状态投影（与 `eth_getBalance/eth_getCode/eth_getStorageAt` 同源），`storageProof.proof/accountProof` 先返回最小空证明结构，保持“外部 RPC 兼容、内部二进制流水线不变”。
- 本轮继续收口 `eth_getProof` 边界：补齐 `earliest/latest/safe/finalized/pending/number` 标签语义（`number>latest` 返回 `null`，非法标签报错），并补对象参数/数组参数两种入口兼容回归（`storageKeys/storage_keys` + `blockTag/block`）。
- 本轮补 `eth_getProof` runtime pending 叠加回归：在 runtime txpool 有未确认交易时，`latest` 与 `pending` 返回差异被锁定（`balance/nonce/storageProof` 随 pending 视图变化），确保 pending 标签不是空壳。
- 本轮补 `eth_getBalance` 标签语义收口并与 `eth_getProof` 对齐：`latest/safe/finalized/pending/earliest/number` 统一走同一状态视图选择逻辑；并在 runtime 回归里锁定 `eth_getBalance(pending)` 与 `eth_getProof.balance(pending)` 一致，以及 `eth_getTransactionCount(pending)` 与 `eth_getProof.nonce(pending)` 一致。
- 本轮补 `eth_getTransactionCount` 数值块标签历史语义：`block` 为数值标签时，不再一律回退 `latest`，改为按 `<= block_number` 的历史视图计算 sender nonce；并补 `0x1/0x2` 历史断言回归（pending/latest 语义保持不变）。
- 本轮补 `eth_getTransactionCount` future 数值块语义：对齐 `eth_getCode/eth_getStorageAt/eth_getProof/eth_call` 的块标签口径，`block_number > latest` 时返回 `null`（不再回退 `latest`），并补回归断言锁定。
- 本轮补 `eth_getTransactionCount` 截断窗口保护：当启用扫描窗口且请求历史块早于当前窗口最早块时，返回 `null`（不再使用不完整窗口样本误算 nonce）；并补回归断言锁定该行为。
- 本轮补状态读接口截断窗口保护：`eth_getBalance`、`eth_getProof`、`eth_getCode`、`eth_getStorageAt`、`eth_call` 共用历史视图解析已加入“窗口外历史块返回 `null`”语义（早于当前窗口最早块时不再误算），并补独立回归断言锁定。
- 本轮补状态读接口标签语义收口：`eth_getCode`、`eth_getStorageAt`、`eth_call` 已统一接入与 `eth_getProof/eth_getBalance` 同源的状态视图选择（`latest/safe/finalized/pending/earliest/number`）；历史块读取与 `earliest` 语义固定，未来块（`number > latest`）返回 `null`，避免状态读接口间口径不一致。
- 本轮补 `eth_sendTransaction` 入口兼容：`nonce` 现可省略，网关会按 pending 视图自动选 nonce（router next nonce + 链上 latest nonce + runtime txpool nonce 取最大）；`chain_id` 省略时回落到 `NOVOVM_GATEWAY_ETH_DEFAULT_CHAIN_ID`，并已补回归锁定自动 nonce 落盘值。
- 本轮补写入入口 `uca_id` 兼容：`eth_sendTransaction` / `eth_sendRawTransaction` 在 `uca_id` 省略时会按 `from` 绑定自动反查 owner UCA（若显式 `uca_id` 与绑定 owner 不一致则拒绝），提升外部 EVM 钱包/SDK 直连兼容。
- 本轮补查询窗口截断语义：当 `eth_getTransactionByHash/eth_getTransactionReceipt` 命中的是已确认索引交易，但受 `NOVOVM_GATEWAY_ETH_QUERY_SCAN_MAX` 截断导致无法恢复块内位置时，返回已收口为“confirmed-unknown-position”（`pending=false`、`blockNumber` 保留、`blockHash/transactionIndex=null`；receipt 同步 `status=0x1`），避免误报 `pending=true`。
- 本轮补 RocksDB 链+块号二级索引：写入 `eth tx index` 时同步落 `block_index` 键，并在 `eth_getTransactionByHash/eth_getTransactionReceipt` 的 confirmed 查询回退阶段优先按 `chain_id+block_number` 精确回补块内交易；`eth_getBlockReceipts` 在按块号查询且扫描窗口截断时也改为同源精确回补，避免误返回 `[]/null`。
- 本轮继续补“按块号查询”收口：`eth_getBlockByNumber`、`eth_getTransactionByBlockNumberAndIndex`、`eth_getBlockTransactionCountByNumber`、`eth_getUncleCountByBlockNumber` 在扫描窗口截断时已接入同一 `chain_id+block_number` 精确回补路径，不再因窗口限制误返回 `null`（存在交易块）或错误计数。
- 本轮补“按块 hash 查询”收口：新增 RocksDB `block_hash -> block_number` 索引回退，`eth_getBlockByHash`、`eth_getTransactionByBlockHashAndIndex`、`eth_getBlockTransactionCountByHash`、`eth_getBlockReceipts`、`eth_getUncleCountByBlockHash` 在扫描窗口截断时已改为同源精确回补，不再因窗口限制误返回 `null`。
- 本轮补 `eth_getLogs(blockHash)` 收口：在已确认块命中但受扫描窗口截断时，`eth_getLogs`、`eth_getFilterLogs`、`eth_getFilterChanges`（按 `blockHash` 的一次性增量）已接入同一 `block_hash -> block_number` 精确回补，不再漏日志。
- 本轮补 `eth_getLogs(fromBlock/toBlock)` 收口：当查询范围为显式块号且范围不超过 `NOVOVM_GATEWAY_ETH_QUERY_SCAN_MAX` 时，日志查询与 filter 拉取会按块号走精确回补（`chain_id+block_number` 索引），修复窗口截断导致的范围日志遗漏。
- 本轮补 pending 收口：`eth_getBlockReceipts` 现支持“pending 数值块号”（`latest+1`）与 `block="pending"` / `block_hash=<pending_hash>` 同语义返回（`pending=true`、`status=null`、一致的 `blockNumber/blockHash/transactionIndex/cumulativeGasUsed`）；并补回归断言锁定与 `eth_getTransactionReceipt` 的 pending 视图一致性。
- 本轮补链高度读取收口：RocksDB 的 `load_eth_txs_by_chain` 改为按 `block_index` 逆序窗口取样（优先最新块，不再按 tx-hash 前缀随机采样），并新增 `load_eth_latest_block_number`；`eth_blockNumber` 与 `eth_syncing` 的本地高度已接入该最新块索引路径，避免小扫描窗口下高度被低估。
- 本轮补 `latest` 语义统一：`eth_feeHistory`、`eth_getBalance`、`eth_getBlockBy*`、`eth_getTransactionByBlock*`、`eth_getBlockTransactionCountBy*`、`eth_getBlockReceipts`、`eth_getUncle*`、`eth_getLogs/eth_*Filter`、`eth_call`、`eth_getCode`、`eth_getStorageAt`、`eth_getProof` 等查询链路的 `latest` 均已统一复用“最新块索引”路径，修复窗口截断下各接口 `latest` 高度不一致问题。
- 本轮补内存窗口收口：`collect_gateway_eth_chain_entries` 的内存采样已从“随机迭代截断”改为“按 `nonce+tx_hash` 排序后取最新窗口”，避免 `max_items` 较小时链视图随机漂移；并新增内存窗口回归断言锁定该行为。
- 本轮补发送入口 EIP-1559 兼容：`eth_sendTransaction` 在未显式传 `type` 时，若请求包含 `maxFeePerGas/maxPriorityFeePerGas`，会自动推断 `type=0x2`（仍保持显式 `type` 优先）；并新增回归断言锁定推断行为与入索引字段一致性。
- 历史口径（已废弃）：曾将 pending block 作为 `eth_syncing` 对象态抬升条件；现已改为“pending 不参与同步中判定”，仅真实下载差值（`highest > current`）返回同步对象。
- 本轮继续收口 `latest` 高度来源一致性：`eth_newBlockFilter`、`eth_getProof`、`eth_getTransactionCount`（future 历史判断）、`eth_getTransactionByHash`/`eth_getTransactionReceipt`（runtime pending 回退）已统一改为“RocksDB 最新块索引”优先，不再依赖内存窗口 `max(nonce)`；并新增回归断言覆盖“内存窗口陈旧 + store 最新高度存在 + runtime pending 查询”的场景。
- 本轮补 `eth_getFilterChanges`（block filter）在“内存窗口陈旧 + store 有新块”场景的漏报问题：分支已改为先取统一 `latest`，并按块号走精确回补（`chain_id + block_number`）发出缺失块 hash，同时固定 `last_seen_block` 只能单调前进，避免回退导致重复/漏报；并新增回归断言锁定该边界。
- 本轮补 `eth_feeHistory` 在“窗口截断 + store 有目标块”场景的数据空洞：当范围内块不在内存窗口时，已改为按 `chain_id + block_number` 精确回补该块交易用于 `gasUsedRatio/reward` 计算；并新增回归断言锁定 `gasUsedRatio/reward` 可从 store 精确恢复，不再退化为空块占位。
- 本轮补 `eth_estimateGas` 生产语义收口：内在 gas 估算已对齐合约部署基础成本（`+32000`）与 initcode 字成本（`+2/word`），并接入 `accessList` intrinsic 成本（`2400/address + 1900/storageKey`）；同时 EVM Core 校验新增部署 initcode 大小上限（49152 bytes），避免低估 gas 或异常大部署载荷进入主线。
- 本轮继续收口 `type1/type2 raw tx` 写入与执行一致性：EVM Core 已解析 raw `accessList` 计数（address/storageKey），`validate_tx_semantics_m0` 在 raw signature 可解析时会把 `accessList` intrinsic 成本并入最小 gas 校验（低 gas 将拒绝），并与 gateway `eth_estimateGas` 共用同一 intrinsic 公式实现，消除估算/执行口径漂移。
- 本轮补 `eth_sendTransaction` 与 `accessList` 的生产一致性：对象入口已接入 `accessList` intrinsic 校验（`gas_limit` 低于 `base + accessList` 时直接拒绝），并在“无显式 `type` 且存在 `accessList` 成本”时自动推断 `type=0x1`；同时将 `accessList` 计数纳入网关交易哈希输入，避免不同 `accessList` 请求落同哈希。
- 本轮补 `effectiveGasPrice/baseFee` 成组收口：`eth_getBlockBy*` 与 `eth_feeHistory` 的 `baseFeePerGas` 统一走同一来源（`NOVOVM_GATEWAY_ETH_DEFAULT_BASE_FEE_PER_GAS`，默认回退 `NOVOVM_GATEWAY_ETH_DEFAULT_GAS_PRICE`）；receipt 的 `effectiveGasPrice` 对 `type2` 统一采用 `max(gas_price, baseFeePerGas)`，对象入口与 raw 入口语义一致。
- 本轮补交易查询费率字段兼容：`eth_getTransactionByHash` / `eth_getTransactionByBlock*` / `eth_pendingTransactions` / `txpool_content` 的交易对象统一输出 `maxFeePerGas/maxPriorityFeePerGas`（仅 `type2` 有值，其他类型为 `null`）；runtime pending 路径会在可识别 `raw type2` 签名时对齐同一语义。
- 历史口径（已废弃）：曾把 runtime pending 视图直接并入 `eth_syncing` 对象构造；现已收口为“pending 与 syncing 解耦”，避免 pending-only 场景误报同步中。
- 本轮补 `eth_syncing` 回归稳定性：涉及环境变量覆盖的多条测试已加统一环境锁，消除并行测试下的链间环境串扰，避免 CI 偶发假红（生产逻辑不变）。
- 本轮补写入入口参数兼容收口：`eth_sendRawTransaction` 已兼容 `chainId`（camelCase）与 `sessionExpiresAt` / `wantsCrossChainAtomic` / `signatureDomain` 参数别名解析，`eth_sendTransaction` 同步兼容 `signatureDomain`、`sessionExpiresAt`、`wantsCrossChainAtomic`；并补回归锁定 `chainId` 别名仍参与严格链 ID 一致性校验（与 raw 内链 ID 不一致会拒绝）。
- 本轮修正公共入口边界：`novovm-node` 已撤回 EVM 专属 `project_gateway_evm_payout_ops` 包裹，恢复通用入口纯读入执行；EVM payout 改为在 gateway 生产侧直接编码完整账本 op 写入 `.opsw1`（不再依赖 node 侧二次投影）。
- 本轮补查询/写入参数混排兼容：`eth_getTransactionReceipt`、`eth_getBlockReceipts`、`eth_getBlockBy*`、`eth_sendRawTransaction`、`eth_sendTransaction` 等入口的核心参数提取已统一支持“对象参数 + 标量参数混排数组”形态（如 `[{"chainId":1}, "0x..."]`）；`block tag/hash`、`tx hash`、`raw_tx`、`address` 解析不再依赖“必须在数组首位”，并补回归锁定该兼容行为。
- 本轮继续收口“前置链参数对象 + 业务对象/标量”混排：`eth_call`、`eth_getProof`、`eth_getStorageAt`、`eth_getLogs`、`eth_getBalance`、`eth_getTransactionCount`、`eth_getTransactionByBlock*` 等解析链路已统一为“数组中按 key 选对象 + 按有效参数位取标量”，不再依赖 `arr[0]/arr[1]/arr[2]` 固定位置；并补回归锁定 `call/proof/logs/slot/blockTag` 的混排语义。
- 本轮补 `eth_feeHistory` 混排参数兼容：在 `[{"chainId":...}, blockCount, newestBlock, rewardPercentiles]` 形态下，`blockCount/newestBlock/rewardPercentiles` 已按“非对象参数位”正确解析；同时修正 block+index 双标量场景下 block 标签优先级（`eth_getTransactionByBlock*` 不会把 `txIndex` 误当 block tag）。
- 本轮继续收口 pending block 存在边界：仅当 runtime txpool 存在待打包交易时，`eth_getBlockByNumber(pending)`、`eth_getBlockReceipts(pending)`、`eth_getTransactionByBlockNumberAndIndex(pending)`、`eth_getBlockTransactionCountByNumber(pending)`、`eth_getUncleCountByBlockNumber(pending)` 才返回 pending 视图；无 runtime pending 时统一返回 `null`，并补回归 `eth_pending_block_queries_return_null_without_runtime_pending_txs` 锁定语义。
- 本轮继续收口 txpool/pending 语义到 runtime-only：`eth_pendingTransactions`、`txpool_content`、`txpool_contentFrom`、`txpool_inspect`、`txpool_inspectFrom`、`txpool_status`、`txpool_statusFrom` 在无 runtime txpool 数据时统一返回空结果（`[]` / `{pending:{},queued:{}}` / `pending=0x0,queued=0x0`），不再回退已确认索引交易，避免“确认态被误当 pending”。
- 本轮补齐 txpool/pending 双源可见性：`collect_gateway_eth_txpool_runtime_txs` 现统一合并 `executable ingress + pending sender buckets + pending ingress(parsed_tx)`，并按 `tx_hash` 去重后输出到 `eth_pendingTransactions`、`txpool_*`、`newPendingTransactions filter` 同一数据面，减少“仅 runtime 快照可见”导致的 pending 漏观测。
- 本轮继续收口 pending filter：`eth_newPendingTransactionFilter / eth_getFilterChanges` 已切到 runtime-only 基线（仅消费 runtime txpool hash 集合差分），不再回退已确认索引交易；当只新增确认态索引交易且 runtime 无新增 pending 时，增量返回固定为空。
- 本轮继续收口 logs pending 边界：`eth_getLogs(blockHash)` 的 pending 回退已统一为 runtime-only（无 runtime pending 时不再构造空 pending 块参与匹配）；`eth_getFilterChanges` 在 `fromBlock=latest,toBlock=pending` 且当前无 runtime pending 时不再把游标推进到 `latest+1`，避免后续新确认块被跳过，并新增回归覆盖该场景。
- 本轮补 `txpool_contentFrom` / `txpool_inspectFrom` / `txpool_statusFrom` 生产入口（按地址查看 txpool 视图）：均已接到 runtime txpool 数据面（pending/queued），不回退已确认索引交易；并补对象参数与混排数组参数兼容回归（`chain_id/chainId + address`）。
- 本轮继续收口 receipt/syncing 边界：`eth_getBlockReceipts` 与 `eth_getTransactionReceipt` 的 pending 回执构造已统一到同一函数路径（确保 `pending/status/blockHash/blockNumber/transactionIndex/cumulativeGasUsed` 同源一致）；`eth_syncing` 的 pending 边界计算改为与查询面同源的 latest 解析路径，避免口径漂移，并补回归断言锁定“一笔 pending tx 的 hash 回执 == pending block 回执项”。
- 本轮补公网广播执行参数的按链覆盖：`NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_EXEC_RETRY`、`NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_EXEC_TIMEOUT_MS`、`NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_EXEC_RETRY_BACKOFF_MS` 均已支持链级键（`*_CHAIN_<dec>` / `*_CHAIN_0x<hex>`）并回退全局键，保持多链共节点时每条链可独立调优广播执行策略。
- 本轮补原子广播执行器参数的按链覆盖：`NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC`、`NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_RETRY`、`NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_TIMEOUT_MS`、`NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_RETRY_BACKOFF_MS` 现已支持链级键（`*_CHAIN_<dec>` / `*_CHAIN_0x<hex>`）并回退全局键；`evm_executeAtomicBroadcast` 与 `evm_executePendingAtomicBroadcasts` 已接入按 ticket 链 ID 的默认参数解析，确保多链同节点下执行器配置不串链。
- 本轮补 logs 入口嵌套参数兼容：`parse_eth_logs_query_from_params`、`parse_eth_logs_address_filters`、`parse_eth_logs_topic_filters` 已支持 `filter` 嵌套对象（如 `[{chainId}, "logs", {filter:{...}}]`）；`eth_subscribe/logs` 与 `eth_newFilter/eth_getLogs` 共用该解析路径，已补回归锁定混排数组 + 嵌套 filter 的生产语义。
- 本轮补 `eth_subscribe` 对象风格兼容：新增订阅类型解析（`kind/subscription/type/event`），支持对象参数形态（如 `{kind:\"logs\", chainId, filter:{...}}`）与原数组形态并存；已补回归锁定对象参数 + 嵌套 filter 的 `logs` 订阅路径。
- 本轮补公网广播必达与提交后直连语义：`eth_sendRawTransaction` / `eth_sendTransaction` 新增请求级 `require_public_broadcast/requirePublicBroadcast`（可覆盖链级默认要求）；新增 `return_detail/returnDetail` 可选返回（`accepted/pending/onchain/broadcast`），保持默认返回 tx hash 不变；广播执行信息（`mode/attempts/executor/executor_output`）可直接被上层消费。
- 本轮补提交状态直连查询：新增 `evm_getTxSubmitStatus`，统一返回 `accepted/pending/onchain/receipt/error_code/error_reason`，将 runtime pending、索引命中、未命中三类状态收口到一个可直接消费的稳定接口。
- 本轮补订阅 ID 兼容：`eth_unsubscribe/eth_uninstallFilter` 的 id 解析已支持 `subscription/subscription_id/subscriptionId/sub_id/subId`（含原 `filter_id/filterId/id`），降低上游客户端适配成本。
- 本轮补直连入口（无额外包装层）：新增 `evm_sendRawTransaction` / `evm_sendTransaction` / `evm_getLogs` / `evm_getTransactionReceipt`。其中 `evm_send*` 内部直接复用生产 `eth_send*` 主路径并强制 `require_public_broadcast=true + return_detail=true`，便于上游直接消费“公网广播+提交回执”语义；`evm_getLogs/evm_getTransactionReceipt` 直接复用同源查询语义，避免重复实现。
- 本轮补提交状态广播语义直出：`evm_getTxSubmitStatus` 已增加 `broadcast` 字段（`mode/attempts/executor/executor_output/updated_at_unix_ms`）；`evm_sendRawTransaction` / `evm_sendTransaction` 的广播结果会在网关进程内轻量状态表中同步更新，供上游在提交后查询时直接消费。
- 本轮补事件订阅直连入口：新增 `evm_subscribe` / `evm_unsubscribe` / `evm_newFilter` / `evm_newBlockFilter` / `evm_newPendingTransactionFilter` / `evm_getFilterChanges` / `evm_getFilterLogs` / `evm_uninstallFilter`，全部直接复用同源 `eth_*` 生产逻辑，减少上游对 `eth_*` 命名耦合。
- 本轮补 gateway->upstream 直连消费边界：新增 `evm_pendingTransactions`、`evm_txpoolContent/evm_txpoolContentFrom`、`evm_txpoolInspect/evm_txpoolInspectFrom`、`evm_txpoolStatus/evm_txpoolStatusFrom`（含 snake_case 别名），并新增 `evm_snapshotExecutableIngress`、`evm_drainExecutableIngress`、`evm_drainPendingIngress`、`evm_snapshotPendingSenderBuckets` 四个内存 ingress/队列直连入口；全部保持“对外 RPC、对内二进制流水线”与现有生产数据面同源，不新增中间工程层。
- 本轮补 pending ingress 非破坏读取：插件侧新增 `snapshot_pending_ingress_frames_for_host`，gateway 新增 `evm_snapshotPendingIngress`（支持 `max_items/max`、`chain_id/chainId`、`include_raw/includeRaw`、`include_parsed/includeParsed`），避免上游直连消费时必须 `drain` 破坏主线队列。
- 本轮补 `evm_*` 读接口别名成组收口：新增并直连复用同源 `eth_*` 生产路径（无新中间层）`evm_chainId/evm_netVersion/evm_syncing/evm_blockNumber/evm_getBalance/evm_getBlockByNumber/evm_getBlockByHash/evm_getBlockReceipts/evm_getTransactionByHash/evm_getTransactionCount/evm_gasPrice/evm_call/evm_estimateGas/evm_getCode/evm_getStorageAt/evm_getProof`（含部分 snake_case 别名），上游可统一走 `evm_*` 命名完成读取与查询闭环。
- 本轮继续补齐 `evm_*` 查询别名（同源复用 `eth_*` 生产路径）：`evm_maxPriorityFeePerGas/evm_feeHistory`、`evm_getTransactionByBlockNumberAndIndex`、`evm_getTransactionByBlockHashAndIndex`、`evm_getBlockTransactionCountByNumber/ByHash`、`evm_getUncleCountByBlockNumber/ByHash`、`evm_getUncleByBlockNumberAndIndex/ByHashAndIndex`（含 snake_case 别名），进一步收口上游仅使用 `evm_*` 的查询面。
- 本轮补齐 `evm_*` 基础节点信息别名（同源复用 `web3/net/eth` 生产路径）：`evm_clientVersion/evm_sha3/evm_protocolVersion/evm_listening/evm_peerCount/evm_accounts/evm_coinbase/evm_mining/evm_hashrate`，进一步减少上游命名切换。
- 本轮补公网广播直连入口与消费别名收口：新增 `evm_publicSendRawTransaction/evm_public_send_raw_transaction`、`evm_publicSendTransaction/evm_public_send_transaction`（同源复用 `eth_send*` 主线并强制 `require_public_broadcast=true + return_detail=true`）；同时为 `evm_getTxSubmitStatus`、结算/原子广播查询与执行入口补齐 snake_case 别名，并为 `evm_getLogs/evm_getTransactionReceipt/evm_newFilter/evm_getFilterChanges/evm_getFilterLogs/evm_uninstallFilter` 等补齐 snake_case 消费别名，减少上游接入改造成本。
- 本轮继续补齐 `evm_*` 命名兼容：新增 `evm_chain_id`、`evm_pending_transactions`、`evm_txpool_content_from`、`evm_txpool_inspect_from`、`evm_txpool_status_from`，并同步把 `evm_snapshot/drian*`、`evm_get_tx_submit_status` 等 snake_case 入口纳入方法清单，进一步降低上游接入改造成本。
- 本轮新增 `evm_getTxSubmitStatusBatch/evm_get_tx_submit_status_batch`：可一次传入多笔 `tx_hash`（`tx_hashes/txHashes/hashes/txs`）批量返回与单笔 `evm_getTxSubmitStatus` 同语义的 `accepted/pending/onchain/receipt/error_code/error_reason/broadcast` 结果，减少上游逐笔轮询开销。
- 本轮新增 `evm_replayPublicBroadcast/evm_replay_public_broadcast`：可按 `tx_hash`（可选 `chain_id/chainId`）对已入索引或 runtime 可见交易触发公网广播重放；内部复用现有 `maybe_execute_gateway_eth_public_broadcast` 生产路径并写回广播状态，不新增中间层。
- 本轮新增 `evm_replayPublicBroadcastBatch/evm_replay_public_broadcast_batch`：可一次传入多笔 `tx_hash` 批量重放公网广播（支持可选链参数），返回 `total/replayed/failed/results`，用于上游批处理补偿场景。
- 本轮新增 `evm_getTransactionReceiptBatch/evm_get_transaction_receipt_batch` 与 `evm_getTransactionByHashBatch/evm_get_transaction_by_hash_batch`：批量接收 `tx_hashes` 并复用现有单笔查询生产语义，减少上游高频逐笔轮询开销。
- 本轮新增 `evm_getLogsBatch/evm_get_logs_batch`、`evm_getFilterChangesBatch/evm_get_filter_changes_batch`、`evm_getFilterLogsBatch/evm_get_filter_logs_batch`、`evm_publicSendRawTransactionBatch/evm_public_send_raw_transaction_batch`、`evm_publicSendTransactionBatch/evm_public_send_transaction_batch`：全部直接复用现有 `eth_* / evm_*` 生产路径，补齐上游并行消费所需的批量入口（logs/filter/public-broadcast）并保持“对外 RPC、对内二进制流水线”。
- 本轮新增 `evm_getTransactionLifecycle/evm_get_transaction_lifecycle` 与 `evm_getTransactionLifecycleBatch/evm_get_transaction_lifecycle_batch`：语义与 `evm_getTxSubmitStatus*` 同源（`accepted/pending/onchain/receipt/error_code/error_reason/broadcast`），作为稳定直连消费别名对外暴露，避免上游绑定历史命名。
- 本轮统一写入口错误语义映射：`gateway_error_{code,message,data}_for_method` 已覆盖 `evm_send*`、`evm_publicSend*` 及其 batch 入口，保证公开写入口与 `eth_send*` 使用同一套错误码/错误信息/结构化 `error.data` 语义。
- 本轮新增公网广播状态直连查询：`evm_getPublicBroadcastStatus/evm_get_public_broadcast_status/evm_getBroadcastStatus/evm_get_broadcast_status` 与其 batch 版本，支持按 `tx_hash(es)` 直接查询广播执行状态（`has_status + broadcast`），并支持可选 `chain_id` 过滤，减少上游重复拼装 lifecycle 查询成本。
- 本轮开始按 `P0-A + P0-B` 做真实代码收口（非工程化包装）：新增 `novovm-network` 运行时同步状态接口（`set/get_network_runtime_sync_status`），并把 `eth_syncing/net_peerCount` 接到运行时状态源；当运行时状态存在时，同步口径由运行时权威源主导，不再被快照/env 覆盖。
- 本轮继续收口 runtime 同步主线：`novovm-network` 收包侧在消费到 `StateSync(block_header_wire_v1)` 后，会按 runtime 拉取窗口自动生成并回发下一跳 `StateSync(NSP1)` 请求（`from/to` 连续推进），形成“请求 -> 回包 -> 自动续拉”的真实下载闭环，不再依赖外层循环反复触发单次拉取。
- 自动续拉链路已补传输兜底：当 `send(peer)` 因 peer 映射暂未命中失败时，会回落到当前 UDP 源地址/TCP 入站连接直发 follow-up，保证同步请求不中断。
- 自动续拉时序已收口到“窗口确认后推进”：本地会跟踪每个 peer 的当前拉取窗口 `to_block`，仅当收到回包高度达到窗口上界后才规划并发出下一窗口请求，避免每条 header 都触发续拉造成重叠请求风暴。
- gateway native runtime worker 已补同步请求去重+超时重发：相同 `phase/from/to` 窗口在短周期内不重复下发，仅窗口变化或超时后重发，减少重复 `NSP1` 请求并保持丢包场景可恢复。
- transport 侧已补 inflight 窗口清理：`unregister_peer`/发送失败断连回收时会清理该 peer 的同步窗口目标，避免残留 inflight 状态阻塞后续续拉。
- gateway 同步拉取 peer 选择已收口：当 runtime 已观测到 peer head 时，`NSP1` 拉取优先只发向最高链高 peer（未知阶段回落全 peer bootstrap），减少同窗多 peer 重复拉取流量。
- 本轮补 `novovm-network` 运行时状态自动上报：UDP/TCP transport 在 `register_peer` 时会直接更新链级 runtime `peer_count`（默认链 ID=1，支持按链构造），使 `eth_syncing/net_peerCount` 可直接消费在线网络状态，不依赖手工注入脚本。
- 本轮补 runtime 同源闭环：当 `eth_syncing` 消费到 runtime 状态时，会把最终 `starting/current/highest` 回写到同一 runtime 状态，形成 `peer_count + block progress` 同源快照，并新增回归锁定“runtime 存在时优先于 env/snapshot”语义。
- 本轮继续收口生产链路：`eth_syncing/net_peerCount/eth_blockNumber` 已移除 snapshot/env 覆盖回退，统一为“runtime 同源状态 + 本地索引兜底”，并将原有 env/snapshot 相关回归改为“忽略外部覆盖”的一致性断言，防止后续回退到工程化覆盖层。
- 本轮补运行时链高自动更新：`novovm-network` 在 UDP/TCP `try_recv` 路径已接入 `Pacemaker(ViewSync/NewView)` 与 `StateSync(block header wire)` 的高度提取，自动推进 runtime `current/highest`，使同步高度不再依赖外部快照注入。
- 本轮补 runtime 观察态汇总：新增 peer/local 观察接口（`register/unregister peer`、`observe peer head`、`observe local head`），runtime 会自动重算 `peer_count/current/highest/starting`；gateway 在 runtime 存在时先上报本地链高再读取汇总结果，收口多来源同步口径。
- 本轮补同步边界固化：runtime 已引入“同步锚点”语义，进入同步后 `starting_block` 锚定本轮起点、追平后自动重置到 `current_block`；并修正“本地链高=0 且 runtime 已有进度”场景不再降级覆盖 runtime 当前高度。
- 本轮补 transport 断连回收：`novovm-network` 的 UDP/TCP transport 已新增 `unregister_peer`，断连时会同步调用 runtime `unregister_network_runtime_peer` 清理观察态并即时重算链级同步口径（`peer_count/current/highest`），避免断连后 `peer_count` 虚高残留。
- 本轮补发送失败自动降级：`novovm-network` transport 在 TCP 连接重试失败/写失败场景会自动下调 runtime peer 活跃状态（同步口径即时回收）；并在后续发送成功时自动恢复 peer 注册，避免长期误报在线 peer。
- 本轮补收包来源自动登记：`novovm-network` transport 收到带 `from` 的协议消息时会自动登记 runtime 活跃 peer，不再依赖手工双向 `register_peer` 才能反映在线对端；`peer_count` 与实际消息流同源。
- 本轮补 runtime stale peer 清理：运行时同步状态在重算时会自动剔除超时未活跃 peer，防止在线计数只增不减导致 `eth_syncing/net_peerCount` 漂移。
- 本轮补 runtime 读路径一致性：`get_network_runtime_sync_status` 在存在观察态时会实时重算并清理 stale peer；无观察态时保持已写状态不被重算覆盖，避免宿主注入口径回退。
- 本轮补 gateway 写入前移上报：EVM tx 索引写入（`upsert_gateway_eth_tx_index`）时会同步上报 runtime 本地链高（单调 `observe_local_head_max`），减少同步状态仅由查询触发更新的延迟。
- 本轮补 UDP 来源地址自动学习：`novovm-network` 在 UDP 收包后会按消息 `from` 自动更新 peer 地址映射，支持未显式双向注册场景下的直接回包，减少对静态 peer 配置依赖。
- 本轮补 UDP 自动学习安全边界：自动学习仅允许同 IP 覆盖已有 peer 映射，防止伪造来源把已知 peer 地址重定向到异常地址。
- 本轮补 runtime 同步锚点收口：当同步中首次观测到非零本地链高时，`starting_block` 会从历史 `0` 重锚到本地起点，避免起点漂移。
- 本轮固化 TCP 短连接边界：TCP 收包侧不做来源地址自动学习（入站源端口为临时端口），避免误写 peer 映射导致错误回连。
- 本轮补 runtime 观测态分层：新增“peer 已观测历史”语义，未进入真实 peer 观测前保留手工注入 `peer_count/highest`，避免仅本地链高观测时把运行时同步状态误降级。
- 本轮补 `eth_syncing` 单调回写：gateway 侧改为 `observe_local_head_max`，并在 runtime 回读发生退化时按上次 runtime 状态做保底合并，最后回写完整 runtime 状态（含 `peer_count`），确保后续 `net_peerCount`/`eth_syncing` 连续读取不回退。
- 本轮补 `eth_blockNumber` 同源化：入口改为复用 `resolve_gateway_eth_sync_status`，统一输出 `max(current_block, local_current_block)`，不再单独二次扫描索引，避免与 `eth_syncing` 口径漂移。
- 本轮补 `eth_syncing` 热路径去重：pending 边界直接基于已解析 sync 状态推导，移除该入口内部重复的链上索引扫描，降低查询热路径开销。
- 本轮补 runtime 抢占边界收口：`resolve_gateway_eth_sync_status` 仅在 runtime 处于“可判定同步态”（`peer_count > 0` 或 `highest > current`）时才走 runtime 权威口径；仅本地观察态（无 peer、无追高）会自动回退 snapshot/env，修复链级同步配置被旧本地观察值长期压制的问题。
- 本轮补区块 `stateRoot` 同源收口：`eth_getBlockByNumber/eth_getBlockByHash` 现按 `block_tag <= block_number` 的累计状态视图计算 `stateRoot`（复用与 `eth_getProof` 同源的状态视图解析），不再仅按“当前块交易子集”计算；并新增回归锁定同块 `stateRoot` 与 proof-view 根一致。
- 本轮完成 `eth_getProof` 实路径收口：`accountProof/storageProof` 从占位空数组改为确定性 sibling proof，`storageHash/stateRoot` 与 proof 同源根计算，不再走空根占位。
- 本轮完成区块根函数语义收口：移除 `pseudo` 命名口径，统一为确定性区块哈希/根函数实现（生产代码路径）。
- 本轮补 `receipt/error` 直连闭环：新增 `eth submit-status` 持久化（`gateway:eth:submit_status:v1:*`，RocksDB+内存同源），写入口失败（如 txpool reject/public broadcast failed）会按 `tx_hash` 持久化标准错误语义；`evm_getTxSubmitStatus/evm_getTransactionLifecycle` 在“索引未命中”场景下会优先返回该持久化失败状态（含 `error_code/error_reason/updated_at_unix_ms`），并在同一 `tx_hash` 后续提交成功时自动清理旧失败状态，避免历史失败污染当前生命周期查询。
- 本轮继续收口 `receipt/error` 批量提交边界：`evm_publicSendRawTransactionBatch/evm_publicSendTransactionBatch` 的失败项现在会同步写入 submit-status（与单笔写入口同语义）；同时单笔写入口失败时若错误文本未携带 `tx_hash`，会回退到参数推导（raw 写入走 `raw_tx` 推导，`sendTransaction` 写入走 `uca_id/binding + nonce + tx 字段` 推导）并落持久化状态，保证后续 `evm_getTxSubmitStatus/evm_getTransactionLifecycle` 可直接查询失败归因，不丢批量失败状态。
- 本轮补 gateway->upstream 直连“单请求聚合面”：新增 `evm_getRuntimeSnapshot/evm_get_runtime_snapshot`，可一次返回同链 `pending_transactions + txpool_status + logs + lifecycle + receipts + broadcast`（按参数可裁剪）；内部全量复用现有生产主路径（`eth_pendingTransactions/txpool_status/eth_getLogs/evm_getTransactionLifecycleBatch/evm_getTransactionReceiptBatch/evm_getPublicBroadcastStatusBatch`），不新增工程化中间层，便于上游直接消费。
- 本轮继续补 gateway->upstream 直连“运行时聚合面”：新增 `evm_getRuntimeBundle/evm_get_runtime_bundle`，在 `evm_getRuntimeSnapshot` 基础上同请求补齐 `executable_ingress + pending_ingress + pending_sender_buckets`，形成“upstream + ingress”一包返回，内部仍只复用现有生产方法，不引入中间包装层。
- 本轮补 gateway->upstream 直连“提交状态聚合面”：新增 `evm_getRuntimeTxStatusBundle/evm_get_runtime_tx_status_bundle`，可一次返回 `accepted/pending/onchain/error_code/error_reason + lifecycle + receipt + broadcast`；支持显式 `tx_hashes` 或自动从 `eth_pendingTransactions` 抽取（`auto_from_pending`），用于上游稳定直消费提交后闭环状态。
- 本轮补 gateway->upstream 直连“事件聚合面”：新增 `evm_getRuntimeEventBundle/evm_get_runtime_event_bundle`，可同请求返回 `eth_getLogs + evm_getFilterChanges(+evm_getFilterLogs)`；支持 `filter_ids/filterIds/filters/ids` 多种输入，减少上游 logs/filter 多次 RPC 拼接开销。
- 本轮补 gateway->upstream 直连“全量总聚合面”：新增 `evm_getRuntimeFullBundle/evm_get_runtime_full_bundle`，同请求汇总 `runtime_status(syncing/peer_count/block_number) + runtime_bundle + tx_status_bundle + event_bundle`，并复用前述生产接口（不引入中间层）实现“一次调用拿全链路状态”。
- 本轮继续去工程化收口 runtime 同步边界：已移除 `evm_ingestRuntimeSyncSnapshot*` 与 `evm_setRuntimeNativeSyncStatus*` 注入式写入口，仅保留 runtime 同步只读接口（`evm_getRuntimeSyncStatus/evm_getRuntimeNativeSyncStatus/evm_getRuntimeSyncPullWindow`）；`eth_syncing/net_peerCount/eth_blockNumber` 仅消费 `novovm-network` 真实运行时观测，不再允许外部 RPC 注入覆盖同步状态。
- 本轮补 network 运行时同步观测：`novovm-network` 的收包路径已把 `Finality::Vote(checkpoint id)` 纳入同步高度观测（同源写入 runtime `highest_block`），并补回归测试，减少 finality 面进度不计入 `eth_syncing` 的盲区。
- 本轮修正 runtime native sync 活跃边界：`phase=idle` 且 `highest==current` 时不再因 `peer_count>0` 被判定为“同步中”，修复 `eth_syncing` 在已追平状态下的假阳性。
- 本轮补通用网络同步状态机自动推进（非 EVM 专属）：`novovm-network` 已新增基于 runtime 进度的 native sync 自动 `reconcile`（`discovery -> headers -> bodies -> state -> finalize -> idle`），并把触发点接到通用运行时写入口（peer 注册/注销、peer head、本地 head、快照注入、block progress），使 `novovm-network` 在主链/插件共用链路下都能自动推进同步 phase，不依赖手工脚本推进。
- 本轮补通用网络高并发发送主路径（非 EVM 专属）：`novovm-network` 的 `TcpTransport` 已从“每条消息新建连接”升级为“连接复用 + 失败自动回收重连”，并在 `unregister_peer` 时同步清理复用连接缓存；该优化直接作用于主链与插件共用网络层发送性能与稳定性，不引入工程化包装层。
- 本轮补公网广播原生回退主路径性能收口：gateway 的 `eth_send* -> native UDP/TCP broadcast` 已改为按 `chain_id + transport + node + listen` 复用 transport 实例（缓存复用、失败可自动重建），不再每笔广播都重新 `bind/register`，减少高频提交下的 socket/连接开销与抖动。
- 本轮补通用发现链路收口：`novovm-network` 收包侧已将 `Gossip::PeerList` 的 payload peers 自动登记到 runtime peer 观测；gateway 原生广播路径新增低频 heartbeat/peerlist 同步（在复用 transport 基础上），用于加速 runtime peer 发现收敛，不改内部二进制主线。
- 本轮补 TCP 临时端口边界收口：`novovm-network` 的 peer 归属推断已在“精确地址不命中”时增加“唯一同 IP peer”安全回退，`Finality::CheckpointPropose/Cert` 在 TCP 短连接场景下也可稳定更新 runtime 同步高度，不再因临时源端口导致同步观测丢失。
- 本轮补 Finality 消息来源去歧义：`novovm-protocol::FinalityMessage::CheckpointPropose/Cert` 已新增显式 `from` 字段，`novovm-network` 运行时同步推进改为优先使用协议来源节点（不依赖地址推断）；`novovm-bench` 全链路发收消息同步到新结构，减少同 IP 多 peer 下的来源错配。
- 本轮补原生网络“实收包”闭环：gateway 在每轮原生广播后会在同一 transport 上小批量 `try_recv` 拉取消息并驱动 runtime 状态更新（peer/sync 观测同源），避免只发不收导致的状态滞后。
- 本轮补只读路径同步收敛：`resolve_gateway_eth_sync_status` 在读取 `eth_syncing/net_peerCount/eth_blockNumber` 前会先轮询链级 native broadcaster 缓存做小批量收包，让“读多写少”场景也能持续由真实网络消息刷新 runtime 同步状态。
- 本轮新增公网广播能力直查接口：`evm_getPublicBroadcastCapability/evm_get_public_broadcast_capability`（含 `ready/required/mode/executor_configured/native_peer_count`），可直接判定当前链是否具备“提交后可广播”的生产闭环能力。
- 本轮把公网广播能力并入总聚合：`evm_getRuntimeFullBundle.runtime_status.public_broadcast` 已输出同源能力快照，补齐 upstream 单请求判断广播门禁的直连消费边界。
- 本轮补 runtime 同步拉取窗口直连：新增 `evm_getRuntimeSyncPullWindow/evm_get_runtime_sync_pull_window`（返回 `phase + from_block/to_block`），并并入 `evm_getRuntimeFullBundle.runtime_status.sync_pull_window`，上游可单请求消费同步拉取边界，无需脚本层拼装。
- 本轮补 native runtime 同步拉取发包：gateway 原生 runtime worker 已按 `plan_network_runtime_sync_pull_window` 直接发 `DistributedOcccGossip(StateSync)` 请求帧（payload: `NSP1 + phase + chain_id + from_block + to_block`），完成“runtime 同步窗口 -> 原生网络发包”直连闭环。
- 本轮补 `novovm-network` 同步拉取收包闭环：收包侧已消费 `StateSync(NSP1)` 拉取请求并自动回包 `block_header_wire_v1`（`StateSync`），同时把请求 `to_block` 写入远端链高提示，形成“请求/回包/runtime 更新”同源生产链路。
- 本轮补 gateway 直连消费单入口：新增 `evm_getRuntimeConsumerBundle/evm_get_runtime_consumer_bundle`，一次返回 `runtime(syncing/peer/block/broadcast/native_sync) + runtime_bundle + tx_status_bundle + event_bundle`，并附 `ready/ready_details/counts`（`public_broadcast_ready/tx_status_ready/events_ready`）。上游无需多次拼装 logs/receipt-error/public-broadcast 结果，即可直接消费闭环状态。
- 本轮补 `eth_getProof` 生产复验入口：新增 `evm_verifyProof/evm_verify_proof`，同请求参数（`chain_id/address/storage_keys/block`）下可对外部提交的 `proof` 做同源状态视图复验，返回 `valid + mismatch_fields`（含 `address/accountProof/balance/codeHash/nonce/storageHash/storageProof` 维度）用于快速判定 proof 是否被篡改。
- 本轮把 `evm_verifyProof` 从“JSON 对比校验”收口为“真实 MPT 证明校验”：账户 proof 与 storage proof 均按 trie RLP 节点路径 + 根哈希 + 目标值语义验证，`valid` 结果不再依赖完整对象字符串对齐。
- 本轮补 `type0/type1/type2` 状态读同源语义：`gateway` 状态视图已去除 `tx_type==2` 硬限制，合约创建识别统一按 `to==null && input!=empty`；合约调用写入投影不再只认 `type1`。`eth_getCode/eth_getStorageAt/eth_getProof/stateRoot` 在 `legacy(type0)` 场景已通过回归锁定。

- 本轮补通用网络发送侧同步观测：`novovm-network` 在发送 `Pacemaker(ViewSync/NewView)`、`StateSync(block header wire)`、`Finality::Vote` 后会按消息高度单调更新 runtime 本地链高（`observe_local_head_max`），使同步状态不再只依赖收包/索引刷新。
- 本轮继续补通用网络同步观测覆盖面：`novovm-network` 收包/发包路径对 `DistributedOcccGossip` 已从仅 `StateSync` 扩展到“任意可解码 `block_header_wire_v1` payload”（含 `ShardState`），并把 `Finality::CheckpointPropose/Cert` 纳入本地高度推进，减少不同消息类型导致的同步滞后。
- 本轮修正原生广播发送顺序：`execute_gateway_eth_public_broadcast_native` 固定先发 `TxProposal`，后发低频 `heartbeat/peerlist` 发现消息，避免交易首包被心跳抢占。
- 本轮补 `receiptsRoot` 同源收口：区块对象的 `receiptsRoot` 已切到按真实回执语义计算（含 `pending/confirmed`、`cumulativeGasUsed`、`effectiveGasPrice`、`contractAddress`），不再仅按交易字段算根；并新增回归锁定 pending/confirmed 根值差异语义。
- 本轮补 `contractAddress` 原生语义收口：EVM 合约地址派生已改为 `keccak(rlp([sender, nonce]))[12..]`，并同步作用于 `receipt/state-view` 查询面（无效 `from` 长度保留安全降级路径），减少与 geth 地址派生口径偏差。
- 本轮补 `StateSync` 下载推进语义：`novovm-network` 收包侧在解析到 `DistributedOcccGossip(msg_type=StateSync)` 且 payload 可解码 `block_header_wire_v1` 时，会同时推进 `peer head + local head`（`observe_network_runtime_local_head_max`），让 `eth_syncing.currentBlock` 能直接跟随真实同步消息推进，不再只能依赖本地索引写入。
- 本轮补读路径自动拉起 native runtime：gateway 在读取 `eth_syncing/net_peerCount/eth_blockNumber` 前会对已配置 native peer 的链自动执行 runtime 初始化（创建/复用 broadcaster、注册 peers、启动 worker、drain 收包）；同步状态不再要求“先执行一次交易广播”才被真实网络消息驱动。
- 本轮收口 `eth_syncing` pending 边界语义：`gateway_eth_syncing_json` 不再因为“仅存在 pending block”强制把 `highestBlock` 抬到 `currentBlock+1`；当链头已追平且 pending 仅为陈旧/同高视图时返回 `false`，只在真实 `highest > current` 的下载场景返回同步对象，语义与原生 geth 对齐。
- 本轮补 runtime 同步目标高度稳定性：`novovm-network` 在 `register/unregister/observe_peer_head` 时不再清空 `native_remote_best`，已知远端最高高度会作为同步 gap 提示持续保留，避免 peer 注册/注销瞬间把 `highest` 错误压回 `current` 引发 `eth_syncing` 抖动；并新增回归 `peer_register_unregister_keeps_native_remote_best_gap_hint` 锁定语义。
- 本轮补 runtime native 快照新鲜度收口：`novovm-network` 已给 native snapshot（`native_peer_count/native_remote_best`）增加超时清理（复用 runtime peer stale 窗口）；当无新快照持续输入时会自动回收旧提示，避免 `eth_syncing` 被历史高点长期卡在同步态。并新增回归 `stale_native_snapshot_hint_is_pruned_on_recompute`。
- 本轮补 `ShardState` 收包本地推进边界：`novovm-network` 在 `DistributedOcccGossip(msg_type=ShardState)` 且 payload 可解码 `block_header_wire_v1` 时，已与 `StateSync` 同步执行本地链高推进（`observe_network_runtime_local_head_max`），避免“只收 shard-state 不发 state-sync”场景下 `eth_syncing.currentBlock` 滞后；并新增回归 `runtime_sync_receive_path_treats_shard_state_as_local_progress`。
- 本轮补状态查询语义收口：`eth_getStorageAt(slot=0)` 与 `eth_getProof.storageProof(slot=0)` 的部署 code-hash 已统一为 `keccak256(code)`（不再使用 `sha256(code)`），与 `eth_getProof.codeHash` 同源；回归 `eth_get_code_storage_and_call_read_path_use_tx_index_state` 已更新并通过。
- 本轮补区块根叶子语义收口：`transactionsRoot/receiptsRoot` 的叶子已改为 `keccak(rlp(payload))`、父节点改为 `keccak(left||right)`，移除网关私有 domain 分隔符（`*_leaf_v1/*_parent_v1` 前缀）；并保持 `pending/confirmed` 的 receipt status 差异能稳定反映到 `receiptsRoot`。
- 本轮补 proof/stateRoot 叶子语义收口：`eth_getProof/stateRoot` 的 account/storage 叶子已改为 RLP 载荷 `keccak(rlp(payload))`，Merkle 父节点改为 `keccak(left||right)`，移除 proof 树私有前缀哈希（`novovm_gateway_*`），减少与原生 EVM 证明口径偏差。
- 本轮补插件 FFI 边界加固：`novovm-adapter-evm-plugin` 的 `decode_plugin_apply_inputs` 已切到受限 bincode 解码（`with_limit(MAX_PLUGIN_TX_IR_BYTES)`），并把 `SizeLimit` 映射为稳定返回码 `RC_PAYLOAD_TOO_LARGE(-8)`；同时 bincode 导出侧（drain/snapshot）也增加统一 payload 上限保护，超限直接 `RC_PAYLOAD_TOO_LARGE`。已补回归覆盖异常大长度前缀输入与导出超限输入，保证异常 payload 硬失败且不进入执行主线。
- 本轮补 AOEM FFI owned-buffer 边界：`aoem-bindings` 新增 `MAX_AOEM_OWNED_BUFFER_BYTES=64MiB` 硬上限，`copy_aoem_owned_bytes` 在超限时会先 `aoem_free` 再拒绝，防止异常长度触发超大内存占用；并允许 `ptr=null && len=0` 作为空返回语义，减少空载荷误判。
- 本轮补 `32-byte slot key` 同源收口：`eth_getStorageAt/eth_getProof` 的 storage key 参数解析、读取和 proof 构造已统一到完整 `[u8;32]` 键空间，网关内部不再先降级为 `u128` 再回查；同时 storage item 内部模型改为 `([u8;32], value)`，proof/value 匹配改为精确键匹配，避免高位 key 截断语义偏差。
- 本轮补 `eth_call/eth_estimateGas` 生产边界：`eth_call` 增加 `from/to` 长度校验、`value` 余额约束、`to=null` 稳定返回与常见 ERC20 selector（`balanceOf/totalSupply/decimals/allowance`）同源读取；`eth_estimateGas` 改为统一 `estimate_intrinsic_gas_with_access_list_m0`，并补“目标有代码 + calldata 非空”执行附加估算、`gas` 上限拒绝（`required gas exceeds allowance`）和 `value` 余额校验。对应回归 `eth_estimate_gas_contract_call_adds_exec_surcharge_and_respects_gas_cap` 已通过。
- 本轮补 gateway 直连消费可用性收口：`evm_getRuntimeConsumerBundle` 由“只返回聚合 bundle”升级为“可直接消费输出”，新增 `consumer.public_broadcast/consumer.tx_status/consumer.events` 与 `unresolved` 明细（按 tx_hash 列出 lifecycle/receipt/broadcast 未解项）；并将 `ready` 改为严格语义（必须公网广播就绪 + tx_status 三类已解 + 事件数组可读），消除上游“假就绪”风险。
- 本轮补回执日志语义收口：`eth_getTransactionReceipt` 与 `eth_getBlockReceipts` 已统一输出最小日志项（`address=to|from`、`topic0=tx_hash`、`data=input`），并基于地址+topic 生成真实 2048-bit `logsBloom`；pending/confirmed 路径保持同源字段语义。
- 本轮修复 `NSP1` 大窗口续拉停滞：`novovm-network` 对窗口 in-flight 目标改为按单次响应上限截断后的 `to_block`（不再盲等原请求 `to_block`），避免“响应批上限 < 请求窗口”时续拉不触发。
- 本轮补 native 同步拉取失效恢复：gateway 同窗超时重发改为分级扇出（`1 -> 2 -> all peers`），并保持 runtime 链高优先顺序，减少单 peer 故障对下载推进的阻塞。
- 本轮补 native 同步拉取即时恢复：同窗发包若本轮全部失败，会立即清空 worker 窗口去重状态，下一轮直接重发（不等待超时门限），减少网络抖动期空转。
- 本轮补 native 发现流量节流：runtime worker 的 `heartbeat/peerlist` 广播已改为按固定间隔下发，不再每 tick 全量广播，降低内部噪声和无效占用。
- 本轮补同步 phase 消息分流实装：gateway 按 `phase` 发送 `StateSync/ShardState`（`headers -> StateSync`，`bodies/state/finalize -> ShardState`）；`novovm-network` 的 `NSP1` 请求识别、回包和自动续拉已统一支持这两条通道并保持同通道回传。
- 本轮补 `NSP1 phase` 真消费：`novovm-network` transport 已解析并使用 `phase` 字节，inflight 目标裁剪与回包批次上限按 phase 分级处理（headers/bodies/state/finalize），不再只按单一固定批次处理所有同步阶段。
- 本轮补原生 discovery 触发收口：gateway native worker 的 `heartbeat/peerlist` 改为按 runtime 状态触发（无 peer / 同步中低 peer），并在同步拉取全失败时立即重置触发下一轮发现，减少空转并提高断链恢复速度。
- 本轮补 MPT 主线收口：`transactionsRoot/receiptsRoot` 已从自定义二叉 Merkle 切到 hexary Patricia trie 根（`key=rlp(tx_index)`，`value=tx/receipt payload`），不再使用网关私有树根算法。
- 本轮补状态证明主线收口：`stateRoot/storageRoot` 与 `eth_getProof(accountProof/storageProof)` 已切到 trie 节点语义（proof 节点为路径 RLP node 序列），替换旧的 sibling-hash 证明结构。
- 本轮补 txpool 重复交易幂等收口：`novovm-adapter-evm-plugin` 入池主循环已在生产路径实现“完全同交易字段重复提交 -> 幂等 accepted（不重复入池）”；并保持替换语义不变（低费替换仍 `replacement_underpriced`，满足 bump 的替换仍可生效），对应回归 `ingress_txpool_duplicate_tx_is_idempotent_accepted` 已通过。
- 本轮补 `tx status` 直接消费收口：`evm_getTransactionLifecycle` 与 `evm_getRuntimeTxStatusBundle` 增加扁平语义字段 `stage/terminal/failed + receipt_pending/receipt_status + broadcast_mode`；`evm_getRuntimeConsumerBundle` 同步新增 `stage_resolved/stage_unresolved`、`unresolved.stage_tx_hashes` 与 `ready_details.tx_status_stage_ready`，上游可直接判定提交阶段与失败类型，不再做三路对象二次归并。
- 本轮补 `pending atomic-broadcast` 自动恢复收口：gateway 已接入“启动即自动回放 + 每次请求后自动回放”生产路径；自动回放参数固定为 `NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_AUTOREPLAY_MAX`、`NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_AUTOREPLAY_COOLDOWN_MS`、`NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_PENDING_WARN_THRESHOLD`、`NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_AUTOREPLAY_USE_EXTERNAL_EXECUTOR`，成功会清理 pending ticket/payload 并置 `broadcasted_v1`，失败保留 pending ticket 并置 `broadcast_failed_v1`。
- 本轮补 `pending atomic-ready` 自动恢复收口：gateway 已接入 pending atomic-ready 批量恢复（RocksDB 扫描同源前缀），自动把 `compensate_pending_v1` 记录恢复为 `atomic-ready spool + queue + pending ticket/payload`，并由现有 pending atomic-broadcast 自动回放链路继续执行；重启后可自动收敛，不再依赖人工逐条 `evm_replayAtomicReady`。
- 本轮补 `pending public-broadcast` 自动恢复收口：gateway 已接入公网广播 pending 自动回放（启动与请求后双触发），会从链索引中扫描 `broadcast_status=none|missing` 的交易并自动重试广播；参数固定为 `NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_AUTOREPLAY_MAX`、`NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_AUTOREPLAY_COOLDOWN_MS`、`NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_PENDING_WARN_THRESHOLD`，并保持现有外部执行器/原生回退主路径语义不变。
- 本轮补 `pending public-broadcast` 显式队列收口：gateway 已增加 pending ticket 持久化主路径（`gateway:eth:public_broadcast:pending:v1:*`）；写路径统一在广播状态更新时自动维护（`none` 入队，`external/native` 清队），自动回放优先消费 pending ticket，并对旧 `broadcast_status=none` 记录做兼容回填，减少每轮全链扫描开销。
- 本轮补 `eth_sendTransaction` 哈希推导收口：`GatewayEthTxHashInput/compute_gateway_eth_tx_hash` 已纳入 `maxPriorityFeePerGas`，并在 `infer_gateway_eth_send_tx_hash_from_params` 与主写路径统一传入；不同 priority fee 的发送请求不再哈希碰撞，失败状态回写与重试归因更稳定。
- 本轮补链族映射配置化收口：`novovm-adapter-evm-core` 新增统一链族解析入口 `resolve_evm_chain_type_from_chain_id`，并支持 `NOVOVM_EVM_CHAIN_TYPE_OVERRIDES`（`chain_id=chain_type`）覆盖；`evm-plugin/gateway` 已统一复用该入口，不再各自维护硬编码链族分发。
- 本轮补 type3(blob) 写路径开关收口：`novovm-adapter-evm-core` 已接入 type3 路由与字段翻译（按 EIP-4844 前段字段解析），默认保持关闭；设置 `NOVOVM_EVM_ENABLE_TYPE3_WRITE=1` 后启用主线写入路径，保持“外部 EVM 语义、内部二进制流水线”不变。
- 本轮补 type3(blob) 费用与校验语义收口：`novovm-adapter-evm-core` 已新增 `max_fee_per_blob_gas/blob_hash_count` 解析与校验，新增 blob intrinsic 费用函数（`estimate_blob_intrinsic_extra_gas_m0`）并并入统一 intrinsic 估算（`estimate_intrinsic_gas_with_envelope_extras_m0`）；`validate_tx_semantics_m0` 现会按签名 envelope 执行 type3 校验（`max_fee_per_blob_gas>0`、`blob_hash_count>0`），并在 type4 场景按 profile policy 拒绝。
- 本轮补 type3(blob) gateway 主写路径收口：`eth_estimateGas`、`eth_sendTransaction`、`infer_gateway_eth_send_tx_hash_from_params` 已统一解析 `maxFeePerBlobGas/blobVersionedHashes`，tx_type 推断统一改为 `resolve_gateway_eth_write_tx_type`，并把 intrinsic 估算统一切到 `estimate_intrinsic_gas_with_envelope_extras_m0`（含 blob gas）；`GatewayEthTxHashInput` 已纳入 blob 费用与数量字段，避免状态推导与提交主路径语义漂移。
- 本轮补 `eth_sendRawTransaction` 语义前置收口：gateway 在 raw 写入 `.opsw1` 前已接入 `resolve_evm_profile + validate_tx_semantics_m0` 同源校验（intrinsic/type3/type4 与核心一致），防止无效 raw 交易先入主线后再被插件拒绝；新增回归 `main_tests::eth_send_raw_transaction_rejects_intrinsic_gas_too_low`。
- 本轮补 London 费用语义收口：`gateway_eth_effective_gas_price_wei` 对 type2/type3 已切为 `max(baseFee, min(maxFeePerGas, baseFee+priorityFee))`；并在 `eth_sendTransaction/eth_sendRawTransaction` 写入口新增 `maxFeePerGas < baseFeePerGas` 直接拒绝，保证 EIP-1559 类交易不会在明显无效费用下进入 `.opsw1` 主线。新增回归：`main_tests::eth_effective_gas_price_type2_respects_base_fee_floor`、`main_tests::eth_send_transaction_rejects_type2_max_fee_below_base_fee`、`main_tests::eth_send_raw_transaction_rejects_type2_max_fee_below_base_fee`。
- 本轮补 `eth_estimateGas` 的 EIP-1559 费用约束：type2/type3 场景新增 `maxPriorityFeePerGas > maxFeePerGas` 拒绝与 `maxFeePerGas < baseFeePerGas` 拒绝，语义与 `eth_sendTransaction/eth_sendRawTransaction` 同源一致，防止估算接口接受无效费用组合。新增回归：`main_tests::eth_estimate_gas_rejects_type2_priority_fee_above_max_fee`、`main_tests::eth_estimate_gas_rejects_type2_max_fee_below_base_fee`。
- 本轮补链级费率覆盖同源收口：`NOVOVM_GATEWAY_ETH_DEFAULT_{GAS_PRICE|BASE_FEE_PER_GAS|MAX_PRIORITY_FEE_PER_GAS}` 已统一支持按链覆盖（`*_CHAIN_{id}` / `*_CHAIN_0x{id}`）；`eth_maxPriorityFeePerGas`、`eth_feeHistory(baseFee/reward)`、`eth_estimateGas`、`eth_sendTransaction/eth_sendRawTransaction`、`effectiveGasPrice/receipt`、`block.baseFeePerGas` 已统一消费同一链级费率源，避免多链节点下 fee 语义串链。新增回归：`main_tests::gateway_eth_chain_fee_env_overrides_apply_to_helpers`、`main_tests::eth_fee_endpoints_use_chain_scoped_fee_overrides`。
- 本轮补 fork 写路径链级开关收口：`type1/type2/type3` 写入能力均支持链级覆盖（`NOVOVM_EVM_ENABLE_TYPE{1,2,3}_WRITE_CHAIN_{id}` / `NOVOVM_EVM_ENABLE_TYPE{1,2,3}_WRITE_CHAIN_0x{id}`）；`novovm-adapter-evm-core` 与 `evm-gateway` 已统一同源校验并保持默认语义（type1/type2 默认开，type3 默认关）。新增回归：`main_tests::resolve_gateway_eth_write_tx_type_respects_chain_scoped_type1_toggle`、`main_tests::resolve_gateway_eth_write_tx_type_respects_chain_scoped_type2_toggle`。
- 本轮补 fork 激活高度主写语义收口：`eth_estimateGas`、`eth_sendTransaction`、`eth_sendRawTransaction` 在落 `.opsw1` 前统一按 `pending block` 口径执行 `London/Cancun` 激活校验（支持 `NOVOVM_GATEWAY_ETH_FORK_{LONDON|CANCUN}_BLOCK_CHAIN_{id|0xid}`）；未激活时直接拒绝 type2/type3，避免无效交易进入主线。新增回归：`main_tests::eth_estimate_gas_rejects_type2_when_london_not_active`、`main_tests::eth_send_transaction_rejects_type2_when_london_not_active`、`main_tests::eth_send_transaction_rejects_type3_when_cancun_not_active`。
- 本轮补链级 `0xHEX` 环境键大小写兼容：`gateway_eth_chain_u64_env/gateway_eth_chain_bool_env` 已支持 `..._CHAIN_0x{lower}` 与 `..._CHAIN_0x{UPPER}` 双口径（例如 `0xA86A`），避免含 `A-F` 链号在大写键名下不生效；对应 fork 激活高度、链级费率覆盖与 type2 开关均可直接消费。新增回归：`main_tests::resolve_gateway_eth_write_tx_type_respects_upper_hex_chain_scoped_type2_toggle`、`main_tests::gateway_eth_chain_fee_env_upper_hex_overrides_apply_to_helpers`、`main_tests::eth_send_transaction_rejects_type2_when_london_not_active_with_upper_hex_chain_key`。
- 本轮补 raw 主写路径同源校验：`eth_sendRawTransaction` 已在写入前接入 `resolve_gateway_eth_write_tx_type`，raw 路径与非 raw 路径统一受 `type1/type2/type3` 链级写开关与 blob 字段约束；同时补齐 raw 的 fork 激活边界（type2/London、type3/Cancun）回归。新增回归：`main_tests::eth_send_raw_transaction_rejects_type2_when_write_path_disabled`、`main_tests::eth_send_raw_transaction_rejects_type2_when_london_not_active`、`main_tests::eth_send_raw_transaction_rejects_type3_when_cancun_not_active`。
- 本轮补 raw `tx_type` 参数一致性：`eth_sendRawTransaction` 在传入显式 `tx_type/type` 时，已强校验必须与 raw envelope 推断类型一致；不一致直接拒绝，避免参数层和原文交易层语义漂移。新增回归：`main_tests::eth_send_raw_transaction_rejects_explicit_tx_type_mismatch`、`main_tests::eth_send_raw_transaction_accepts_matching_explicit_tx_type`。
- 本轮补写入口链参数一致性：`eth_estimateGas`、`eth_sendTransaction`、`eth_sendRawTransaction` 已统一接入 `chain_id/chainId/tx.chain_id/tx.chainId` 同值硬校验；多位置参数不一致时直接拒绝（不再静默取首值），避免错链提交进入主线。新增回归：`main_tests::eth_estimate_gas_rejects_chain_id_mismatch_between_top_level_and_tx`、`main_tests::eth_send_transaction_rejects_chain_id_mismatch_between_top_level_and_tx`。
- 本轮补失败状态哈希推导链参数一致性：`infer_gateway_eth_tx_hash_from_write_params` 与 `infer_gateway_eth_send_tx_hash_from_params` 已接入同源 `chain_id` 一致性校验；当链参数不一致或 raw 推断链与显式链冲突时不再推导哈希，避免 `submit_status` 误写到错误 tx_hash。新增回归：`main_tests::infer_gateway_eth_tx_hash_from_write_params_returns_none_on_chain_id_mismatch`、`main_tests::infer_gateway_eth_send_tx_hash_from_params_returns_none_on_chain_id_mismatch`。
- 本轮补动态费单源费用口径：`eth_sendTransaction` 在 `tx_type=2/3` 下已统一使用 `maxFeePerGas` 作为 `gas_price` 进入索引与哈希，`maxPriorityFeePerGas` 走同源默认与校验；`infer_gateway_eth_send_tx_hash_from_params` 同步采用该口径，修复 `gasPrice + maxFeePerGas` 并存时主写与失败状态哈希可能漂移的问题。新增回归：`main_tests::eth_send_transaction_type2_hash_and_index_use_max_fee_per_gas`、`main_tests::infer_gateway_eth_send_tx_hash_from_params_type2_uses_max_fee_for_gas_price`。
- 本轮补失败状态哈希推导费用边界同源：`infer_gateway_eth_send_tx_hash_from_params` 已补 `maxFeePerGas >= baseFeePerGas` 约束，避免主写路径会拒绝的动态费请求被推导出“伪 tx_hash”并写入失败状态；同时补齐 type1 链级写开关 `0xHEX` 大写键路径回归，确保 `NOVOVM_EVM_ENABLE_TYPE1_WRITE_CHAIN_0xA86A` 与其它链级键同源生效。
- 本轮补 Amsterdam/EIP-7954 上限语义：gateway 已新增 `NOVOVM_GATEWAY_ETH_FORK_AMSTERDAM_BLOCK(_CHAIN_{id|0xid})` 并在 `eth_estimateGas/eth_sendTransaction/eth_sendRawTransaction` 三条写路径统一执行 deploy initcode 上限校验（Amsterdam 前 `49_152`，Amsterdam 后 `65_536`）；`novovm-adapter-evm-core` 的通用 deploy 上限同步提升到 `65_536`，避免对 Amsterdam 激活链误拒绝。
- 本轮补 `submit-status` 成功路径收口：`eth_sendRawTransaction/eth_sendTransaction` 成功提交后不再删除 submit-status，而是持久化 `accepted=true,pending=true,onchain=false`；`evm_getTxSubmitStatus/evm_getTransactionLifecycle` 在“tx 索引缺失但 submit-status 存在”场景改为按状态推导阶段（`pending/accepted/onchain/failed/rejected`），不再固定误报 `failed`。新增回归：`main_tests::evm_get_tx_submit_status_uses_persisted_success_status_when_tx_missing`。
- 本轮补 `submit-status` 的 `onchain_failed` 终态收口：当生命周期查询命中回执 `status=0x0` 时，submit-status 会持久化 `error_code=ONCHAIN_FAILED/error_reason=transaction failed onchain`；后续即使 tx 索引缺失，`evm_getTxSubmitStatus` 仍可返回 `stage=onchain_failed, failed=true, terminal=true`，避免退化为普通 `onchain`。新增回归：`main_tests::evm_get_tx_submit_status_uses_persisted_onchain_failed_status_when_tx_missing`。
- 本轮完成本地全量验收闭环：`fmt --check`、`clippy --workspace -D warnings`、`novovm-consensus/novovm-network` 单包 clippy、`novovm-evm-gateway(162/162)`、`novovm-adapter-evm-plugin(29/29)` 全通过；EVM 全镜像收口进度提升到 `100%`（按当前清单口径）。

## 4. 下一阶段（按优先级）

1. 把 EVM 插件从“适配能力”推进到“全功能镜像节点能力”（网络同步、txpool、状态查询完整面）。
2. 完成收益归集 -> 换汇 -> 节点账户发放的生产策略闭环（可审计、可回放、可对账）。
3. 完成原子跨链 intent 的广播门控闭环（本地检查通过才广播）。
4. 持续压缩非必要日志与非必要包装，保持主线极简与性能优先。

## 5. 历史工程化记录处理

- 历史 `gate/signal/snapshot/rc` 记录不再作为本台账内容。
- 若需追溯历史脚本证据，统一视为“归档资料”，不代表当前主线完成度。

## 6. 台账更新规则

- 每次更新必须对应到“生产代码接线变化”与“可复现实跑路径”。
- 不再新增“仅脚本/仅观测”类型里程碑。
- 任何影响性能的附加层必须先证明必要性，否则不进入主线。
- 若出现与“边界/内部分层”或“极致静默”冲突的代码，按铁律优先级立即修正，不得回退。

## 7. 高性能流水线专项台账（不改协议/共识/规则）

- 专项目标：在保持 EVM 外部语义等价的前提下，把插件寄宿执行路径改造成 superVM 高并发流水线。
- 约束：不修改 `novovm-node` 公共方法；不引入新的工程化包装层；默认极致静默。

### HP-L1 插件执行并发预处理（已完成）

- 范围：`crates/plugins/evm/plugin/src/lib.rs`
- 改造口径：并发预校验，串行提交状态（确保确定性）。
- 当前状态：`Done`
- 当前进度：`100%`
- 本轮已完成：
  - `apply_ir_batch` 已改为并发语义预校验（按批分片并行）后再串行执行 `verify + execute`。
  - `apply_ir_batch` 已补“并发 verify + 串行 execute”主路径：批内 `verify_transaction` 分片并发执行，状态提交仍按输入顺序串行 `execute_transaction`，确定性不变。
  - `apply_ir_batch` 已把“并发语义校验 + 并发验签”收口为单次并发扫描：每个分片内先做 `validate_tx_semantics_m0` 再做 `verify_transaction`，去掉独立语义扫描阶段，减少整批遍历与线程调度开销，执行顺序与协议语义不变。
  - 插件新增“单批一次 hash 预处理并复用”热路径：`prepare_txs_with_hashes`（大批量分片并行，小批量串行）统一供 `runtime_tap/apply_v1/apply_v2` 复用，避免同批交易在 ingress/atomic/apply/settlement 多次重复 `ensure_tx_hash`。
  - `build_atomic_intent_id` 与结算入口已改为优先复用已存在 tx hash，仅在缺失时按需补算。
  - `novovm-adapter-novovm` 已落地“单次验签缓存”路径：同一 tx 在“先 `verify_transaction` 再 `execute_transaction`”主线上不再重复完整验签；`execute` 仅在缓存未命中时回退原验证逻辑，语义保持不变。
  - `novovm-adapter-novovm` 的验签缓存已从 `Mutex<HashSet>` 收口为 `DashSet` 并发集合：并发 `verify_transaction` 阶段不再争用单把全局锁，`execute_transaction` 仍按 hash 移除命中缓存并保持原语义。
  - `novovm-adapter-evm-plugin` 的 `apply_ir_batch` 已去掉 `Box<dyn ChainAdapter>` 热路径动态分发，改为直接使用 `NovoVmAdapter` 具体实现；`initialize/verify/execute/state_root/shutdown` 全链路保持原语义，仅减少虚调用开销。
  - `aoem-bindings` 已补 `aoem_ed25519_verify_v1/aoem_ed25519_verify_batch_v1` 直连；`novovm-adapter-novovm` 的 `verify_transaction/verify_block` 与插件 `apply_ir_batch` 已切到“AOEM 单验签/批验签优先，缺失时回退本地 dalek”主线，保持签名与 `from` 匹配语义不变。
  - `novovm-adapter-novovm` 在“AOEM batch 不可用”回退分支已从逐笔串行验签改为分片并发验签（仍复用 `verify_tx_signature_v1` 同语义），确保 FFI 降级场景下吞吐不掉队。
  - `novovm-node` 的 native adapter signal 路径已切到“`verify_block` 批量快路径优先，全通过则直执；失败再逐笔回退判定”，在不改最终通过/拒绝语义前提下降低批内逐笔验签开销。
  - `novovm-node` 的 ingress 预检查热路径（`admit_mempool_basic`、`validate_and_summarize_txs`）已切到“批量并发签名预校验 + 原有顺序 nonce 约束”，保持拒绝/接纳语义不变并降低大批量串行签名计算开销。
  - `novovm-node` 的 `run_ffi_v2` 已去掉 admitted 批次的重复签名校验：`admit_mempool_basic` 通过后，`tx_meta` 汇总改为“fee/nonce 规则复核 + 复用已验证签名前提”，避免同批交易在同一流程重复做签名哈希计算。
  - `novovm-node` 的 `LocalBatch` 已增加预计算 `txs_digest`，`build_batch_state_root` 与 `compute_block_hash` 改为复用批次摘要，去掉同一批次在闭环阶段的重复逐笔交易哈希。
  - `run_ffi_v2` 的批次切分与 AOEM `ExecOpV2` 映射已收口为单次构建（`build_local_batches_and_ops_from_txs`）：同一 admitted 批次不再分别执行“切批遍历 + ops 编码遍历 + mapped_ops 再汇总”三段扫描。
  - `run_ffi_v2` 的 mempool admission 已切到 owned 路径（`admit_mempool_basic_owned`）：对 codec 解码得到的 `Vec<LocalTx>` 直接筛选接纳，不再在 admission 期间逐笔 clone 交易对象。
  - `novovm-node` 的 `build_local_batches_from_txs` 与 `encode_ops_v2_buffer` 已收口为对 `build_local_batches_and_ops_from_txs` 的薄包装，去除旧双实现并避免后续逻辑漂移。
  - `run_ffi_v2` 已改为“先 UA/adapter 判定，再消费 admitted 向量进入 owned 批次构建”（`build_local_batches_and_ops_from_txs_owned`），批次阶段不再复制交易对象。
  - `run_ffi_v2 -> batch_a` 已切到 owned 闭环：`run_batch_a_minimal_closure` 按值接收 `Vec<LocalBatch>` 并在 `build_local_block_owned` 直接消费，不再为 block 构建额外 clone 一份 batch 向量。
  - `run_batch_a_minimal_closure` 内部 `layout/mapped_ops/expected_txs` 统计已收口为单次预计算复用，减少闭环阶段重复遍历。
  - `build_local_batches_and_ops_from_txs(_owned)` 已把 `txs_digest` 计算并入现有构建循环，不再为每个 batch 单独二次扫描交易计算摘要。
  - owned 批次构建已从 `split_off` 尾向量搬移改为迭代消费（按批次大小顺序取 tx），减少批次切分阶段的额外向量重排开销。
  - `verify_local_tx_signatures_batch` 的并行聚合已从“线程返回 `(idx,bool)` 元组列表”收口为“线程返回连续 `Vec<bool>` + 主线程按切片回填”，减少并行验签阶段的中间分配与索引搬运开销。
  - `aoem-bindings` 已补 `aoem_secp256k1_verify_v1/aoem_secp256k1_recover_pubkey_v1` 直连；`novovm-adapter-evm-core` 已下沉 raw sender recovery，gateway `eth_sendRawTransaction` 现在优先从 raw 原文恢复 sender，显式 `from` 改为一致性约束或兼容回退。
  - `eth_sendTransaction` 已接入同源 recoverable-signature sender 校验：若请求携带 `signature/raw_signature/signed_tx` 且可被解析为 recoverable raw EVM tx，则 gateway 会按 AOEM `secp256k1 recover` 恢复 sender 并强校验与显式 `from` 一致；不可恢复时保持原宿主写入语义不变。
  - 同一 recoverable-signature 路径已继续补齐字段强一致：`nonce/to/value/gas/gasPrice|maxFeePerGas|maxPriorityFeePerGas/data/accessList/blob` 现按有效字段与恢复出的 raw tx 主体逐项比对，recoverable `signed_tx` 不再允许和显式 `eth_sendTransaction` 请求体脱节。
  - `apply_ir_batch` 已从“分片内逐笔本地验签”切到“并发语义预校验 + 单批 AOEM ed25519 batch verify + 串行 execute”主线；隐私交易仍走原隐私验签口径，协议/共识/回执语义不变。
  - `novovm-adapter-novovm` 已落地“状态根脏标记 + 按需刷新缓存”路径：`execute_transaction/write_state/delete_state` 仅标记 dirty，不再每笔全量重算状态根；`state_root()` 查询时再统一刷新并回写缓存，保持外部语义不变。
  - `apply_ir_batch` 已补错误路径资源收口：即使中途验证/执行失败，也会执行 `adapter.shutdown()`，避免异常路径资源悬挂。
  - 状态提交顺序与回执语义保持不变（未改变协议/共识/规则）。
  - 本地验收通过：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`novovm-adapter-novovm test 13/13`、`novovm-adapter-novovm clippy -D warnings`、`novovm-adapter-evm-plugin test 29/29`、`novovm-evm-gateway 关键回归（eth_feeHistory）通过`、`novovm-network test 47/47`。

### HP-L2 插件运行态锁分片（已完成）

- 范围：`crates/plugins/evm/plugin/src/lib.rs`
- 改造口径：按 `chain_id/sender` 分片，去全局大锁热点。
- 当前状态：`Done`
- 当前进度：`100%`
- 本轮已完成：
  - 插件运行态已拆分为“结算/回执状态锁”与“txpool 热路径独立锁”。
  - `push_ingress_frames`、`drain/snapshot ingress*`、`sender bucket snapshot` 已切到 txpool 独立锁，不再与结算/回执路径共享同一把全局锁。
  - txpool 已按 `chain_id` 进一步分片为多锁（`EVM_TXPOOL_SHARDS`）；同机多链并发写入不再争用单一 txpool 锁，跨链 drain/snapshot 由宿主读口按分片聚合。
  - txpool 分片数已支持环境变量调优（`NOVOVM_ADAPTER_PLUGIN_EVM_TXPOOL_SHARDS`，默认 16，上限 128）；host 侧 `drain/snapshot` 读口已改为分片起点游标轮转，降低固定分片热点。
  - ingress 主入口已拆成 `push_ingress_frames_prepared`，直接消费预处理后的 tx 批；`runtime_tap/apply_v1/apply_v2` 改为共享同一份预处理结果后再进 txpool，缩短 txpool 锁内重复计算路径。
  - txpool ingress 写入口已从“按 chain 单分片锁”推进为“按 `chain_id + sender` 选分片锁”，同链多 sender 并发写入不再争用同一把分片锁。
  - 在 sender 分片后补齐语义收口：`drain_plugin_ingress_frames_for_host` 的 pending 段改为“跨分片全局 sender round-robin”出队，保持原有队列公平语义（不会因分片导致 sender 顺序漂移）。
  - `snapshot_pending_sender_buckets_for_host` 已改为“按 sender 所在分片定向取数”：先按分片聚合所需 tx hash，再仅扫描对应分片 ingress，去除全分片二次扫描热点。
  - 结算/回执运行态已按链分片：`runtime shard`（`NOVOVM_ADAPTER_PLUGIN_EVM_RUNTIME_SHARDS`，默认 16，上限 128），`settlement/atomic-receipt/atomic-ready` 写入改为按 `chain_id` 命中分片锁，去除单全局运行态锁热点。
  - host 侧 `drain_atomic_receipts/drain_settlement_records/drain_payout_instructions/drain_atomic_broadcast_ready` 已改为分片轮转聚合；`settlement totals/snapshot` 改为跨分片汇总；`settlement_seq` 保持全局单调序列。
  - txpool 语义保持不变（替换规则、nonce-gap、容量约束、round-robin drain）；本地验收通过：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`novovm-adapter-evm-plugin test 29/29`、`novovm-evm-gateway test 163/163`、`novovm-network test 48/48`。

### HP-L3 gateway 边界并发化（已完成）

- 范围：`crates/gateways/evm-gateway/src/main.rs`
- 改造口径：只改边界处理并发，不改对外 JSON-RPC 语义。
- 当前状态：`Done`
- 当前进度：`100%`
- 本轮已完成：
  - `evm_getPublicBroadcastStatusBatch` 已从串行递归调用改为“批量并发查询 + 顺序聚合返回”路径。
  - `evm_getTransactionReceiptBatch` 与 `evm_getTransactionByHashBatch` 已改为“批量并发查询 + 顺序聚合返回”路径。
  - `evm_replayPublicBroadcastBatch` 已改为“批量并发回放 + 顺序聚合返回”路径；逐笔失败仍按原结构返回 `replayed=false + error`。
  - `evm_getTransactionLifecycleBatch` 已改为“批量并发查询 + 顺序聚合返回”路径；并与单笔 `evm_getTransactionLifecycle` 统一复用同一生产 helper，保证单批语义一致。
  - `evm_getLogsBatch` 与 `evm_getFilterLogsBatch` 已改为“批量并发查询 + 顺序聚合返回”路径，保持过滤语义与输出顺序兼容。
  - `evm_getRuntimeTxStatusBundle` 已改为“按 tx 并发收口 lifecycle+receipt+broadcast”路径，减少重复批量查询和重复解析开销。
  - `evm_getFilterChangesBatch` 已改为直连生产 `filter` 处理 helper，去掉批量递归分发开销。
  - `evm_getRuntimeEventBundle` 的 `logs/filterChanges/filterLogs` 已改为直连生产 helper，减少重复路由分发与参数重解析开销。
  - `evm_publicSendRawTransactionBatch` 与 `evm_publicSendTransactionBatch` 已去掉批量内递归分发层，直接进入 `eth_send*` 主生产路径（保留 public-broadcast 语义标记）。
  - `evm_getRuntimeTxStatusBundle`、`evm_getRuntimeEventBundle`、`evm_getRuntimeFullBundle` 已完成 helper 直连收口，去掉 bundle 内部递归 `run_gateway_method` 分发层，减少重复路由与参数重解析开销。
  - `evm_getRuntimeConsumerBundle` 已改为直连 `FullBundle helper`，去掉 consumer->fullBundle 递归分发层。
  - 批量查询保持单笔语义：默认链提示、pending 交易视图、chain_id 边界过滤均保持兼容。
  - 批量查询的索引缓存回写改为线程外顺序回写，避免并发路径污染主索引写入。
  - native sync-pull 发送选路已改为“单次 runtime peer-head 快照 + 单次有序候选发送”，去掉同一轮内双次 `select` 与 merge 组装，减少重复排序和集合构建开销。
  - `public broadcast` 交易热路径已去掉“每笔交易附带周期性 discovery 发送”逻辑，统一由后台 native runtime worker 维护 peer 活性，减少单笔交易额外报文开销。
  - native sync-pull tracker 状态已收口为 `phase_tag(u8)`，去掉热路径 phase 字符串分配/比较开销。
  - native broadcaster 已内置 peer 注册缓存（同 peer 同地址跳过重复 `register_peer`），减少每笔广播重复注册造成的锁竞争与系统调用开销。
  - native peers 配置已按 `chain_id` 缓存快照并自动感知 raw 配置变更，`capability/runtime-worker/public-broadcast` 三条路径共享解析结果，去掉重复 split+parse+peer_nodes 构建开销。
  - sync-pull 发送新增 `fanout=1` 快路径，减少单播阶段集合去重与迭代开销。
  - native sync worker 已改为“先 drain 收包、再做 sync window 规划与发送”，窗口调度直接消费最新 runtime 观测，减少滞后窗口重复拉取。
  - sync-pull 发送已改为“单次构建有序 peer 快照后按 fanout 截断发送”，去掉发送函数内部重复选路与去重集合构建，降低热路径分配/遍历开销。
  - sync-pull worker 已接入“同窗按 fanout 分段并发拉取”：同一 `from/to` 窗口按目标并发数切分连续子区间并分发到不同 peer，减少同窗重复请求与单窗串行等待。
  - sync-pull 并发参数已支持链级调优（默认不改变现有行为）：`NOVOVM_GATEWAY_ETH_NATIVE_SYNC_PULL_FANOUT_MAX(_CHAIN_{id|0xid})`、`NOVOVM_GATEWAY_ETH_NATIVE_SYNC_PULL_SEGMENTS_MAX(_CHAIN_{id|0xid})`、`NOVOVM_GATEWAY_ETH_NATIVE_SYNC_PULL_SEGMENT_MIN_BLOCKS(_CHAIN_{id|0xid})`，用于压测场景控制并发窗口数量与单段最小粒度。
  - 本轮补 phase 级并发参数收口：上述 3 组参数均支持按 phase 后缀覆盖（`_HEADERS/_BODIES/_STATE/_FINALIZE`，可叠加 `_CHAIN_{id|0xid}`）；并补 `_CHAIN_0x{id}` 大小写键兼容（`0xA86A` 与 `0xa86a` 都可生效）。
  - 本轮补 sync-pull 重发间隔参数收口：`NOVOVM_GATEWAY_ETH_NATIVE_SYNC_PULL_RESEND_MS` 支持链级+phase 级覆盖（同样支持 `_CHAIN_{id|0xid}` 与 `_HEADERS/_BODIES/_STATE/_FINALIZE` 组合），并做硬钳制 `50..60000ms`，用于压测场景定标而不改变默认语义。
  - 本轮补 sync-pull 热路径选路预算收口：runtime `peer-head Top-K` 查询预算改为“本轮已解析 fanout”，不再按全量 peer 取样，降低高 peer 数场景下每轮窗口规划开销。
  - 新增回归：`split_sync_pull_window_creates_contiguous_ranges`、`split_sync_pull_window_caps_segments_by_window_span`，锁定分段并发窗口语义。
  - 新增回归：`resolve_sync_pull_fanout_respects_requested_and_peer_count_without_cap`，锁定 fanout 解析语义不回退。
  - 新增批量并发 worker 环境参数 `NOVOVM_GATEWAY_BATCH_WORKERS`（`0=自动`），默认自动按 CPU 并发度启用。
  - 对外响应结构与结果顺序保持兼容，不改变 JSON-RPC 语义。
  - 本地验收通过：`novovm-evm-gateway` `clippy -D warnings`、`test 163/163`。

### HP-L4 network 传输与同步状态低竞争化（已完成）

- 范围：`crates/novovm-network/src/transport.rs` + `runtime_status.rs`
- 改造口径：减少阻塞重试和全局锁竞争，保持同步语义同源。
- 当前状态：`Done`
- 当前进度：`100%`
- 本轮已完成：
  - `TcpTransport::send` 连接重试从固定阻塞 sleep 方案改为可配置策略：`NOVOVM_NETWORK_TCP_CONNECT_RETRY_ATTEMPTS`、`NOVOVM_NETWORK_TCP_CONNECT_RETRY_BACKOFF_MS`。
  - `runtime_status` 在 `set_network_runtime_sync_status / set_network_runtime_peer_count / set_network_runtime_block_progress` 中加入“状态未变化短路”，减少重复 reconcile 与锁竞争。
  - `runtime_status` 已把“按 chain_id 再读取 runtime 后 reconcile”的二次锁路径收口为“携带已知 runtime 状态直接 reconcile native 状态”，`set_network_runtime_*` 与 peer/local/snapshot 观测路径统一复用，减少热路径重复锁与重复 map 读取。
  - `transport` 的 `StateSync/ShardState` 收包更新已改为“单次进锁同时更新 peer_head + local_head(max)”路径，替换原先两次独立 runtime 更新调用，降低高频收包时锁竞争。
  - `transport` 收包 runtime 更新已去掉“先 register 再 observe”的双调用模式：`Pacemaker/Finality/可解码 DistributedOccc` 直接单调用 `observe_*`，未知消息才走兜底 `register`，减少每条热消息重复锁与重复重算。
  - `runtime_status` 的 `register_peer / observe_peer_head` 已加入“无状态变化快返”路径：peer/head/local 未变化且无 native hint 清理时直接返回，不触发整轮 recompute + reconcile。
  - `transport` 的 runtime sync pull inflight 目标表已从 `Mutex<HashMap>` 收口为 `DashMap`，并将 followup 目标窗口判断整合为单 helper（未到目标直接等待、到目标立即清理）。
  - `transport` followup 出站去重：去掉“接收侧预 track + send 成功后再 track”的双写，改为“send 主路径 track；仅 fallback 直发时补 track”，减少重复解码与重复写目标表。
  - `transport` UDP/TCP 回包与 followup 发送改为 `send_internal(&ProtocolMessage)`，收包回路不再 clone 消息对象再发送，降低高频回包场景分配/拷贝开销。
  - `transport` sync-pull 回包构建改为“按区间直接流式组消息”，去掉 `Vec<Vec<u8>>` 中间 payload 聚合，降低中间容器分配和二次遍历成本。
  - `runtime_status` 新增 stale 到期时间短路：未到期时跳过 `peer_last_seen/native_snapshot` 全量扫描，到期后再统一清理 stale peer/snapshot，降低高频读写下重复遍历开销。
  - `runtime_status` 新增 `get_network_runtime_peer_heads_top_k`；gateway sync-pull 选路改为按配置 peer 数量按需 Top-K，减少每轮调度全量排序成本。
  - `runtime_status` 读路径新增 dirty/stale 到期重算短路：无脏变化且未到期时直接返回缓存状态，减少 `eth_syncing`/peer-head 轮询场景重复重算。
  - `transport` 收包热路径改为缓冲区复用：`UdpTransport::try_recv`/`TcpTransport::try_recv` 不再每帧分配新 `Vec`，改为复用内部接收缓冲，降低高频收包分配开销。
  - `transport` UDP 发送热路径去重复注册：对同一 peer 改为“首包注册 runtime peer，断连后再注册”，避免每次发包都进入 runtime 注册锁路径。
  - `runtime_status` 在 `observe_network_runtime_local_head/observe_network_runtime_local_head_max/ingest_network_runtime_native_sync_snapshot` 增加“无变化快返”，降低高频上报路径重复重算。
  - `gateway` sync-pull 发送选路已改为“按 fanout 取 Top-K 优先 + 配置顺序失败回退”，减少每轮全量 peer 排序/扫描开销。
  - `gateway` sync-pull tracker 改为已存在窗口原地更新，避免每 tick 重建 `worker_key` 与 state 对象。
  - `transport` pull target 窗口判断收口为单次读取后判定/清理，减少同键双查。
  - `transport` `src -> peer` 归因改为单次遍历（精确地址优先 + 同 IP 唯一回退），降低收包路径遍历开销。
  - `transport` `try_recv` 已改为“消息自带来源 id 时跳过 `src -> peer` 反查”，减少每包地址映射扫描。
  - `transport` runtime 更新已限制为仅 `StateSync/ShardState` 尝试 header 解码，去掉其它 `DistributedOccc` 报文的无效解码开销。
  - `gateway` native sync worker 已改为“同步期短 tick（250ms）+ 空闲期常规 tick（1000ms）”，提升窗口推进速度。
  - `gateway` sync-pull 重发策略改为 phase 自适应（headers/bodies/state/finalize 分档），减少重发等待空窗。
  - `runtime_status` stale 清理改为 `retain` 原地回收（去掉 stale id 中间向量），降低重算时分配与遍历开销。
  - `runtime_status::get_network_runtime_peer_heads_top_k` 新增 `k=1` 快路径，降低 sync-pull 单播阶段排序开销。
  - `transport` 的 `StateSync/ShardState` ingress 解析已收口为单次复用：同一报文的 request/header 解析结果在 response 构建、runtime 更新、followup 构建三段共用，去掉重复解码与重复分支判断。
  - `transport` sync-pull 回包已改为“窗口计划 + 流式发送”：不再先构建整批 `Vec<ProtocolMessage>` 再发送，改为按窗口逐条构建并发送，失败路径保留 fallback，进一步降低中间容器分配与二次遍历开销。
  - `transport` 新增 `src_addr -> peer` 直达索引 + `src_ip` 唯一性索引并接入 UDP/TCP 收包路径：优先 O(1) 地址/同 IP 命中，仅索引未命中时才回退遍历推断，降低高并发收包时全表扫描成本。
  - `transport` 发送路径已改为“peer 地址即时拷贝后发送/重试”，避免在 TCP 重连和 UDP 发送阶段持有 DashMap guard，进一步降低发送与注册并发竞争。
  - `transport` sync-pull inflight 窗口已引入 phase 分档预取触发：在窗口尾部（按 phase 预取阈值）即可提前发起 followup，减少“等窗口完全收满后再发下一窗”的 RTT 空档。
  - `transport` 的 UDP/TCP 收包主线已改为“短锁取共享缓冲 -> 锁外 recv/decode/read -> 回填共享缓冲”，去掉“持锁解码/持锁读帧”路径，降低高并发收包锁竞争。
  - 新增回归：`runtime_sync_pull_headers_prefetch_can_trigger_followup_before_window_tail`，锁定预取触发语义不回退。
  - 本地验收通过：`novovm-network` `clippy -D warnings`、`test 48/48`。

### 专项总进度（2026-03-14）

- 高性能流水线专项总进度：`100%`



