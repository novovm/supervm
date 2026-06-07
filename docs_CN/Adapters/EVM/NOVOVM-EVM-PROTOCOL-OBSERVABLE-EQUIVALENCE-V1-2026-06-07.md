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
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_reorg_gate_v3 -- --nocapture
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_bal_response_gate_v3 -- --nocapture
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_snap_account_range_gate_v3 -- --nocapture
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_snap_account_to_storage_code_gate_v3 -- --nocapture
cargo test -p novovm-network rlpx_snap_rejects_non_empty_account_or_storage_without_proof_v1 -- --nocapture
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
- 本地 pending raw tx 经过 worker budget 触发 `Transactions` outbound broadcast。
- 远端 peer 解析 SUPERVM 发出的 `Transactions` frame，确认 tx hash 和 raw RLP payload 一致。
- 本地 pending tx 标记为 propagated，并记录目标 peer 和 broadcast runtime summary。
- 远端 peer 发送 `NewPooledTransactionHashes` 后，SUPERVM 会按 hash 发起 `GetPooledTransactions`，收到 raw `PooledTransactions` 后 materialize 到 pending tx payload/broadcast candidate。
- 远端 peer 发送 `GetPooledTransactions` 请求本地 pending raw tx 时，SUPERVM 返回协议合法 `PooledTransactions` response，不对未知 hash 返回占位。
- 远端 peer 通过 `BlockHeaders` 返回真实 RLP header，SUPERVM 按 wire header RLP hash 发起 `GetBlockBodies`。
- 远端 peer 通过 `BlockBodies` 返回含 raw tx 的 body，SUPERVM 按 body raw tx 复算并校验 header `transactionsRoot` 后导入 native body snapshot，保留 block hash、tx hash、withdrawal count 和 materialized 状态。
- `BlockBodies` 返回后，SUPERVM 继续发 eth/70 `GetReceipts(firstBlockReceiptIndex=0)`；收到 receipts 后会拒绝 `lastBlockIncomplete=true`、block count mismatch 和 receipt count mismatch，解析 `Receipts(lastBlockIncomplete=false)` 后按 raw receipt MPT 校验 header `receiptsRoot`；对 empty/no-withdrawal block，如果父块已保留，还会校验子块 `stateRoot` 与父块 `stateRoot` 连续；通过后落地 native receipt snapshot、更新 canonical block receipt/stateRoot readiness，并把本轮 peer sync 标记 ready；本地响应 `GetReceipts` 时优先回放已验证 raw receipts，只有能证明空交易 body 时才响应空 receipts，不伪造缺失 receipt 数据。
- 真实 RLPx peer 顺序返回 120A 和 121B 分支，runtime canonical head 从 120A reorg 到 121B。
- 120A 中已 canonical included 的 pending tx 回到 `ReorgedBackToPending`，并携 raw RLP 重新进入 broadcast candidate。
- 无 snap 协商的插件路径里，远端 peer 发送真实 `GetBlockAccessLists` frame，SUPERVM 返回协议合法 `BlockAccessLists` frame，保持 request_id 和 requested hash 数量一致。
- eth/70+snap/1 路径里，global code `0x22/0x23` 归 snap `GetAccountRange/AccountRange`，SUPERVM 在 State phase 使用 native head `stateRoot` 发起 `GetAccountRange` 并记录匹配响应，不让 BAL 插件抢占主网 snap code。
- `AccountRange` 返回非空 slim account 时，SUPERVM 会解析 storage root / code hash，继续发出真实 `GetStorageRanges` 和 `GetByteCodes` 请求；匹配响应会落入 native snap account/storage/code cache，bytecode 在缓存前按 codeHash 校验；这打通 state sync 数据闭环，但还未声明 proof/root verification 或 trie heal 完整。
- snap/1 native cache 入口现在要求非空 `AccountRange` / `StorageRanges` 必须携带 proof，否则按 peer decode failure 拒绝并且不落 cache；这封住未证明非空 state 数据直接进入 native cache 的风险，但仍未声明完整 MPT range proof/root verification。
- 已协商 snap/1 路径里，远端 peer 发送真实 `GetStorageRanges`、`GetByteCodes`、`GetTrieNodes` 请求时，SUPERVM 返回协议合法空 `StorageRanges`、`ByteCodes`、`TrieNodes` 响应，request_id 保持一致；这只封住服务面丢包，不等于完整 snap state heal/download/store。
- `novovm-node` 已新增直接产品入口 `NOVOVM_NODE_MODE=eth_rlpx_sync`，不经过临时脚本即可启动 native Ethereum RLPx worker；真实主网有限 tick 验证中，节点完成 Status、Headers、Bodies、Receipts，native current 从 0 推进到 8192。native capability 现在默认广告/选择 `eth/71`，远端只支持 70 时降级到 `eth/70`；evm-gateway RLPx 产品入口也已广告/选择 `eth/71`，并把 BAL `0x22/0x23` 限定在 negotiated eth/71 下识别，避免 eth/70+snap/1 code 冲突；eth/71 下 BAL 占用 `0x22/0x23`，snap/1 offset 后移到 `0x24`；启用 eth/71 capability 后的 24 tick live run 仍可和旧 peer 降级协商 `negotiated_eth=69` 并推进到 `current=1024/highest=25268137`。入口默认以 Ethereum mainnet geth bootnodes 为基础，并通过 geth DNS discovery root `all.mainnet.ethdisco.net` 解析 ENR 扩展候选池；Google/Cloudflare JSON DoH 对部分 branch 返回空/NXDOMAIN 时，会 fallback 到 UDP TXT 查询，实测 DNS endpoints=28、总 candidates=32，后续同步段来自 DNS peer `65.108.70.101:30303`。DNS root 现在使用 geth signed `enrtree://AKA3AM6LPBYEUDMVNU3BSVQJ5AD45Y7YPOHJLEF6W26QOE4VTUDPE@all.mainnet.ethdisco.net`，默认校验 root signature 和 child TXT `Keccak256(record)` hash prefix；signed DNS + discv4 8 tick live run 返回 `DNS endpoints=5`、`discv4 endpoints=23` 并推进到 `current=1024/highest=25268250`。75 tick live soak 暴露 peer 断线后 `highest` 回落到 local current 的长期追高阻塞点；runtime sync status 已增加短期 remote-best hint，后续 live run 中断线后 `highest` 保持远端高度、不再丢失追高目标（实测 `current=5120` 时 `highest=25267501`）。60 tick live soak 还暴露 EOF/remote-closed 后 dead RLPx stream 留在 session map、导致后续 tick 不继续发下一段 headers 的问题；现在会清理 session，并把 TCP EOF/remote close 记为 transient disconnect 短冷却，减少刚关闭 peer 的立即重连；80 tick live run 仍推进到 `current=6144/highest=25267712`。`too_many_peers` 容量拒绝现在会触发短期 veto/降权，bootstrap 同分候选增加分钟级 rotation bonus，1 tick live snapshot 已出现 `bootstrap_rotation_bonus` reason；96 tick live run 在 48 候选下推进到 `current=8192/highest=25267770`；当所有候选都在 cooldown 且无 ready peer 时，产品入口会按 `NOVOVM_ETH_RLPX_ADAPTIVE_CANDIDATE_PEERS_MAX` 扩容候选并重建 worker，104 tick live run 从 32 自动扩到 64（tick 10）再扩到 128（tick 57），之后仍重新连上 peer 并推进到 `current=5120/highest=25267820`。产品入口还新增默认开启的 checkpoint、latest native head store 和 native history window store：checkpoint 启动时恢复 current/highest，tick 后写回 sync/header 进度，实测临时 checkpoint 可从 `current=1234/highest=5678` 恢复；native head store 会持久化最新已校验 header/body/receipt 并恢复 runtime head，产品入口临时 store 恢复输出 `current=77 highest=99 header_number=77 body_available=true`；native history store 会持久化最近一段 header/body/receipt 并按高度恢复 runtime/canonical head，8 tick live run 写出 `blocks=2` 且推进到 `current=2048/highest=25268092`。当前仍不等于完整 geth DNS iterator/link-cache 语义、discv4 peer churn/长期追高，也不等于完整长期 block/state/receipt durable store 或 eth/71 长稳公网接受度。
- Empty-body receipt materialization 已接入 RLPx 导入路径：当 body 已物化、tx count 为 0 且 header `receiptsRoot` 为 Ethereum empty trie root 时，SUPERVM 会本地生成 empty native receipt snapshot，不再等待远端 `Receipts` 响应；这封住了 live soak 中 block `1024` header/body available 但 `receipt=null` 导致长期不前进的卡点。该门禁由 `rlpx_empty_body_materializes_empty_receipts_without_remote_receipts` 覆盖；本轮后续 live run 未拿到连接，未形成新的越过 1024 live 证据。
- Ethereum discv4 discovery 已接入最小主网候选池扩容路径：signed Ping/Pong/random-target FindNode/Neighbors packet build/parse 通过；产品入口会向 geth mainnet bootnodes 做 endpoint proof bonding，收到 bootnode 反向 Ping 后回 Pong，再发随机 target FindNode 并把 public IPv4 Neighbors materialize 成 `enode://` 候选；混合 IPv4/IPv6 Neighbors 中 unsupported IPv6 会被跳过，不再导致整包失败。实测 discovery-only 4 bootnodes 返回 `endpoints=9` 且 `neighbor_parse_errors=0`；random-target follow-up live run 从单个 bootnode 返回 `endpoints=12`；discv4+DNS 16 tick live run 返回 `discv4 endpoints=29`、`DNS endpoints=15`，RLPx sync 推进到 `current=1024/highest=25267957`。这仍不是完整 discv4 Kademlia table/random walk、discv5 或长稳主网接受度。
- mainline canonical batch append 后会把 persisted block BAL materialize 到 network runtime；对本地已 materialize 的 block BAL payload，响应返回真实 BAL RLP；对未 materialize 的 hash，响应使用 Ethereum RLPx BAL missing sentinel，不伪造 block access list。
- 远端 peer 发送真实非空 `NewBlock` announcement，SUPERVM 按 Ethereum raw transaction trie 校验 `transactionsRoot`，同时保留 empty ommers/withdrawals 校验，再解析并导入 native header/body snapshot，更新 peer head/highest，并继续发 `GetReceipts`；收到 receipts 后按 raw receipt MPT 校验 `receiptsRoot`。
- 远端 peer 发送真实 `NewBlockHashes` announcement，SUPERVM 解析公告高度，更新 peer head/highest，并主动发出后续 `GetBlockHeaders`，不再只依赖初始 `Status` 触发同步。

