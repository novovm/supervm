# NOVOVM EVM 协议可观察等价 v1 收口标准（2026-06-07）

## 定义

这里的“等价”不是代码等价 geth，也不是把 SUPERVM 做成完整以太坊全节点。

目标定义为：

`同一链配置、同一交易/区块输入下，外部系统通过协议、RPC、receipt、log、gas、failure、BAL、block metadata 等可观察结果，无法区分 SUPERVM EVM 插件和以太坊 EVM 语义实现。`

因此可接受：

- SUPERVM 用 Rust 实现，geth 用 Go 实现。
- 内部执行、调度、AOEM artifact、存储结构可以不同。
- 产品面是 Novo 主网可控 EVM 插件，不是替代 geth 的全节点二进制。

不可接受：

- raw tx 验签、gas、fee、nonce、receipt/log、失败分类在可观察结果上偏离目标 fork 语义。
- block metadata、BAL、receipt ingest/export 在插件边界缺失或不可复验。
- 对外宣称完整 geth/full-node 等价，但没有 p2p peer sync、block import、state root/devnet 差分证据。

## v1 自动验收门禁

v1 不再继续按“每轮一个 fixture 子集”推进，而是把已具备的生产路径证据收口为 3 个聚合 gate：

```powershell
cargo test -p novovm-adapter-novovm evm_protocol_observable_equivalence_execution_gate_v1 -- --nocapture
cargo test -p novovm-node evm_protocol_observable_equivalence_geth_rpc_fixture_gate_v1 -- --nocapture
cargo test -p novovm-adapter-evm-plugin evm_protocol_observable_equivalence_plugin_receipt_bal_gate_v1 -- --nocapture
```

这 3 个 gate 覆盖：

- execution observable：raw tx、typed tx、fee/value debit、failure/revert/OOG、EIP-1559、access-list、SLOAD/SSTORE、CREATE/CREATE2、official state fixture projection subset、receipt/log/gas baseline。
- RPC/geth fixture observable：内置 geth parity fixture 和文件驱动 geth parity fixture，覆盖 block、receipt、logs、typed failure、canonical/noncanonical log ownership。
- plugin observable：receipt export/ingest、state mirror update、block metadata、contract call/deploy BAL completeness/hash、fee settlement、txpool replacement/reject。

## 当前 v1 口径

可以声明：

`NOVOVM/SUPERVM 当前具备 Novo 主网可控 EVM 插件的协议可观察等价 v1 门禁：执行语义、geth ethapi 样本 parity、receipt/log/BAL/plugin metadata 生产路径均有自动回归。`

不能声明：

`SUPERVM 是完整 geth 替代品。`

`SUPERVM 已作为 eth/71 p2p 全节点被其它 Ethereum 节点长期稳定识别为等价 peer。`

`SUPERVM 已通过 Ethereum execution-spec 全量 state tests 或 mainnet block replay。`

## v2 当前推进：RPC 黑盒投影根门禁

本轮 v2 不再继续增加官方 fixture 子集，而是先修复可被外部 RPC 客户端直接观察到的区块根字段：

- `eth_getBlockByNumber/eth_getBlockByHash` 的 `transactionsRoot` 不再返回 `null`；canonical batch 带 raw tx RLP 时按 Ethereum raw transaction trie 计算，缺 raw RLP 时回落到 canonical receipt projection 的 MPT 32-byte root。
- `eth_getBlockByNumber/eth_getBlockByHash` 的 `receiptsRoot` 不再返回 `null`，改为基于 receipt status、cumulative gas、logsBloom、logs 的 MPT 32-byte root。
- geth parity report 新增 `observableProjection` section，锁住 `transactionsRoot`、`receiptsRoot`、`stateRoot`、`gasUsed`、`cumulativeGasUsed`、`logsBloom`。
- 默认 11 个 geth parity 样本当前 `totalMismatchCount=0`，`observableProjection.mismatchCount=0`。

v2 projection gate：

```powershell
cargo test -p novovm-node evm_protocol_observable_equivalence_geth_rpc_blackbox_projection_gate_v2 -- --nocapture
```

当前 v2 口径可以声明：

`SUPERVM EVM RPC/geth parity 样本已具备 block/receipt/log/gas/root projection 的黑盒可观察一致性门禁，避免区块对象根字段因 null 被外部客户端直接区分。`

当前 v2 不能声明：

`transactionsRoot/receiptsRoot 已覆盖全量 mainnet block replay 和所有交易类型组合。`

`stateRoot 已完成 geth/reth devnet 同输入 replay 对齐。`

## v2b 当前推进：真实 geth block fixture 差分

本轮已接入一个真实 go-ethereum `ethapi/testdata` fullTx block fixture，并把 raw tx RLP 接入 canonical block projection 路径，v2b 差分 gate：

```powershell
cargo test -p novovm-node evm_protocol_observable_equivalence_geth_real_block_diff_gate_v2b -- --nocapture
```

该 gate 做两件事：

- 从 geth fullTx block fixture 的 legacy tx 字段复算 raw transaction RLP trie root，并确认复算值等于 geth fixture 的 `transactionsRoot`。
- 把同一 raw tx RLP 写入 SUPERVM canonical batch，使 `eth_getBlockByNumber/eth_getBlockByHash` 的 `transactionsRoot` 使用 Ethereum raw transaction trie root。
- 用同一 block/receipt 形态构造 SUPERVM canonical projection，和 geth block 对比 `number`、`gasUsed`、`logsBloom`、`receiptsRoot`、`stateRoot`、`transactionsRoot`。

当前结果：

- 已匹配：`number`、`gasUsed`、`logsBloom`、`transactionsRoot`、`receiptsRoot`、`stateRoot`。
- 已知 gap：无。`knownGapCount=0`，`requiresRawTxRlpForFullTransactionsRootEquivalence=false`。

