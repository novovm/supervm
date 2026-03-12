# NOVOVM EVM/Adapter 迁移进度台账（SUPERVM）- 生产主线版（2026-03-12）

## 1. 执行口径（去工程化）

- 迁移完成度只看生产代码是否接线，不再以 `gate/signal/snapshot/rc` 数量判定。
- 对外可以 `HTTP/JSON-RPC`；对内必须 `ops_wire_v1/.opsw1 -> novovm-node -> AOEM` 二进制流水线。
- EVM 终局目标是 `Rust 全功能镜像节点`，不是“兼容层”或“脚本通过”。
- 铁律-1：`gateway` 只作为外部边界组件，不得成为 superVM 内部层间通信依赖；内部必须保持原生/二进制直连。
- 铁律-2：除非明确必须，禁止新增工程化包装层；旧有工程化内容如影响主线认知与性能，必须持续清理。
- 铁律-3：性能优先与极致静默。默认不输出可观测噪音日志；仅在显式开关下输出 `warn/summary`（当前 gateway 开关：`NOVOVM_GATEWAY_WARN_LOG`、`NOVOVM_GATEWAY_SUMMARY_LOG`，默认关闭）。
- 铁律-4：`novovm-node` 等公共入口/公共方法的修改，必须先经负责人手工同意；未获同意不得改动。

## 2. 当前主线能力状态（只看生产路径）

- 总体进度（EVM 生产主线）：`87%`
- 当前阶段进度（P07 全功能镜像主线）：`91%`
- 本轮收口进度（提交状态广播语义）：`100%`