重启恢复时，latest native head store 和 native history window store 会按已持久化材料恢复 runtime head phase：只有 header 为 `Headers`，已有 body 为 `Bodies`，已有 receipt 为 `State`；这避免已验证 receipts 的 head 在重启后退回 header 阶段重复同步。

DNS discovery 启动阶段现在受 `NOVOVM_ETH_DNS_DISCOVERY_TOTAL_TIMEOUT_MS` 总预算约束，默认 DNS tree max queries 随候选目标收敛到 `min(max(limit*4,16),128)`；这避免产品入口为了扩充候选池在 DoH/UDP fallback 上长期阻塞，短 live run 已验证可按 tick 正常退出，但公网 peer 饱和仍会导致未拿到 ready peer。

Public RLPx 默认能力面现在与当前本地 geth `ProtocolVersions = [eth/71, eth/70, eth/69]` 对齐，只广告 `eth/69,70,71` + `snap/1`；旧 `eth/66-68` peer 不再协商进入短 Status payload 语义，而是作为 capability mismatch 处理。8 tick live run 未再出现 `rlpx_eth_status_fields_short`，但公网 `too_many_peers`/EOF/TCP timeout 仍未封口。

当前 v3 结论：tx propagation 的入站/出站网络可观察语义、pooled tx hash/request/response 链路、RLPx header/body/receipts sync 链路、`NewBlock`/`NewBlockHashes` announcement 路径、`NewBlock`/`BlockBodies` transaction trie root validation、`Receipts` completeness/count/root validation、validated native receipt snapshot + local `GetReceipts` replay、empty/no-withdrawal stateRoot continuity validation、最小 reorg 回池路径、无 snap BAL 插件请求/响应路径、eth/71 native capability/BAL/snap-offset gate、evm-gateway RLPx eth/71 product surface、snap/1 AccountRange request/response 路径、AccountRange -> StorageRanges/ByteCodes follow-up + native cache 路径、`novovm-node` 直接 RLPx sync 入口、geth DNS discovery ENR 候选池扩容/UDP fallback/signed root/hash-prefix verification、discv4 bootnode bonding/random-target FindNode/Neighbors 候选池扩容、remote-best sticky target、dead RLPx session cleanup、remote-close transient cooldown、capacity-reject peer 轮换、bootstrap 候选 tie-break 轮换、全冷却候选池扩容刷新和 adaptive 上限后的同容量新候选 refresh、不兼容 Status decode peer 剔除、RLPx checkpoint 恢复、RLPx latest native head store 恢复、RLPx native history window store 恢复和 snap/1 sidecar 空响应服务面已具备真实 gate；BAL 响应已能返回 canonical/materialized 本地 payload，并对缺失 payload 使用协议 missing sentinel。尚未因此声明 eth/71 长稳公网 peer 接受度、长连接主网接受度、完整 geth DNS iterator/link-cache 语义、完整 discv4 Kademlia table/random walk、discv5、完整 BAL 可用性、完整长期历史 receipt store、完整 snap proof/root verification、完整 snap state heal/download/store、完整 state root execution validation 或复杂多分支 reorg 全覆盖。