这一步的意义是把 v2b 从口头目标变成可运行的真实 geth block 差分报告，并完成 raw tx RLP 驱动的 `transactionsRoot` 收口。v3 已开始进入真实 RLPx 网络可观察面，不再继续堆内部 smoke；只有 v3 暴露具体差异时才补对应真实 block/tx 类型差分。

## v3 网络可观察等价进展

新增 gates：

```powershell
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_tx_ingress_gate_v3 -- --nocapture
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_tx_outbound_broadcast_gate_v3 -- --nocapture
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_pooled_tx_gate_v3 -- --nocapture
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_pooled_tx_response_gate_v3 -- --nocapture
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_block_body_import_gate_v3 -- --nocapture
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_receipts_gate_v3 -- --nocapture
cargo test -p novovm-network real_rlpx_worker_recovers_missing_receipts_before_new_header_pull -- --nocapture
cargo test -p novovm-network real_rlpx_peer_worker_ingests_runtime_native_snapshots -- --nocapture
cargo test -p novovm-node eth_rlpx_public_sync_batch_defaults_are_product_chase_ready_v1 -- --nocapture
cargo test -p novovm-node eth_rlpx_peer_refresh_plan -- --nocapture
cargo test -p novovm-node eth_peer_endpoint_refresh_merge_does_not_shrink_pool_v1 -- --nocapture
cargo test -p novovm-node eth_rlpx_peer_discovery_deadline_caps_phase_timeout_v1 -- --nocapture
cargo test -p novovm-node eth_dns_query_txt_respects_expired_discovery_deadline_v1 -- --nocapture
cargo test -p novovm-node eth_discv4_findnode_continues_when_lookup_adds_candidates_v1 -- --nocapture
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_reorg_gate_v3 -- --nocapture
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_bal_response_gate_v3 -- --nocapture
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_bal_commitment_rejects_mismatch_gate_v1 -- --nocapture
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_bal_context_rejects_index_excess_gate_v1 -- --nocapture
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_snap_account_range_gate_v3 -- --nocapture
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_snap_account_to_storage_code_gate_v3 -- --nocapture
cargo test -p novovm-network rlpx_snap_range_proof_semantics_match_geth_complete_storage_v1 -- --nocapture
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_snap_service_sidecars_gate_v3 -- --nocapture
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_new_block_gate_v3 -- --nocapture
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_new_block_hashes_gate_v3 -- --nocapture
$env:NOVOVM_NODE_MODE='eth_rlpx_sync'; $env:NOVOVM_ETH_RLPX_TICKS='8'; cargo run -p novovm-node --bin novovm-node
```

这些 gate 使用真实 TCP socket 和 RLPx frame，不走内存 transport：