| ID | 能力 | 当前状态 | 生产代码锚点 | 说明 |
|---|---|---|---|---|
| EVM-P01 | EVM 外部入口归一化（raw+non-raw） | InProgress | `crates/novovm-edge-gateway/src/main.rs` | `eth_sendRawTransaction`、`eth_sendTransaction`、`web30_sendTransaction` 已接到统一编码与主线消费路径；gateway 已接入插件 txpool 接收摘要判定，若被丢弃则直接失败并阻止 `.opsw1` 继续入主线，并按插件摘要 `reason/reasons`（统一枚举）返回稳定 JSON-RPC 码（underpriced/nonce-too-low/nonce-gap/capacity）与 geth 风格错误文案，同时在 `error.data` 返回结构化拒绝原因与计数。`eth_sendRawTransaction` + `eth_sendTransaction` 现共用可配置公网广播执行主路径（`NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_EXEC`）；执行器请求统一携带 `tx_ir_bincode`（`raw` 路径同时附带 `raw_tx`），配置后广播失败即直接拒绝写入并返回稳定错误语义（`public broadcast failed`）。 |
| EVM-P02 | D1 统一二进制入口（唯一生产 bin） | Done | `crates/novovm-node/src/bin/novovm-node.rs` | 仅保留 `novovm-node` 生产入口，消费 `.opsw1` 并对接 AOEM。 |
| EVM-P03 | EVM 插件执行主路径（apply_v2 + self-guard） | InProgress | `crates/novovm-adapter-evm-plugin/src/lib.rs` | 插件执行与 guard 主路径已在生产代码中。 |
| EVM-P04 | 内存 ingress 队列（插件侧） | InProgress | `crates/novovm-adapter-evm-plugin/src/lib.rs` | 已落地 txpool 最小语义：同 `sender+nonce` 的 price-bump 替换（默认 10%，可配）+ per-sender pending 上限（默认 64，可配）+ nonce gap 丢弃（默认 1024，可配）+ `pending -> executable` 连续 nonce 提升队列；并补齐显式 `executable drain`、`pending drain`、`pending sender bucket snapshot`，以及 pending 出队的 sender 轮转调度。 |
| EVM-P05 | 收益归集/换汇/发放最小闭环 | InProgress | `crates/novovm-adapter-evm-plugin/src/lib.rs` + `crates/novovm-edge-gateway/src/main.rs` | 已接入 `settlement -> payout_instruction` 生产链路；gateway 不再丢弃 settlement 记录，已将 settlement 按账本键直接写入 `.opsw1`（`ledger:evm:settlement:v1:*`、`ledger:evm:settlement_reserve_delta:v1:*`、`ledger:evm:settlement_payout_delta:v1:*`、`ledger:evm:settlement_status:v1:*`），并将 payout 直接编码为完整账本 op（`ledger:evm:payout:v1:*`、`ledger:evm:reserve_delta:v1:*`、`ledger:evm:payout_delta:v1:*`、`ledger:evm:payout_status:v1:*`、`ledger:evm:reserve_debit:v1:*`、`ledger:evm:payout_credit:v1:*`）后写入 `.opsw1`，不再依赖 `novovm-node` 通用入口做 EVM 专属投影。gateway 已补齐最小对账查询面（`evm_getSettlementById`、`evm_getSettlementByTxHash`）和失败补偿最小路径：payout 持久化失败时状态进入 `compensate_pending_v1` 并落待补偿记录，可通过 `evm_replaySettlementPayout` 重放；自动补偿现支持上限/冷却/阈值三参数（`NOVOVM_GATEWAY_EVM_PAYOUT_AUTOREPLAY_MAX`、`NOVOVM_GATEWAY_EVM_PAYOUT_AUTOREPLAY_COOLDOWN_MS`、`NOVOVM_GATEWAY_EVM_PAYOUT_PENDING_WARN_THRESHOLD`）并支持启动按上限回填 pending（`NOVOVM_GATEWAY_EVM_PAYOUT_PENDING_HYDRATE_MAX`）。剩余工作为补偿策略生产参数定版。 |
| EVM-P06 | 原子跨链 intent 本地检查后广播门控 | InProgress | `crates/novovm-adapter-evm-plugin/src/lib.rs` + `crates/novovm-edge-gateway/src/main.rs` | 插件侧已产出 `receipt + broadcast_ready` 队列；gateway 侧已改为严格原子门控：仅在 `wants_cross_chain_atomic=true` 时消费原子队列，并要求“无 rejected + 当前 tx 命中 ready intent”才放行，否则直接拒绝并返回稳定错误码/结构化错误。门控通过后，命中的 `atomic_ready` 已落 `.opsw1`（`ledger:evm:atomic_ready:v1:*`）并自动写入广播队列键（`ledger:evm:atomic_broadcast_queue:v1:*`）；状态推进到 `broadcast_queued_v1`，并同步进入 pending 广播票据（统一由执行器消费，避免“已排队但无执行来源”）。同时保留最小补偿闭环：落盘失败转 `compensate_pending_v1`，支持 `evm_replayAtomicReady` 手工重放为 `compensated_v1`，以及 `evm_getAtomicReadyByIntentId` 查询状态。广播侧新增 `evm_queueAtomicBroadcast`（手工重入队）、`evm_markAtomicBroadcastFailed`（失败标记+挂起重放票据）、`evm_replayAtomicBroadcastQueue`（失败后重放入队）、`evm_markAtomicBroadcasted`（广播完成确认），并新增 `evm_executeAtomicBroadcast`（单 intent 实播）+ `evm_executePendingAtomicBroadcasts`（pending 批量实播）；执行器由 `NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC` 指定，支持最小重试/超时/退避策略（`retry`、`timeout_ms`、`retry_backoff_ms` 及默认环境变量 `NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_RETRY`、`NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_TIMEOUT_MS`、`NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_RETRY_BACKOFF_MS`，已定版默认 `retry=1`、`timeout_ms=5000`、`retry_backoff_ms=25`），并已定版批量策略参数（`NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_BATCH_DEFAULT`、`NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_BATCH_HARD_MAX`，请求值会被硬上限钳制）；执行器请求在可用时会附带 `tx_ir_bincode`（来自 `atomic_ready` 匹配 leg，并按 `intent_id` 缓存，减少二次外部查询），执行器输出若为 JSON，将进行最小一致性校验（`broadcasted/intent_id/tx_hash/chain_id`）。默认走内联原生执行路径（直接写入 EVM ingress `.opsw1` 并复用同一结算/发放主线）；支持请求参数 `native=true`（或 `force_native=true`）显式锁定原生路径，也支持 `use_external_executor=true`（或 `exec=true`）切换到外部执行器路径。成功置 `broadcasted_v1`，失败置 `broadcast_failed_v1` 并保留重放票据。剩余工作是执行器实现侧联调与稳定性验证。 |
| EVM-P07 | EVM Rust 全功能镜像（网络/同步/txpool/运维） | InProgress | `crates/novovm-edge-gateway/src/main.rs` + `docs_CN/Adapters/EVM/NOVOVM-EVM-FULL-MIRROR-NODE-MODE-SPEC-2026-03-11.md` | 查询面继续扩展到主线：`eth_chainId`、`net_version`、`web3_clientVersion`、`eth_protocolVersion`、`net_listening`、`net_peerCount`、`eth_accounts`、`eth_coinbase`、`eth_mining`、`eth_hashrate`、`eth_maxPriorityFeePerGas`、`eth_feeHistory`、`eth_syncing`、`eth_pendingTransactions`、`eth_blockNumber`、`eth_getBalance`、`eth_getBlockByNumber`、`eth_getBlockByHash`、`eth_getTransactionByBlockNumberAndIndex`、`eth_getTransactionByBlockHashAndIndex`、`eth_getBlockTransactionCountByNumber`、`eth_getBlockTransactionCountByHash`、`eth_getBlockReceipts`、`eth_getUncleCountByBlockNumber`、`eth_getUncleCountByBlockHash`、`eth_getUncleByBlockNumberAndIndex`、`eth_getUncleByBlockHashAndIndex`、`eth_getLogs`、`eth_newFilter`、`eth_getFilterChanges`、`eth_getFilterLogs`、`eth_uninstallFilter`、`eth_newBlockFilter`、`eth_newPendingTransactionFilter`、`txpool_content`、`txpool_contentFrom`、`txpool_inspect`、`txpool_inspectFrom`、`txpool_status`、`txpool_statusFrom`、`eth_call`、`eth_getCode`、`eth_getStorageAt`。数据源统一来自 gateway EVM tx 索引（内存 + RocksDB 回补扫描），保持“对外 RPC、对内二进制流水线”口径。`eth_getCode/eth_getStorageAt` 已从占位返回升级为基于索引交易的最小状态投影读取（部署代码、调用槽位、部署 code-hash@slot0）。`eth_call` 已补最小读路径（空 calldata 读代码、32B calldata 读槽位、`balanceOf(address)` 读本地索引余额）。`eth_getTransactionCount` 已补 `latest/pending` 分离语义：默认按索引计算 `latest` nonce，并在 `pending` 标签下叠加 UA 路由 nonce（若存在绑定）。`eth_getBlockReceipts` 边界已固定：命中 hash/number 返回 receipts；`number <= latest` 且当前伪块无交易返回 `[]`；未知 hash 或超前块返回 `null`。`eth_getTransactionReceipt` 边界已固定：命中 tx hash 返回稳定块字段（`blockHash/blockNumber/transactionIndex/status`），未知 tx hash 返回 `null`。`eth_getTransactionByHash` 边界已固定：命中 tx hash 返回稳定块字段（`blockHash/blockNumber/transactionIndex` 且 `pending=false`），未知 tx hash 返回 `null`。`eth_getBlockByNumber/eth_getBlockTransactionCountByNumber/eth_getUncleCountByBlockNumber` 的空块语义已统一：`number <= latest` 且该伪块无交易时分别返回“空块对象 / 0x0 / 0x0”，超前块返回 `null`。`eth_*Filter` 当前为生产最小语义：filter id 本地注册、增量拉取和卸载；`txpool_content/contentFrom/inspect/inspectFrom/status/statusFrom` 优先投影 EVM 插件运行时快照（executable=>pending、nonce-gap=>queued），无运行时快照时回落 gateway 索引态。`eth_syncing/net_peerCount` 已接入同步状态源：优先读取 `NOVOVM_GATEWAY_ETH_SYNC_STATUS_PATH`（JSON 快照），再走环境覆盖（`NOVOVM_GATEWAY_ETH_PEER_COUNT`/`NOVOVM_GATEWAY_NET_PEER_COUNT` + `NOVOVM_GATEWAY_ETH_SYNC_{STARTING,CURRENT,HIGHEST}_BLOCK`），最后回落到本地索引高度。当前仍是最小可运行镜像查询语义（按链 + nonce 聚合的伪块结构），后续继续替换为真实同步头/状态树驱动。 |