## 剩余阶段

| 阶段 | 目标 | 退出标准 |
| --- | --- | --- |
| v1 | 插件产品面协议可观察等价 | 本文 3 个聚合 gate 全绿 |
| v2a | RPC 黑盒投影根门禁 | `evm_protocol_observable_equivalence_geth_rpc_blackbox_projection_gate_v2` 全绿 |
| v2b | 真 geth/reth 黑盒差分 | 真实 geth fullTx block diff gate 全绿；raw tx RLP 存在时 `transactionsRoot` 也一致 |
| v3 | 网络可观察等价 | 已覆盖真实 RLPx handshake/Status + 入站 `Transactions` -> pending tx raw RLP + 出站 `Transactions` broadcast + pooled tx hash/request/response + header/body/receipts import 链路 + `NewBlock`/`NewBlockHashes` announcement + `NewBlock`/`BlockBodies` transaction trie root validation + `Receipts` completeness/count/root validation + native receipt snapshot/local `GetReceipts` replay + empty/no-withdrawal stateRoot continuity validation + 最小 reorg 回池 + eth/71 capability/BAL/snap-offset + BAL request/response + snap AccountRange + AccountRange -> StorageRanges/ByteCodes follow-up/native cache/codeHash check + `novovm-node` 直接 RLPx sync 入口 + geth DNS discovery ENR 候选池扩容/UDP fallback/signed root/hash-prefix verification + discv4 bootnode bonding/random-target FindNode/Neighbors 候选池扩容 + remote-best sticky target + dead RLPx session cleanup + remote-close transient cooldown + capacity-reject peer 轮换 + bootstrap 候选 tie-break 轮换 + 全冷却候选池扩容刷新 + adaptive 上限后的同容量新候选 refresh + 不兼容 Status decode peer 剔除 + RLPx checkpoint/latest native head/native history window store 恢复 + snap sidecar 空响应服务面 |
| v4 | 长稳生产封口 | 多节点 devnet soak、重启恢复、恶意/边界输入、BAL/receipt/RPC 长稳无漂移 |