- 完成 RLPx auth、P2P Hello 和 eth Status 网络校验。
- 远端 peer 发送标准 eth `Transactions` frame。
- SUPERVM 解析 raw transaction RLP，按以太坊 tx hash 计算进入 native pending tx。
- pending tx 标记为 remote origin，保留 source peer。
- 原始 tx RLP 保留为后续 `Transactions` broadcast candidate。
- 本地 pending raw tx 经过 worker budget 触发 hash-only `NewPooledTransactionHashes` outbound announce。
- 远端 peer 解析 SUPERVM 发出的 `NewPooledTransactionHashes` frame，确认 tx type、size 和 hash，再通过 `GetPooledTransactions` 拉取 raw RLP payload。
- 本地 pending tx 以 geth 风格 `NewPooledTransactionHashes` 发送 hash-only announce，wire-frame 写成功后才标记 propagated，并记录目标 peer 和 broadcast runtime summary；远端随后发 `GetPooledTransactions` 时，SUPERVM 回放本地 raw tx payload。
- 远端 peer 发送 `NewPooledTransactionHashes` 后，SUPERVM 会按 hash 发起 `GetPooledTransactions`，收到 raw `PooledTransactions` 后 materialize 到 pending tx payload/broadcast candidate。
- `PooledTransactions` / `BlockBodies` / `Receipts` / `BlockAccessLists` 这类 response 型消息现在都要求本地存在匹配 pending request；未请求响应按 peer decode failure 拒绝。`PooledTransactions` 按 geth request tracker 语义要求 response item count 不超过 request count，并拒绝重复 tx hash；匹配 request_id 后不再要求返回 hash 必须是 requested hashes 的有序子集，避免把 geth 可接受的 out-of-bound tx delivery 误判成 wire 协议错误。
- 远端 peer 发送 `GetPooledTransactions` 请求本地 pending raw tx 时，SUPERVM 返回协议合法 `PooledTransactions` response，不对未知 hash 返回占位。
- 远端 peer 发送 eth/69+ `BlockRangeUpdate` 后，SUPERVM 按 geth 同形三字段 `[earliestBlock, latestBlock, latestBlockHash]` 校验并更新 runtime peer head/highest；`earliest > latest` 和 zero latest hash 会被拒绝。
- 远端 peer 通过 `BlockHeaders` 返回真实 RLP header，SUPERVM 按 wire header RLP hash 发起 `GetBlockBodies`。
- `BlockHeaders` 批次导入前必须存在匹配的本地 `GetBlockHeaders` pending request，并校验 request_id、origin number/hash、skip/reverse step 和相邻 parentHash 连续性；未请求响应、编号跳跃或拼接批次按 peer decode failure 拒绝，不写入 native header/materialization 队列。
- 远端 peer 通过 `BlockBodies` 返回含 raw tx 的 body，SUPERVM 按 body raw tx 复算并校验 header `transactionsRoot` 后导入 native body snapshot，并保留 raw transaction RLP、block hash、tx hash、withdrawal count 和 materialized 状态；native head/history store 也会持久化/恢复 raw tx RLP，避免后续 Engine `getPayloadBodies*` 只能从 tx hash 或网关索引字段重构假 payload。
- 远端 peer 主动发送 hash-origin `GetBlockHeaders` 或 `GetBlockBodies` 时，SUPERVM 只从本地 canonical native runtime 中已验证/已物化的数据返回 `BlockHeaders` / `BlockBodies`；header 回包使用原始 header RLP，body 回包使用 raw tx RLP，缺失、非 canonical、未 materialize 或本地无法重建的 body 提前短响应，不伪造历史。
- `BlockBodies` 返回后，SUPERVM 继续发 eth/70 `GetReceipts(firstBlockReceiptIndex=0)`；收到 receipts 后会拒绝 `lastBlockIncomplete=true`、block count mismatch 和 receipt count mismatch，解析 `Receipts(lastBlockIncomplete=false)` 后按 raw receipt MPT 校验 header `receiptsRoot`；如果 peer 在 body 已导入但 receipt 未返回前断开，下一条 ready RLPx session 会先从 latest native header/body 重建 pending receipt 并补发 `GetReceipts`，不会直接跳到新 header pull；对 empty/no-withdrawal block，如果父块已保留，还会校验子块 `stateRoot` 与父块 `stateRoot` 连续；通过后落地 native receipt snapshot、更新 canonical block receipt/stateRoot readiness，并把本轮 peer sync 标记 ready；本地响应 `GetReceipts` 时优先回放已验证 raw receipts，只有能证明空交易 body 时才响应空 receipts，不伪造缺失 receipt 数据。
- 真实 RLPx peer 顺序返回 120A 和 121B 分支，runtime canonical head 从 120A reorg 到 121B。
- 120A 中已 canonical included 的 pending tx 回到 `ReorgedBackToPending`，并携 raw RLP 重新进入 broadcast candidate。
- 无 snap 协商的插件路径里，远端 peer 发送真实 `GetBlockAccessLists` frame，SUPERVM 返回协议合法 `BlockAccessLists` frame，保持 request_id 和 requested hash 数量一致。
- eth/71 header RLP 现在按 geth optional header 顺序保留 `BlockAccessListHash`；当 headers/NewBlock 带 BAL hash 且本地缺 payload 时，SUPERVM 会在 body/receipt 收口后主动发送 `GetBlockAccessLists`，收到匹配 request_id 的 `BlockAccessLists` 后按请求 block hash 顺序处理真实 BAL RLP。BAL 入库前会执行 geth-style raw RLP 结构校验，包括拒绝空 `slotChanges`，要求 raw-RLP Keccak 等于 header `BlockAccessListHash`，并使用 header `gasLimit` 与 body `tx_count` 校验 item count 和 block access index 上限；missing sentinel 不伪造数据。
- eth/70+snap/1 路径里，global code `0x22/0x23` 归 snap `GetAccountRange/AccountRange`，SUPERVM 在 State phase 使用 native head `stateRoot` 发起 `GetAccountRange` 并记录匹配响应，不让 BAL 插件抢占主网 snap code。
- `AccountRange` 返回非空 slim account 时，SUPERVM 会解析 storage root / code hash，继续发出真实 `GetStorageRanges` 和 `GetByteCodes` 请求；匹配响应会落入 native snap account/storage/code cache，bytecode 在缓存前按 codeHash 校验；`AccountRange`/`StorageRanges` response 现在会按 geth `VerifyRangeProof` 前置条件拒绝非严格递增 key、删除空 value、account origin/limit 越界和不可解码 slim account；带 proof 时会验证 proof node 是合法 trie RLP，并要求 proof 中包含请求 `stateRoot` / account `storageRoot` 对应的 root node hash；当 proof 可沿 MPT path 解析出返回 account/slot 的 leaf value 时，该 value 必须和 snap response 一致，否则拒绝；如果 proof 证明请求 origin 本身存在 account value，response 第一个 account 不能跳过 origin；如果 proof 证明 `origin..first` 或相邻返回 key 之间存在被省略的 account/slot entry，也会拒绝；空 `AccountRange`/`StorageRanges` 带 proof 时还必须证明 range 右侧没有剩余 trie entry，不能把仍有后续数据的空响应当完成；`StorageRanges` 无 proof 时必须按返回 slot range 重建对应 account `storageRoot` 后才允许缓存；`StorageRanges` 带 proof 时按 geth 只把 proof 应用于最后一个 slotset，前置 slotset 必须完整重建 root；这打通 state sync 数据闭环并覆盖 geth 完整范围 no-proof root 子集、range precondition 子集、partial range 左/内部 gap 子集和空 range no-more 子集，但还未声明完整多分支 snap heal。
- snap/1 native cache 入口现在要求非空 `AccountRange` 必须携带 proof，否则按 peer decode failure 拒绝并且不落 cache；`AccountRange`/`StorageRanges` 会先校验 key/value range 前置条件；带 proof 的 `AccountRange`/`StorageRanges` 会校验 proof node RLP 结构、root 归属、可解析 leaf value 与 response 的一致性，以及 proof 可证明的左边界/内部 gap；proof 证明 origin 有 account value 时禁止 response 从更右侧开始；空 `AccountRange`/`StorageRanges` 带 proof 时会用 MPT 右侧元素判断确认 range 已结束，否则拒绝；`StorageRanges` 没有 proof 时按 geth 完整范围路径做 storageRoot 重建校验，root mismatch 拒绝；带 proof 时只用最后一个 slotset 的 proof，前面 slotset 走完整 root 重建。这封住未证明非空 state 数据、乱序/删除 range、origin omission、partial range 漏项和伪终止空 range 直接进入 native cache 的风险，但仍未声明完整 snap state heal。
- 已协商 snap/1 路径里，远端 peer 发送真实 `GetStorageRanges`、`GetByteCodes`、`GetTrieNodes` 请求时，SUPERVM 返回协议合法空 `StorageRanges`、`ByteCodes`、`TrieNodes` 响应，request_id 保持一致；这只封住服务面丢包，不等于完整 snap state heal/download/store。
- `novovm-node` 已新增直接产品入口 `NOVOVM_NODE_MODE=eth_rlpx_sync`，不经过临时脚本即可启动 native Ethereum RLPx worker；真实主网有限 tick 验证中，节点完成 Status、Headers、Bodies、Receipts，native current 从 0 推进到 8192。native capability 现在默认广告/选择 `eth/71`，远端只支持 70 时降级到 `eth/70`；evm-gateway RLPx 产品入口也已广告/选择 `eth/71`，并把 BAL `0x22/0x23` 限定在 negotiated eth/71 下识别，避免 eth/70+snap/1 code 冲突；eth/71 下 BAL 占用 `0x22/0x23`，snap/1 offset 后移到 `0x24`；启用 eth/71 capability 后的 24 tick live run 仍可和旧 peer 降级协商 `negotiated_eth=69` 并推进到 `current=1024/highest=25268137`。入口默认候选顺序已调整为 explicit ENODEs -> geth DNS discovery -> discv4 discovered peers -> Ethereum mainnet geth bootnodes fallback，减少固定 bootnodes 长期占住直连尝试窗口；Google/Cloudflare JSON DoH 对部分 branch 返回空/NXDOMAIN 时，会 fallback 到 UDP TXT 查询，实测 DNS endpoints=28、总 candidates=32，后续同步段来自 DNS peer `65.108.70.101:30303`。DNS root 现在使用 geth signed `enrtree://AKA3AM6LPBYEUDMVNU3BSVQJ5AD45Y7YPOHJLEF6W26QOE4VTUDPE@all.mainnet.ethdisco.net`，默认校验 root signature 和 child TXT `Keccak256(record)` hash prefix；signed DNS + discv4 8 tick live run 返回 `DNS endpoints=5`、`discv4 endpoints=23` 并推进到 `current=1024/highest=25268250`。75 tick live soak 暴露 peer 断线后 `highest` 回落到 local current 的长期追高阻塞点；runtime sync status 已增加短期 remote-best hint，后续 live run 中断线后 `highest` 保持远端高度、不再丢失追高目标（实测 `current=5120` 时 `highest=25267501`）。60 tick live soak 还暴露 EOF/remote-closed 后 dead RLPx stream 留在 session map、导致后续 tick 不继续发下一段 headers 的问题；现在会清理 session，并把 TCP EOF/remote close 记为 transient disconnect 短冷却，减少刚关闭 peer 的立即重连；80 tick live run 仍推进到 `current=6144/highest=25267712`。`too_many_peers` 容量拒绝现在会触发短期 veto/降权，bootstrap 同分候选增加分钟级 rotation bonus，1 tick live snapshot 已出现 `bootstrap_rotation_bonus` reason；96 tick live run 在 48 候选下推进到 `current=8192/highest=25267770`；当所有候选都在 cooldown 且无 ready peer 时，产品入口会按 `NOVOVM_ETH_RLPX_ADAPTIVE_CANDIDATE_PEERS_MAX` 扩容候选并重建 worker，104 tick live run 从 32 自动扩到 64（tick 10）再扩到 128（tick 57），之后仍重新连上 peer 并推进到 `current=5120/highest=25267820`。产品入口还新增默认开启的 checkpoint、latest native head store 和 native history window store：checkpoint 启动时恢复 current/highest，tick 后写回 sync/header 进度，实测临时 checkpoint 可从 `current=1234/highest=5678` 恢复；native head store 会持久化最新已校验 header/body/receipt 并恢复 runtime head，也会按 restored head `stateRoot` 持久化/恢复 bounded snap account/storage/code 子集；产品入口临时 store 恢复输出 `current=77 highest=99 header_number=77 body_available=true`；native history store 会持久化最近一段 header/body/receipt 并按高度恢复 runtime/canonical head，8 tick live run 写出 `blocks=2` 且推进到 `current=2048/highest=25268092`。直接产品入口现在还支持显式 trusted head/pivot：`NOVOVM_ETH_RLPX_TRUSTED_HEAD_NUMBER/HASH/STATE_ROOT` 可安装 operator 信任的 runtime head，只有不落后于 checkpoint/native store 时才覆盖，避免冷启动被迫从 genesis-only 路径追赶；这仍不等于 trustless checkpoint selection 或完整 state DB。当前仍不等于完整 geth DNS iterator/link-cache 语义、discv4 peer churn/长期追高，也不等于完整长期 block/state/receipt durable store、完整 state DB 或 eth/71 长稳公网接受度。
- trusted head/pivot 的 `NUMBER`/`HIGHEST` 输入接受十进制或 Ethereum RPC 风格 `0x`，可直接复用 `eth_getBlockByNumber` 返回的 header 字段作为 operator 信任锚。
- trusted head/pivot 可选落入 `parentHash`、`transactionsRoot`、`receiptsRoot`、`sha3Uncles`、`logsBloom`、gas/timestamp/baseFee、withdrawals/blob 和 BAL hash 字段，避免产品入口只恢复 hash/stateRoot 而暴露不完整 runtime header。
- trusted head/pivot 真实入口 probe：用 RPC header `0x1819755` 的 hash/stateRoot/parent/txRoot/receiptRoot/ommers 作为临时环境变量，`NOVOVM_NODE_MODE=eth_rlpx_sync` 1 tick 成功安装 trusted head，连接到 ready peer 并输出 `current=25270101/highest=25270113/native_phase=finalize/sync_requests=1`；临时 store 已清理，不作为长稳同步证明。
- trusted pivot follow-up probe 暴露真实公网 peer 可能对 `GetBlockBodies` 返回短响应（本轮 observed 12/16），当前 RLPx body import 已按 index 优先、唯一 `transactionsRoot` 其次接受可匹配 body、落地已返回 body，并立即补发剩余 hash 的 `GetBlockBodies`，避免长期同步把软响应当成 peer decode failure；无法匹配 pending header 的 body 仍会被拒绝。
- 本轮 snap proof hardening 后的直接产品入口 probe：使用临时 checkpoint/head/history store、`NOVOVM_ETH_RLPX_PEER_DISCOVERY_TOTAL_TIMEOUT_MS=10000`、`NOVOVM_ETH_DNS_DISCOVERY_TOTAL_TIMEOUT_MS=5000`、`NOVOVM_ETH_RLPX_CANDIDATE_PEERS=64`、16 tick live run 拿到 ready peer，完成 Headers/Bodies/Receipts，推进到 `current=2048/highest=25269891`，`body_available=true`；128 候选无启动预算约束的 probe 会在发现阶段超时，不作为长稳证据。
- Empty-body receipt materialization 已接入 RLPx 导入路径：当 body 已物化、tx count 为 0 且 header `receiptsRoot` 为 Ethereum empty trie root 时，SUPERVM 会本地生成 empty native receipt snapshot，不再等待远端 `Receipts` 响应；这封住了 live soak 中 block `1024` header/body available 但 `receipt=null` 导致长期不前进的卡点。该门禁由 `rlpx_empty_body_materializes_empty_receipts_without_remote_receipts` 覆盖；本轮后续 live run 未拿到连接，未形成新的越过 1024 live 证据。
- Missing-receipts recovery 已接入 RLPx worker：当断线留下 latest header/body available 但 receipt missing，下一条 ready session 会优先发送 `GetReceipts(firstBlockReceiptIndex=0)` 并写入 recovered receipt snapshot；该门禁由 `real_rlpx_worker_recovers_missing_receipts_before_new_header_pull` 覆盖。
- Same-tick sync dispatch 已接入 real RLPx worker：Status 成功后同一 tick 会立刻 drive ready session 并发送首个 `GetBlockHeaders`/sync request，不再等下一 scheduler tick；这减少公网 peer 在 ready 后、首个同步请求前关闭导致的空窗。
- Public RLPx sync batch 已在 `novovm-node` 产品入口收敛：默认 `NOVOVM_ETH_RLPX_HEADERS_BATCH=128`、`NOVOVM_ETH_RLPX_BODIES_BATCH=32`，用于提高产品入口主网追赶吞吐；如果公网 peer 持续出现大 frame 中途 EOF，仍可通过 env 显式下调 body batch。该逻辑只收敛产品入口 worker budget，不改变底层 native fullnode 默认 2048/256 能力。
- Stalled peer refresh 已接入 `novovm-node` 产品入口：当公共 peer churn 导致未全 cooldown 但 `highest > current` 且连续无同步推进时，会按 `NOVOVM_ETH_RLPX_STALLED_REFRESH_INTERVAL_TICKS` 扩容或刷新候选，默认 4 tick；这会覆盖 ready peer 假活跃但不返回 headers/bodies/receipts 的 stalled 场景；refresh 结果会和旧候选池合并去重，避免刷新时因 discovery 本轮返回更少 endpoint 而让候选池缩水；初始候选默认 256、上限 512，自适应上限默认 512、最高 1024，活跃连接与默认 sync/bootstrap fanout 现在由 `NOVOVM_ETH_RLPX_MAX_PEERS=32` 控制，显式配置仍可下调；本轮 40 tick probe 在 `current=1024/highest=25268829` 后触发 `reason=sync_progress_stalled_expand`，候选从 103 刷新到 113；默认 10s discovery + 旧 `MAX_PEERS=8` + body batch 8 的 60 tick trusted-pivot probe 从 `current=25270120` 推进到 `25270160`，最终 `body_available=true` 且无 root/tx/receipt mismatch；stalled 判定放开 ready=0 后的 24 tick trusted-pivot probe 完成 headers/bodies/receipts 并推进到 `current=25270160/highest=25270954`，peer 断开后按 cooldown expand 把 candidates 扩到 19，仍无 root/tx/receipt mismatch；本轮 fanout/batch/cache/highest 单调修正后，真实入口从 `current=611/body_available=false` 连续推进到 `current=1077/body_available=true/receipts=true`，默认 `BODIES_BATCH=8` 多次拿到 `headers=8/bodies=8/receipts=8`；旧默认 `MAX_PEERS=16` 的 live follow-up 从 `current=973/highest=25274693` 推进到 `current=1077/highest=25274898`，peer endpoint cache 会裁掉 runtime `permanently_rejected` 的旧 capability peer，真实 run 中 cache 经 prune/refresh 后保持 242 candidates；`749`/`765`/`829`/`917` 的短 probe 中途断开或短 body 响应导致的 missing body 均由 recovery 恢复，本轮 `1061` 也在 body frame 读到 `313328/334144` 后远端断开、短暂 `body_available=false`，后续 tick 以 `bodies=4/receipts=4` 恢复为 true；runtime sync status 现在还保证 checkpoint/restored `highest` 不会被落后 peer Status 压低，20 tick follow-up 从 `current=941/highest=25274632` 推进到 `current=973/highest=25274667` 且 `highest` 未回退；无同步贡献 peer 的 `subprotocol_error(0x10)` 现在进入 permanent reject，已有 header/body 贡献的 peer 不会被误杀；旧 `eth/66-68` capability mismatch 已进入 permanent reject，auth 阶段 TCP timeout 归入 timeout 生命周期；公网 `too_many_peers`/TCP timeout/mid-body remote close 仍存在。对照本地最新 geth `ProtocolVersions = ETH71, ETH70, ETH69`，eth/68-only peer 仍按不兼容处理，不为追求连接率而偏离当前 geth 产品面。
- Peer discovery total budget 已接入 `novovm-node` 产品入口：`NOVOVM_ETH_RLPX_PEER_DISCOVERY_TOTAL_TIMEOUT_MS` 默认 10s，限制启动/refresh 阶段 DNS+discv4 网络发现总时间；预算耗尽后入口使用已发现 endpoints 加 geth bootnodes fallback 进入 RLPx tick，避免 discovery 阶段阻塞长期同步主循环。真实入口 budget probe 设置 3000ms 时 DNS 在 56 queries/48 endpoints 后触发 budget exhausted，最终以 52 candidates 进入 tick。
- Current-head material match 已接入 `novovm-node` 产品入口：tick 输出、latest head store 和 history store 更新前都会要求 body/receipt 的 number/hash 匹配当前 header；live probe 里 header 3072 到达但 body 未回时 `body_available=false`，body/receipts 回来后才为 true，避免上一块 body 污染当前 head 可观测面；latest head/history store 写入现在还会优先从 current header 对应 canonical runtime block 回填 body/receipt material，避免 recovery 批处理多个块时 single latest body snapshot 覆盖导致 current head store 暂时 `body=null`。
- 本轮真实主网产品入口 follow-up 从 `current=1077/highest=25274898` 推进到 `current=1277/highest=25275268`；期间 block `1093`/`1149` 出现 mid-body remote close 后均由 missing-body recovery 恢复。`1149` 暴露了 retained historical body gaps 会在 `highest > current` 时抢占 header pull 的追高阻塞；现在追高状态只恢复 current/latest missing body，旧缺口不再阻塞 forward headers，修正后从 `1149` 继续推进到 `1181`，block `1181` 保留了非空 ommer body material 和 receipt snapshot；后续重启恢复 `1181` 后继续以 `headers=8/bodies=8/receipts=8` 批次推进 `1189/1197/1205/1221/1229/1237/1245/1253/1261/1269/1277`；block `1213` 在 body frame 读到 `147628/345792` 后远端强断并短暂 `body_available=false`，block `1269` 也出现 header 先到、body 暂缺，后续 ready session 以 `bodies=1/receipts=1` 补齐，最终 checkpoint/head store 在 block `1277` 保持 `body_available=true` 和 receipt available。本轮还修正了产品入口 adaptive candidate 默认值：无显式 `NOVOVM_ETH_RLPX_ADAPTIVE_CANDIDATE_PEERS_MAX` 时不再停在初始 256，而是默认至少 512；live run 在 tick 4 触发 `sync_progress_stalled_expand` 后候选从 249 扩到 273，并在 tick 6/7 恢复 ready 与同步推进。随后从 `1277` 继续的旧默认 16 活跃 peer run 在 candidates 扩到 356 后仍到 tick 15 保持 `ready=0`，主要遇到 `too_many_peers`、pre-auth close 和 TCP timeout；产品默认 active peer 窗口已提升到 32，仍低于本地 geth 默认 `MaxPeers=50`，用于提高公网接入成功率而不改变 EVM 协议语义。
- 默认 32 active peer 的真实主网产品入口验证已完成一轮：未显式设置 `NOVOVM_ETH_RLPX_MAX_PEERS` 时，tick 1 显示 `failures=32`；tick 2/11 拿到 ready/status，tick 12 导入 `headers=8/bodies=8/receipts=8` 并推进 `1277 -> 1285`；tick 15 block `1293` 出现 mid-body close，tick 18 recovery 后 latest head store 恢复 `body_available=true` 和 receipt available，最终 checkpoint/head store 为 `current=1293/highest=25275386`。这证明默认 32 改动改善公网接入并保持 current-head recovery 语义，但仍不是长期主网同步完成证明。
- EVM gateway Engine API probe 已接入：`engine_exchangeCapabilities` 按 geth 风格返回当前可调用 probe methods `["engine_exchangeTransitionConfigurationV1","engine_getClientVersionV1"]`，`engine_exchangeTransitionConfigurationV1` 按 Ethereum mainnet TTD `0xc70d808a128d7380000` + zero terminal block hash/number 回应，`engine_getClientVersionV1` 返回 SUPERVM 自身 identity；其它 `engine_*` payload/forkchoice 控制面仍禁用，避免在没有真实 CL forkchoice/newPayload 语义前对外伪造 geth Engine API 等价。
- Ethereum discv4 discovery 已接入最小主网候选池扩容路径：signed Ping/Pong/random-target FindNode/Neighbors packet build/parse 通过；产品入口会向 geth mainnet bootnodes 做 endpoint proof bonding，收到 bootnode 反向 Ping 后回 Pong，再发随机 target FindNode 并把 public IPv4 Neighbors materialize 成 `enode://` 候选；每个 bonded bootnode 现在按 `NOVOVM_ETH_DISCV4_DISCOVERY_LOOKUPS_PER_BOOTNODE`（默认 4）使用 fresh random FindNode target，并在每轮新增候选或被反向 Ping 时继续 lookup，减少单 target 候选覆盖面不足；混合 IPv4/IPv6 Neighbors 中 unsupported IPv6 会被跳过，不再导致整包失败。实测 discovery-only 4 bootnodes 返回 `endpoints=9` 且 `neighbor_parse_errors=0`；random-target follow-up live run 从单个 bootnode 返回 `endpoints=12`；discv4+DNS 16 tick live run 返回 `discv4 endpoints=29`、`DNS endpoints=15`，RLPx sync 推进到 `current=1024/highest=25267957`。这仍不是完整 discv4 Kademlia table/random walk、discv5 或长稳主网接受度。
- mainline canonical batch append 后会把 persisted block BAL materialize 到 network runtime；对本地已 materialize 的 block BAL payload，响应返回真实 BAL RLP；对未 materialize 的 hash，响应使用 Ethereum RLPx BAL missing sentinel，不伪造 block access list。
- 远端 peer 发送真实非空 `NewBlock` announcement，SUPERVM 按 Ethereum raw transaction trie 校验 `transactionsRoot`，同时保留 empty ommers/withdrawals 校验，再解析并导入 native header/body snapshot，更新 peer head/highest，并继续发 `GetReceipts`；收到 receipts 后按 raw receipt MPT 校验 `receiptsRoot`。
- 远端 peer 发送真实 `NewBlockHashes` announcement，SUPERVM 解析公告高度，更新 peer head/highest，并主动发出后续 `GetBlockHeaders`，不再只依赖初始 `Status` 触发同步。