## 3. 已落地主线（生产意义）

- 边界层请求已可归一化进入内部二进制流水线，不需要在内部再走 RPC/HTTP。
- `eth_sendRawTransaction` 与 `eth_sendTransaction` 已共用可配置公网广播执行主路径（`NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_EXEC`）；启用后失败即拒绝并返回 `public broadcast failed`，避免“已接收但未广播”。
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
- gateway 已补齐 `atomic-ready` 广播队列接线：自动写入 `ledger:evm:atomic_broadcast_queue:v1:*` 并更新状态 `broadcast_queued_v1`，支持 `evm_queueAtomicBroadcast` 手工重入队、`evm_markAtomicBroadcastFailed` 失败标记、`evm_replayAtomicBroadcastQueue` 失败后重放入队、`evm_markAtomicBroadcasted` 广播完成确认，以及 `evm_executeAtomicBroadcast`（单 intent）/`evm_executePendingAtomicBroadcasts`（批量）执行路径（默认原生内联；显式 `use_external_executor=true` 时可切外部执行器）。外部执行器最小 Rust 二进制已提供：`crates/novovm-edge-gateway/src/bin/evm_atomic_broadcast_executor.rs`（环境变量 `NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC`）。
- gateway 已补 EVM 查询面主线路径：`eth_chainId`、`net_version`、`web3_clientVersion`、`eth_protocolVersion`、`net_listening`、`net_peerCount`、`eth_accounts`、`eth_coinbase`、`eth_mining`、`eth_hashrate`、`eth_maxPriorityFeePerGas`、`eth_feeHistory`、`eth_syncing`、`eth_pendingTransactions`、`eth_blockNumber`、`eth_getBalance`、`eth_getBlockByNumber`、`eth_getBlockByHash`、`eth_getTransactionByBlockNumberAndIndex`、`eth_getTransactionByBlockHashAndIndex`、`eth_getBlockTransactionCountByNumber`、`eth_getBlockTransactionCountByHash`、`eth_getBlockReceipts`、`eth_getUncleCountByBlockNumber`、`eth_getUncleCountByBlockHash`、`eth_getUncleByBlockNumberAndIndex`、`eth_getUncleByBlockHashAndIndex`、`eth_getLogs`、`eth_newFilter`、`eth_getFilterChanges`、`eth_getFilterLogs`、`eth_uninstallFilter`、`eth_newBlockFilter`、`eth_newPendingTransactionFilter`、`txpool_content`、`txpool_contentFrom`、`txpool_inspect`、`txpool_inspectFrom`、`txpool_status`、`txpool_statusFrom`、`eth_call`、`eth_getCode`、`eth_getStorageAt`；查询数据源统一来自 EVM tx 索引（内存 + RocksDB 回补扫描），并保持对象参数/数组参数最小兼容入口。`eth_getCode/eth_getStorageAt/eth_call` 已具备最小状态读投影（部署代码、槽位值、`balanceOf` 读路径）。`eth_getTransactionCount` 已支持 `latest/pending/earliest` 最小语义。`eth_*Filter` 已具备生产最小语义（增量变化/全量 logs/卸载），`txpool_content/contentFrom/inspect/inspectFrom/status/statusFrom` 已优先接到插件运行时快照（fallback 到索引态）；`eth_syncing/net_peerCount` 已从硬编码占位切换为状态源驱动（快照文件/环境覆盖/本地索引回落）；`eth_getTransactionReceipt` 与 `eth_getTransactionByHash` 命中时都会返回稳定块字段，未命中返回 `null`；按块号查询的空块语义已统一（空块对象/0x0/0x0）。
- 本轮补 `eth_chainId/net_version` 链参数兼容：两者已统一支持 `chain_id/chainId`（含 `tx` 嵌套对象）并保持默认链回退，语义与其他 `eth_*` 查询入口一致。
- 本轮补 `eth_getTransactionByHash/eth_getTransactionReceipt` 的默认链隔离：未显式传 `chain_id` 时按默认链过滤（不再跨链命中 runtime/index），显式 `chain_id/chainId`（含 `tx.chainId`）可覆盖默认链，确保与其余查询接口链语义一致。
- 本轮补 `eth_syncing` 多链状态源隔离：同步快照文件支持按链读取（`chains.{chain_id}` 或顶层 `{chain_id}`），并新增按链环境覆盖键（如 `NOVOVM_GATEWAY_ETH_SYNC_CURRENT_BLOCK_CHAIN_137`）；同时新增按链快照路径键（`NOVOVM_GATEWAY_ETH_SYNC_STATUS_PATH_CHAIN_<id>`）可覆盖全局快照路径。未命中链专属键时自动回退全局键，避免多链节点共用同步源时链间串值。
- 本轮修正 `eth_syncing` 状态源优先级：由“env -> snapshot 覆盖”改为“snapshot 基线 + env 最终覆盖”，确保运维在不改快照文件的情况下可用环境变量即时覆盖同步高度。
- 本轮补齐 runtime pending 查询语义：`eth_getTransactionByHash` 在索引未命中时会回落到插件运行时快照（executable + queued）返回 pending 交易对象；`eth_pendingTransactions` 优先返回运行时快照（无快照再回落索引）；`eth_getTransactionCount(tag=pending)` 已叠加运行时 txpool nonce 视图（与 UA 路由 nonce / 索引 latest 取最大）。
- 本轮补齐 pending block 语义：`eth_getBlockByNumber("pending")` 与 `eth_getBlockTransactionCountByNumber("pending")` 已直接对接运行时 txpool 快照（executable + queued），不再复用 `latest` 视图。
- 本轮进一步补齐 pending block 扩展语义：`eth_getBlockByHash`、`eth_getTransactionByBlockNumberAndIndex("pending")`、`eth_getTransactionByBlockHashAndIndex`、`eth_getBlockTransactionCountByHash` 已接入同一 pending 视图；`eth_newPendingTransactionFilter/eth_getFilterChanges` 也已优先消费运行时 txpool（运行时为空时回落索引态）。
- 本轮继续补齐 pending 对账边界：`eth_getBlockReceipts`（by number/by hash）与 `eth_getUncleCountByBlockNumber/eth_getUncleCountByBlockHash` 已接入 pending block 视图；`eth_newPendingTransactionFilter/eth_getFilterChanges` 的增量判定已收敛为 hash 集合差分，避免计数偏移带来的漏报。
- 本轮成组收口：`eth_getBlockReceipts(pending)` 与 `eth_getTransactionReceipt` 的 pending 语义已统一为 `pending=true + status=null`（并保持 pending block 的 `blockNumber/blockHash/transactionIndex`）；`eth_getTransactionReceipt` 现已在索引未命中时回落插件运行时 txpool（按 hash 命中返回 pending receipt）；`eth_syncing` 在返回同步对象时补齐 pending block 边界一致性（不会把 pending 当已同步块，同时 `highestBlock` 不低于 pending 边界）。
- 本轮继续收口 pending 交易对象语义：`eth_getBlockByNumber("pending", true)`、`eth_getBlockByHash(pending-hash, true)`、`eth_getTransactionByBlockNumberAndIndex("pending")`、`eth_getTransactionByBlockHashAndIndex(pending-hash)` 统一返回 `pending=true`；`eth_getTransactionByHash` 在索引未命中时也会优先按 runtime pending block 返回带 `blockNumber/blockHash/transactionIndex` 的 pending 交易对象（仍保持 `pending=true`）。
- 本轮新增回归锁定：`eth_syncing` 的“对象态 + pending 边界”已抽成纯函数并加专门测试，固定 `startingBlock<=currentBlock<=highestBlock` 且 pending 场景下 `highestBlock>=local_current+1` 的语义，避免后续改动回退。
- 本轮补端到端一致性样例：同一 runtime 链下，当存在 pending block 时，`eth_blockNumber` 保持已确认高度（如 `0x0`），`eth_getBlockByNumber("pending")` 返回下一高度（如 `0x1`），`eth_syncing` 进入对象态并将 `highestBlock` 抬升到 pending 边界（如 `0x1`），统一“已确认高度 / pending 视图 / 同步状态”三者边界。
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
- 本轮继续收口 `eth_syncing` 与 pending block 边界：当存在 pending block 时，同步对象语义固定为 `highestBlock >= currentBlock + 1`（不再受外部快照 `current==highest` 覆盖而误返回 `false`）；并新增回归断言覆盖“外部同步高度已抬升 + 本地 pending 仍存在”的边界场景。
- 本轮继续收口 `latest` 高度来源一致性：`eth_newBlockFilter`、`eth_getProof`、`eth_getTransactionCount`（future 历史判断）、`eth_getTransactionByHash`/`eth_getTransactionReceipt`（runtime pending 回退）已统一改为“RocksDB 最新块索引”优先，不再依赖内存窗口 `max(nonce)`；并新增回归断言覆盖“内存窗口陈旧 + store 最新高度存在 + runtime pending 查询”的场景。
- 本轮补 `eth_getFilterChanges`（block filter）在“内存窗口陈旧 + store 有新块”场景的漏报问题：分支已改为先取统一 `latest`，并按块号走精确回补（`chain_id + block_number`）发出缺失块 hash，同时固定 `last_seen_block` 只能单调前进，避免回退导致重复/漏报；并新增回归断言锁定该边界。
- 本轮补 `eth_feeHistory` 在“窗口截断 + store 有目标块”场景的数据空洞：当范围内块不在内存窗口时，已改为按 `chain_id + block_number` 精确回补该块交易用于 `gasUsedRatio/reward` 计算；并新增回归断言锁定 `gasUsedRatio/reward` 可从 store 精确恢复，不再退化为空块占位。
- 本轮补 `eth_estimateGas` 生产语义收口：内在 gas 估算已对齐合约部署基础成本（`+32000`）与 initcode 字成本（`+2/word`），并接入 `accessList` intrinsic 成本（`2400/address + 1900/storageKey`）；同时 EVM Core 校验新增部署 initcode 大小上限（49152 bytes），避免低估 gas 或异常大部署载荷进入主线。
- 本轮继续收口 `type1/type2 raw tx` 写入与执行一致性：EVM Core 已解析 raw `accessList` 计数（address/storageKey），`validate_tx_semantics_m0` 在 raw signature 可解析时会把 `accessList` intrinsic 成本并入最小 gas 校验（低 gas 将拒绝），并与 gateway `eth_estimateGas` 共用同一 intrinsic 公式实现，消除估算/执行口径漂移。
- 本轮补 `eth_sendTransaction` 与 `accessList` 的生产一致性：对象入口已接入 `accessList` intrinsic 校验（`gas_limit` 低于 `base + accessList` 时直接拒绝），并在“无显式 `type` 且存在 `accessList` 成本”时自动推断 `type=0x1`；同时将 `accessList` 计数纳入网关交易哈希输入，避免不同 `accessList` 请求落同哈希。
- 本轮补 `effectiveGasPrice/baseFee` 成组收口：`eth_getBlockBy*` 与 `eth_feeHistory` 的 `baseFeePerGas` 统一走同一来源（`NOVOVM_GATEWAY_ETH_DEFAULT_BASE_FEE_PER_GAS`，默认回退 `NOVOVM_GATEWAY_ETH_DEFAULT_GAS_PRICE`）；receipt 的 `effectiveGasPrice` 对 `type2` 统一采用 `max(gas_price, baseFeePerGas)`，对象入口与 raw 入口语义一致。
- 本轮补交易查询费率字段兼容：`eth_getTransactionByHash` / `eth_getTransactionByBlock*` / `eth_pendingTransactions` / `txpool_content` 的交易对象统一输出 `maxFeePerGas/maxPriorityFeePerGas`（仅 `type2` 有值，其他类型为 `null`）；runtime pending 路径会在可识别 `raw type2` 签名时对齐同一语义。
- 本轮补 `eth_syncing` 与 pending block 边界直连：同步对象构造改为直接消费 runtime pending 视图产出的 `pending_block_number`，不再用布尔态间接推断；保持 pending 存在时 `highestBlock` 至少覆盖 pending 边界，并满足 `highestBlock >= currentBlock + 1`。
- 本轮补 `eth_syncing` 回归稳定性：涉及环境变量覆盖的多条测试已加统一环境锁，消除并行测试下的链间环境串扰，避免 CI 偶发假红（生产逻辑不变）。
- 本轮补写入入口参数兼容收口：`eth_sendRawTransaction` 已兼容 `chainId`（camelCase）与 `sessionExpiresAt` / `wantsCrossChainAtomic` / `signatureDomain` 参数别名解析，`eth_sendTransaction` 同步兼容 `signatureDomain`、`sessionExpiresAt`、`wantsCrossChainAtomic`；并补回归锁定 `chainId` 别名仍参与严格链 ID 一致性校验（与 raw 内链 ID 不一致会拒绝）。
- 本轮修正公共入口边界：`novovm-node` 已撤回 EVM 专属 `project_gateway_evm_payout_ops` 包裹，恢复通用入口纯读入执行；EVM payout 改为在 gateway 生产侧直接编码完整账本 op 写入 `.opsw1`（不再依赖 node 侧二次投影）。
- 本轮补查询/写入参数混排兼容：`eth_getTransactionReceipt`、`eth_getBlockReceipts`、`eth_getBlockBy*`、`eth_sendRawTransaction`、`eth_sendTransaction` 等入口的核心参数提取已统一支持“对象参数 + 标量参数混排数组”形态（如 `[{"chainId":1}, "0x..."]`）；`block tag/hash`、`tx hash`、`raw_tx`、`address` 解析不再依赖“必须在数组首位”，并补回归锁定该兼容行为。
- 本轮继续收口“前置链参数对象 + 业务对象/标量”混排：`eth_call`、`eth_getProof`、`eth_getStorageAt`、`eth_getLogs`、`eth_getBalance`、`eth_getTransactionCount`、`eth_getTransactionByBlock*` 等解析链路已统一为“数组中按 key 选对象 + 按有效参数位取标量”，不再依赖 `arr[0]/arr[1]/arr[2]` 固定位置；并补回归锁定 `call/proof/logs/slot/blockTag` 的混排语义。
- 本轮补 `eth_feeHistory` 混排参数兼容：在 `[{"chainId":...}, blockCount, newestBlock, rewardPercentiles]` 形态下，`blockCount/newestBlock/rewardPercentiles` 已按“非对象参数位”正确解析；同时修正 block+index 双标量场景下 block 标签优先级（`eth_getTransactionByBlock*` 不会把 `txIndex` 误当 block tag）。
- 本轮继续收口 pending block 存在边界：仅当 runtime txpool 存在待打包交易时，`eth_getBlockByNumber(pending)`、`eth_getBlockReceipts(pending)`、`eth_getTransactionByBlockNumberAndIndex(pending)`、`eth_getBlockTransactionCountByNumber(pending)`、`eth_getUncleCountByBlockNumber(pending)` 才返回 pending 视图；无 runtime pending 时统一返回 `null`，并补回归 `eth_pending_block_queries_return_null_without_runtime_pending_txs` 锁定语义。
- 本轮继续收口 txpool/pending 语义到 runtime-only：`eth_pendingTransactions`、`txpool_content`、`txpool_contentFrom`、`txpool_inspect`、`txpool_inspectFrom`、`txpool_status`、`txpool_statusFrom` 在无 runtime txpool 数据时统一返回空结果（`[]` / `{pending:{},queued:{}}` / `pending=0x0,queued=0x0`），不再回退已确认索引交易，避免“确认态被误当 pending”。
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