## 下一步

下一步不再继续堆官方 fixture 子集，除非某个 v2/v3 差分暴露具体缺口。

优先顺序：

1. 先让 v1 三个聚合 gate 进入固定回归清单。
2. 已完成 v2a：RPC block root projection 不再返回 `null`，并进入 geth parity batch report。
3. 已完成 v2b：真实 geth fullTx block fixture 差分已接入，raw tx RLP 进入 canonical block projection，`transactionsRoot` 从 gap 变成 match。
4. 已开始 v3：真实 RLPx handshake/Status + 入站 `Transactions` -> pending tx raw RLP gate 通过；出站 `Transactions` broadcast gate 通过；pooled tx hash/request/response gate 通过；header/body/receipts sync gate 通过；最小 reorg 回池 gate 通过；`NewBlock`/`NewBlockHashes` gate 通过；`NewBlock`/`BlockBodies` transaction trie root validation、`Receipts` completeness/count/root validation、native receipt snapshot、本地 `GetReceipts` replay、empty/no-withdrawal stateRoot continuity validation、eth/71 capability/BAL/snap-offset、snap AccountRange、AccountRange -> StorageRanges/ByteCodes follow-up/native cache/codeHash check、geth DNS discovery ENR 候选池扩容/UDP fallback/signed root/hash-prefix verification、discv4 bootnode bonding/random-target FindNode/Neighbors 候选池扩容、remote-best sticky target、dead RLPx session cleanup、remote-close transient cooldown、capacity-reject peer 轮换、bootstrap 候选 tie-break 轮换、全冷却候选池扩容刷新、adaptive 上限后的同容量新候选 refresh、不兼容 Status decode peer 剔除、RLPx checkpoint/latest native head/native history window store 恢复和 snap sidecar 空响应服务面通过。
5. 如果 v3 或真实 block replay 暴露具体交易类型/root 差异，再补对应最小真实 fixture，不回到开放式 smoke 堆叠。

这会把“等价”从开放式 fixture 堆叠改成有限的协议验收。