重启恢复时，latest native head store 和 native history window store 会按已持久化材料恢复 runtime head phase：只有 header 为 `Headers`，已有 body 为 `Bodies`，已有 receipt 为 `State`；这避免已验证 receipts 的 head 在重启后退回 header 阶段重复同步。

DNS discovery 启动阶段现在受 `NOVOVM_ETH_DNS_DISCOVERY_TOTAL_TIMEOUT_MS` 总预算约束，默认 5s，默认 DNS tree max queries 随候选目标收敛到 `min(max(limit*4,16),128)`；DoH TXT 每次查询都会按剩余 global discovery deadline 重新设置 timeout，deadline 已过则直接跳过网络查询。这避免产品入口为了扩充候选池在 DoH/UDP fallback 上长期阻塞，也给 10s peer discovery 总预算里的 discv4 bootnode discovery 留出时间；128-candidate bounded probe 在 10.5s 内进入 tick 并拿到 ready peer；默认 5s DNS budget 的 30 tick trusted-pivot probe 后续 refresh 经 discv4/DNS 把 candidates 扩到 46，并推进到 `current=25270152/body_available=true`、无 root/tx/receipt mismatch，但公网 peer 饱和仍会导致后续同步不稳定。

Public RLPx 默认 Hello/client identity 现在使用 geth-compatible profile，默认能力面与当前本地 geth `ProtocolVersions = [eth/71, eth/70, eth/69]` 对齐，只广告 `eth/69,70,71` + `snap/1`；旧 `eth/66-68` peer 不再协商进入短 Status payload 语义，而是作为 capability mismatch 进入 decode-failure 生命周期过滤；pristine peer 的短 Status/capability mismatch 会立即 permanent reject，Decode 包装的 TCP timeout 和 auth 阶段 TCP timeout 都归入 timeout 生命周期。8 tick live run 未再出现 `rlpx_eth_status_fields_short`，但公网 `too_many_peers`/EOF/TCP timeout 仍未封口。

2026-06-10 geth upstream 复核：本地 `D:\WEB3_AI\go-ethereum` HEAD 为 `43b7b4e8d9f9c8bf74d27bc6b13cb6c90a6128f8`，GitHub API 返回的 `master` SHA 相同；本轮 `git pull`/`git fetch` 在 shell 中遇到 GitHub transport reset/443 timeout，未改变本地仓库。相关 upstream 差异判断如下：`#35130` 只修 `debug_clearTxpool` 测试，`#33347` 只新增 debug RPC 清空本地 txpool，不影响 RLPx/snap/header/body/receipt/root/tx gossip；`#35122` 的 pooled tx hash 发送成功后再标记 known，SUPERVM outbound pooled tx announce 已是 frame 写成功后才标记 propagated；`#35110` 的 BAL 空 `slotChanges` 拒绝已在 raw `BlockAccessLists` 校验中覆盖。结论：不新增 debug RPC、gateway、fixture 或 BAL 产品面；后续只收敛 `Ethereum mainnet long-sync v1`。

当前 v3 结论：tx propagation 的入站/出站网络可观察语义、outbound hash-only pooled tx announce + raw tx response、pooled tx hash/request/response 链路、eth/69+ `BlockRangeUpdate` head refresh、RLPx header/body/receipts sync 链路、入站 `GetBlockHeaders`/`GetBlockBodies` canonical raw RLP 服务面、`NewBlock`/`NewBlockHashes` announcement 路径、`NewBlock`/`BlockBodies` transaction trie root validation、`Receipts` completeness/count/root validation、validated native receipt snapshot + local `GetReceipts` replay、missing-receipts recovery、empty/no-withdrawal stateRoot continuity validation、最小 reorg 回池路径、无 snap BAL 插件请求/响应路径、eth/71 header `BlockAccessListHash` RLP 保真、eth/71 BAL 主动请求/materialize、eth/71 native capability/BAL/snap-offset gate、evm-gateway RLPx eth/71 product surface、snap/1 AccountRange request/response 路径、AccountRange -> StorageRanges/ByteCodes follow-up + native cache 路径、AccountRange/StorageRanges range precondition guard、AccountRange/StorageRanges proof node RLP + root membership + resolvable leaf value guard、partial range 左边界/内部 gap guard、empty AccountRange/StorageRanges proof no-more guard、StorageRanges no-proof complete-range storageRoot validation、StorageRanges last-slotset proof 语义、`novovm-node` 直接 RLPx sync 入口、geth DNS discovery ENR 候选池扩容/UDP fallback/signed root/hash-prefix verification、discv4 bootnode bonding/random-target FindNode/Neighbors 候选池扩容、remote-best sticky target、dead RLPx session cleanup、remote-close transient cooldown、capacity-reject peer 轮换、bootstrap 候选 tie-break 轮换、成功 peer endpoint cache 稳定前置、全冷却候选池扩容刷新和 adaptive 上限后的同容量新候选 refresh、不兼容 Status/capability decode peer 剔除、RLPx checkpoint 恢复、RLPx latest native head store 恢复、RLPx native history window store 恢复和 snap/1 sidecar 空响应服务面已具备真实 gate；BAL 响应已能返回 canonical/materialized 本地 payload，并对缺失 payload 使用协议 missing sentinel。尚未因此声明 eth/71 长稳公网 peer 接受度、长连接主网接受度、完整 peer reputation、完整 geth DNS iterator/link-cache 语义、完整 discv4 Kademlia table/random walk、discv5、完整 BAL 可用性、完整长期历史 block/body/receipt store、完整 geth partial trie reconstruction/minimal proof verification、完整 snap state heal/download/store、完整 state root execution validation 或复杂多分支 reorg 全覆盖。

## 剩余阶段

| 阶段 | 目标 | 退出标准 |
| --- | --- | --- |
| v1 | 插件产品面协议可观察等价 | 本文 3 个聚合 gate 全绿 |
| v2a | RPC 黑盒投影根门禁 | `evm_protocol_observable_equivalence_geth_rpc_blackbox_projection_gate_v2` 全绿 |
| v2b | 真 geth/reth 黑盒差分 | 真实 geth fullTx block diff gate 全绿；raw tx RLP 存在时 `transactionsRoot` 也一致 |
| v3 | 网络可观察等价 | 已覆盖真实 RLPx handshake/Status + 入站 `Transactions` -> pending tx raw RLP + 出站 hash-only `NewPooledTransactionHashes` announce + peer `GetPooledTransactions` raw tx response + pooled tx hash/request/response + eth/69+ `BlockRangeUpdate` head refresh + header/body/receipts import 链路 + 入站 `GetBlockHeaders`/`GetBlockBodies` canonical raw RLP 服务面 + `NewBlock`/`NewBlockHashes` announcement + `NewBlock`/`BlockBodies` transaction trie root validation + `Receipts` completeness/count/root validation + native receipt snapshot/local `GetReceipts` replay + missing-receipts recovery + empty/no-withdrawal stateRoot continuity validation + 最小 reorg 回池 + eth/71 capability/BAL/snap-offset + BAL request/response + snap AccountRange + AccountRange -> StorageRanges/ByteCodes follow-up/native cache/codeHash check + AccountRange/StorageRanges range precondition guard + AccountRange/StorageRanges proof node RLP/root membership/resolvable leaf value guard + partial range 左边界/内部 gap guard + empty AccountRange/StorageRanges proof no-more guard + StorageRanges no-proof complete-range storageRoot validation + StorageRanges last-slotset proof 语义 + `novovm-node` 直接 RLPx sync 入口 + geth DNS discovery ENR 候选池扩容/UDP fallback/signed root/hash-prefix verification + discv4 bootnode bonding/random-target FindNode/Neighbors 候选池扩容 + remote-best sticky target + dead RLPx session cleanup + remote-close transient cooldown + capacity-reject peer 轮换 + bootstrap 候选 tie-break 轮换 + 成功 peer endpoint cache 稳定前置 + 全冷却候选池扩容刷新 + adaptive 上限后的同容量新候选 refresh + 不兼容 Status/capability decode peer 剔除 + RLPx checkpoint/latest native head/native history window store 恢复 + snap sidecar 空响应服务面 |
| v4 | 长稳生产封口 | 多节点 devnet soak、重启恢复、恶意/边界输入、BAL/receipt/RPC 长稳无漂移 |

## 下一步

下一步不再继续堆官方 fixture 子集，除非某个 v2/v3 差分暴露具体缺口。

优先顺序：

1. 先让 v1 三个聚合 gate 进入固定回归清单。
2. 已完成 v2a：RPC block root projection 不再返回 `null`，并进入 geth parity batch report。
3. 已完成 v2b：真实 geth fullTx block fixture 差分已接入，raw tx RLP 进入 canonical block projection，`transactionsRoot` 从 gap 变成 match。
4. 已开始 v3：真实 RLPx handshake/Status + 入站 `Transactions` -> pending tx raw RLP gate 通过；出站 hash-only `NewPooledTransactionHashes` announce + peer `GetPooledTransactions` raw tx response gate 通过；pooled tx hash/request/response gate 通过；eth/69+ `BlockRangeUpdate` head refresh gate 通过；header/body/receipts sync gate 通过；入站 `GetBlockHeaders`/`GetBlockBodies` canonical raw RLP 服务 gate 通过；最小 reorg 回池 gate 通过；`NewBlock`/`NewBlockHashes` gate 通过；`NewBlock`/`BlockBodies` transaction trie root validation、`Receipts` completeness/count/root validation、native receipt snapshot、本地 `GetReceipts` replay、缺 receipt 重连恢复、empty/no-withdrawal stateRoot continuity validation、eth/71 capability/BAL/snap-offset、snap AccountRange、AccountRange -> StorageRanges/ByteCodes follow-up/native cache/codeHash check、AccountRange/StorageRanges range precondition guard、AccountRange/StorageRanges proof node RLP/root membership/resolvable leaf value guard、partial range 左边界/内部 gap guard、empty AccountRange/StorageRanges proof no-more guard、StorageRanges 无 proof 完整范围 storageRoot 校验、StorageRanges last-slotset proof 语义、geth DNS discovery ENR 候选池扩容/UDP fallback、discv4 bootnode bonding/FindNode/Neighbors 候选池扩容、remote-best sticky target、dead RLPx session cleanup、remote-close transient cooldown、capacity-reject peer 轮换、bootstrap 候选 tie-break 轮换、全冷却候选池扩容刷新、adaptive 上限后的同容量新候选 refresh、不兼容 Status/capability decode peer 剔除、RLPx checkpoint/latest native head/native history window store 恢复和 snap sidecar 空响应服务面通过。
5. 如果 v3 或真实 block replay 暴露具体交易类型/root 差异，再补对应最小真实 fixture，不回到开放式 smoke 堆叠。

这会把“等价”从开放式 fixture 堆叠改成有限的协议验收。
