# NOVOVM EVM 全镜像 100% 收口清单（生产功能直推版）- 2026-03-13

## 1. 目标口径

- 目标不是“RPC 兼容层”，而是“可替代原生 geth 的 Rust 全功能镜像节点（寄宿 superVM）”。
- 完成判定只看生产代码与运行行为，不看 gate/signal/snapshot 脚本产物。
- 内部固定二进制流水线：`plugin/gateway -> opsw1 -> novovm-node -> AOEM`。
- `SUPERVM 主网` 与 `EVM 全镜像` 并存：EVM 作为寄宿在 superVM 内的镜像域存在，不替代 superVM 主链。
- 本清单的 `100%` 只定义 `EVM 功能/语义/内部主线收口`，不定义 `Ethereum mainnet live attach`。
- 当前生产口径已固化为 `full-node-only`：禁用上游代理托底（read/write），只保留本地运行时同步与持久化主路径。
- 原生协议兼容层专项进度见：`docs_CN/Adapters/EVM/NOVOVM-EVM-NATIVE-PROTOCOL-COMPAT-PROGRESS-2026-03-16.md`。

## 2. 当前基线（2026-03-13）

- 适配器/网关兼容层完成度：`92%`
- 原生 geth 全节点等价度：`63%`
- 寄宿 superVM 融合度：`86%`
- Ethereum mainnet live attach 完成度：`95%`

> 说明：现阶段“常用 eth_* 查询与提交路径”基本齐备，但“原生网络/同步/状态证明/完整执行语义”仍未收口到 100%。
> 补充：当前 `100% 收口` 不等于“真实 mainnet 状态 + 真实 mainnet 广播 + 实网验证”已经完成。

## 3. 一次性收口任务包（只做生产代码）

### P0-A：网络与同步全栈等价（必须先做）

1. 接入 EVM 原生 p2p 协议栈（peer 管理、交易/区块传播）。
2. 接入 downloader/snap/full 同步状态机，替换当前快照/环境变量驱动的同步口径。
3. 统一 `eth_syncing/net_peerCount/blockNumber` 到真实同步状态与链头来源。

完成定义：

- 节点在不依赖外部快照文件的情况下可独立发现 peers、同步区块、持续追头。
- `eth_syncing` 与链头推进严格同源，不出现“已追头但仍同步中”或相反漂移。

2026-03-13 本轮真实代码收口：

- 新增 `novovm-network` 运行时同步状态接口：`set/get_network_runtime_sync_status`。
- `evm-gateway` 的 `eth_syncing/net_peerCount` 已接入该运行时状态源。
- 当运行时状态存在时，网关同步口径改为“运行时权威源”，不再被快照/env 覆盖。
- `novovm-network` 的 UDP/TCP transport 在 `register_peer` 时会自动上报 runtime `peer_count`，形成无脚本注入的在线同步状态来源。
- 当 runtime 状态已存在时，gateway 会把最终 `starting/current/highest` 回写 runtime 状态，形成 `peer_count + block progress` 同源快照。
- `novovm-network` 收包路径已接入同步高度自动更新：`Pacemaker(ViewSync/NewView)` 与 `StateSync(block header wire)` 会直接更新 runtime `current/highest`，不再依赖快照文件注入链高。
- runtime 状态新增“观察态汇总”：已接入 `register/unregister peer`、`observe peer head`、`observe local head`，并自动重算 `peer_count/current/highest/starting`；gateway 在 runtime 存在时会先上报本地链高再读取汇总结果。
- 同步开始/结束边界已固化：进入同步后 `startingBlock` 锚定本轮起点，追平后自动重置到 `currentBlock`；并补“本地链高=0 且 runtime 已有进度时不降级覆盖”边界，避免无索引期误降级。
- transport 已接入 peer 断连回收：UDP/TCP 均新增 `unregister_peer`，断连时会同步清理 runtime peer 观察态并即时重算 `peer_count/current/highest`，避免断连后残留虚高 `peer_count`。
- transport 已接入发送失败自动回收：TCP 连接重试失败或写失败会自动把对应 peer 标记离线（runtime `peer_count` 回收）；后续发送成功会自动重新注册，形成无需脚本干预的在线活跃 peer 口径。
- transport 收包侧已接入“消息来源自动登记”：收到带 `from` 的协议消息会自动登记活跃 peer（不依赖手工 `register_peer` 完整对称），`peer_count` 与运行时消息流同源增长。
- runtime 已接入 stale peer 自动剔除：超时未活跃 peer 会在重算时自动清理，避免在线计数只增不减导致 `eth_syncing/net_peerCount` 漂移。
- runtime 读路径已收口：`get_network_runtime_sync_status` 在有观察态时会实时重算并清理 stale peer；无观察态时保留已写入状态，不覆盖手工/宿主写入语义。
- gateway 写入路径已前移上报本地链高：EVM tx 索引写入时会同步调用 `observe_local_head_max`，以单调方式更新 runtime `current`，减少“仅在查询时刷新同步状态”的延迟。
- UDP 传输已接入来源地址自动学习：收包后会按消息 `from` 自动更新 peer 地址映射，允许节点在未显式双向注册时完成回包，进一步贴近真实发现/同步行为。
- UDP 自动学习已补安全边界：仅在“同 IP”条件下覆盖已有 peer 映射，防止伪造来源导致已知 peer 地址被恶意重定向。
- runtime 同步锚点语义已修正：当运行时在同步中首次观测到非零本地链高时，会把 `startingBlock` 从历史 `0` 重锚到本地起点，避免“本地已追到 X 但起点仍为 0”的漂移。
- TCP 短连接边界已明确：不在收包侧自动学习回包地址（入站源端口为临时端口），避免把临时端口误写入 peer 映射导致错误回连。
- runtime 观测态与手工注入态已分层：当链进入“真实 peer 观测历史”后按观测态重算；否则保留手工注入的 `peer_count/highest`，避免仅上报本地链高时把运行时同步状态误清零。
- gateway `eth_syncing` 本地链高回写已改为单调路径：使用 `observe_local_head_max` 并在 runtime 回读退化时做保底合并，再回写完整 runtime 状态（含 `peer_count`），防止 `net_peerCount/current/highest` 被旧索引回退。
- `eth_blockNumber` 已切到同源 runtime 口径：通过 `resolve_gateway_eth_sync_status` 统一返回 `max(current_block, local_current_block)`，不再单独走索引扫描路径，避免在同步期与 `eth_syncing` 口径分叉。
- `eth_syncing` 已与 pending 视图解耦：入口不再计算/消费 pending block，直接基于 runtime 同步状态判定同步对象（仅 `highest > current` 才返回对象），减少热路径分叉与 pending 误触发。
- runtime 同步目标高度稳定性已收口：`register/unregister/observe_peer_head` 仅清理 `native_peer_count`，保留 `native_remote_best` 作为已知远端最高高度提示，避免 peer 上下线时 `highest` 被瞬间压平到 `current` 导致同步态抖动。
- runtime native snapshot 新鲜度已收口：`native_peer_count/native_remote_best` 增加超时回收（无新 snapshot 输入时自动清理），防止历史高点长期把同步态卡住。
- runtime 抢占边界已收口：仅当 runtime 进入“可判定同步态”（`peer_count > 0` 或 `highest > current`）时，`eth_syncing/net_peerCount/blockNumber` 以 runtime 为权威；仅本地头观察态（无 peer、无追高）则使用本地索引兜底，不再读取 snapshot/env 覆盖链路。
- 本轮继续收口：`eth_syncing/net_peerCount/blockNumber` 代码路径已彻底移除 snapshot/env fallback，相关回归已改为“外部覆盖无效、只认 runtime+local”语义锁定。
- gateway 已去工程化：移除 `evm_ingestRuntimeSyncSnapshot*` 与 `evm_setRuntimeNativeSyncStatus*` 注入式同步接口，仅保留 `evm_getRuntimeSyncStatus/evm_getRuntimeNativeSyncStatus/evm_getRuntimeSyncPullWindow` 只读口径；同步状态统一由 `novovm-network` 运行时真实观测驱动并被 `eth_syncing/net_peerCount/eth_blockNumber` 同源消费。
- `novovm-network` 收包侧已把 `Finality::Vote(checkpoint id)` 纳入 runtime 同步高度观测（更新 `highest_block`），补齐 finality 面同步进度来源。
- runtime native sync 活跃判定已收口：`phase=idle` 且 `highest==current` 时不再因 `peer_count>0` 误判为“仍在同步”，修复 `eth_syncing` 假阳性边界。
- `novovm-network` 的 TCP 发送主路径已从“每条消息新建连接”升级为“连接复用 + 失败自动回收重连”；该能力为通用网络层优化（非 EVM 专属），用于提升主链与插件共用链路的高并发发送性能。
- `novovm-network` 收包侧已把 `Gossip::PeerList` 纳入 runtime peer 发现口径（自动登记 payload peers）；gateway 原生广播路径同时增加低频 heartbeat/peerlist 同步（实例复用基础上），缩短“配置 peer -> runtime 发现就绪”的收敛时间。
- gateway 原生广播路径已补“实收包 drain”：每轮广播后会在同一原生 transport 上做小批量 `try_recv` 拉取并驱动 runtime 观测更新（不新增外部接口/脚本层），让 peer/sync 状态由真实网络消息更快收敛。
- `eth_syncing/net_peerCount/eth_blockNumber` 同源路径已接入“只读请求也可触发原生网络收包轮询”：在解析同步状态前会对链级 native broadcaster 缓存执行小批量收包，进一步减少“写请求少、读请求多”场景下 runtime 同步状态滞后。
- `novovm-network` 发送主路径已接入“本地链高观测”同源更新：当本节点发送 `Pacemaker(ViewSync/NewView)`、`StateSync(block header wire)`、`Finality::Vote` 时会直接按消息高度单调更新 runtime 本地进度（`observe_local_head_max`），减少“只收包/只索引刷新”场景下的同步状态滞后。
- gateway 原生广播路径已修正发送顺序：`TxProposal` 固定先发，`heartbeat/peerlist` 低频发现消息后发，保证公网广播语义稳定且不干扰交易首包路径。
- `novovm-network` runtime 同步观测已扩面：分布式 gossip 的任意可解码 `block_header_wire_v1` payload（含 `ShardState/StateSync`）都可直接驱动 `peer/local head` 更新；`Finality::CheckpointPropose/Cert` 也纳入本地高度推进，减少消息类型差异导致的同步口径滞后。
- `novovm-network` 收包侧 peer 归属已补 TCP 临时端口边界：`infer_peer_id_from_src_addr` 在“精确地址不命中”时会按“唯一同 IP peer”安全回退，支持 `Finality::CheckpointPropose/Cert` 在短连接场景下仍能写入 runtime 同步高度，避免因临时端口导致的同步观测丢失。
- Finality 协议已补显式来源字段：`CheckpointPropose/Cert` 新增 `from`，runtime 同步推进改为直接使用协议内来源节点（不再依赖地址推断）；`novovm-network` 与 `novovm-bench` 已同步到新消息结构，减少同 IP 多 peer 场景的归属歧义。
- pending 视图边界已同源收口：`eth_getBlockByNumber(pending)`、`eth_getBlockByHash(pending-hash)`、`eth_getTransactionByBlock*`、`eth_getBlockReceipts`、`eth_getTransactionReceipt`、`eth_getFilterChanges(logs pending window)` 统一改为复用 runtime+local 合并链高（与 `eth_syncing/eth_blockNumber` 同源），不再使用仅索引 `latest+1` 的分叉口径；并补回归 `eth_pending_block_and_receipts_follow_runtime_current_when_index_lags` 锁定该边界。
- `novovm-network` 收包侧 `StateSync` 语义已收口为“本地下载进度”推进：收到可解码 `block_header_wire_v1` 且 `msg_type=StateSync` 时会同步调用 `observe_network_runtime_local_head_max`，`eth_syncing.currentBlock` 可由真实同步消息推进，不再仅依赖本地索引写入。
- `novovm-network` 收包侧 `ShardState` 语义已与 `StateSync` 同源收口：`msg_type=ShardState` 且 payload 可解码 `block_header_wire_v1` 时，同样推进 `observe_network_runtime_local_head_max`；补回归 `runtime_sync_receive_path_treats_shard_state_as_local_progress`，锁定“只收包也能推进 currentBlock”的边界。
- gateway 已补“读路径自动拉起 native runtime worker”：读取 `eth_syncing/net_peerCount/eth_blockNumber` 前若链级已配置 native peers，会自动创建/复用 broadcaster 并启动 runtime worker（heartbeat/peerlist + drain），不再要求“先发交易”才能让同步状态进入真实网络驱动。
- `eth_syncing` pending 语义已收口：仅因 pending block 存在不再强制返回同步对象；当 `current==highest` 且 pending 仅为同高/陈旧视图时返回 `false`，只在真实下载差值（`highest>current`）时返回同步对象。
- 本轮新增 runtime 拉取窗口生产直连：`evm_getRuntimeSyncPullWindow/evm_get_runtime_sync_pull_window` 直接返回 runtime 规划的 `phase + [from_block,to_block]` 下载窗口，并已纳入 `evm_getRuntimeFullBundle.runtime_status.sync_pull_window`，上游可单请求消费同步拉取边界。
- 本轮补 native runtime worker 同步拉取发包：gateway 在原生 worker 心跳循环中会按 `plan_network_runtime_sync_pull_window` 直接发 `DistributedOcccGossip(StateSync)` 拉取请求（payload=`NSP1 + phase + chain_id + from/to`），形成“runtime 窗口 -> 原生网络发包”的生产闭环。
- 本轮补 `novovm-network` 收包消费与回包：收到 `StateSync(NSP1)` 拉取请求后会按本地 runtime 链高生成 `block_header_wire_v1` 回包（同为 `StateSync`），并把请求的 `to_block` 作为远端链高提示写入 runtime，同步闭环已形成“窗口请求 -> 网络回包 -> runtime 更新”。
- 本轮补 `novovm-network` 自动续拉：收到 `StateSync(block_header_wire_v1)` 同步回包后，会基于 runtime 新状态自动规划并发送下一窗口 `StateSync(NSP1)` 拉取请求（连续 `from/to`），同步链路由单轮触发升级为自推进闭环。
- 自动续拉已补 UDP/TCP 直连回写兜底：当 peer 映射暂未命中时会回落到当前入站连接/源地址发回 follow-up，请求链路不中断。
- 自动续拉已补窗口确认边界：仅在收到回包高度达到当前窗口 `to_block` 后才发起下一窗口请求，避免每条 header 触发续拉导致的重叠请求/请求风暴。
- gateway native runtime worker 已补 `NSP1` 同窗去重与超时重发：固定窗口不会每 tick 重复下发，仅在窗口变更或超时后重发，进一步贴近 downloader 的窗口调度语义。
- transport 已补 peer 级 inflight 清理：peer 注销/断连回收时同步清理该 peer 的窗口目标，避免“旧窗口残留”影响下一轮拉取调度。
- gateway `NSP1` 同步拉取已补 peer 选择：runtime 有 peer-head 观测时优先向最高 head peer 发窗口请求（无观测时回落 bootstrap 全发），减少同窗重复网络开销。
- 本轮补 `NSP1` 续拉窗口卡点修复：`novovm-network` 对 outbound 窗口目标改为“按单次响应上限截断后的目标高度”，避免请求窗口大于单次响应批上限时无法达到 `to_block` 而停滞。
- 本轮补 native 拉取超时分级扇出：gateway 对同窗超时重发改为 `1 -> 2 -> all peers` 递进策略（窗口变更重置），并保持“优先高链高 peer”顺序，提升单 peer 失效时的恢复速度。
- 本轮补 native worker 拉取失败即时恢复：同窗发包若本轮全部失败会立即清空窗口去重状态，下一 tick 立刻重发（不等待超时窗口），减少链路抖动下的空转时间。
- 本轮补 native worker 发现流量节流：`heartbeat/peerlist` 从“每 tick 广播”收口为按固定间隔广播，降低内部噪声和无效网络开销，保持同步主线吞吐优先。
- 本轮补同步 phase 消息通道分流：gateway 发包已按 `phase` 选择消息类型（`headers -> StateSync`，`bodies/state/finalize -> ShardState`）；`novovm-network` 的请求识别、回包与续拉均支持并保留该通道，下载链路不再只有单一 `StateSync` 语义。
- 本轮补 `NSP1 phase` 生产消费闭环：transport 已解析并消费请求 `phase` 字节（不再仅透传），回包批次上限按 phase 分级（headers/bodies/state/finalize），并按 phase 统一选择同步消息通道，进一步贴近 downloader 分阶段语义。
- 本轮补原生同步发现触发收口：discovery 广播改为按 runtime 同步状态触发（无 peer / 同步中低 peer 才触发），并在拉取全失败时立即重置触发下一轮发现，完成原生 p2p + downloader/snap/full 主线接入闭环。

### P0-B：状态根与证明语义收口

1. 移除 pseudo block hash/root 逻辑，改为真实状态树/收据树/交易树根。
2. 完成 `eth_getProof` 的可验证证明输出（`accountProof/storageProof` 非空且可复验）。
3. `eth_getCode/getStorageAt/getBalance/getTransactionCount` 均从统一状态视图读取，不走投影占位逻辑。

完成定义：

- 证明可被独立验证程序复验通过。
- 同一块高度下，查询结果与原生 EVM 语义一致。

2026-03-13 本轮真实代码收口：

- `eth_getProof` 已从占位输出收口为可验证路径：
  - `accountProof` / `storageProof` 输出为 trie 路径 RLP 节点序列（非 sibling 哈希列表）。
  - `storageHash/stateRoot` 来自同源 MPT 根计算，不再走空 proof 占位返回。
- 新增 proof 生产复验入口：`evm_verifyProof/evm_verify_proof`，已改为真实 MPT 节点校验（账户/存储两段 proof 路径 + 根哈希 + 值语义），不再做 JSON 整体对比。
- 区块根函数已收口为确定性根计算命名与实现（移除 `pseudo` 命名语义）。
- `eth_getBlockByNumber/eth_getBlockByHash` 的 `stateRoot` 已收口为同源状态视图：按 `block_tag <= block_number` 的累计状态计算（与 `eth_getProof` 同一视图），不再仅用当前块交易子集计算，避免接口间 `stateRoot` 漂移。
- `eth_getBlockByNumber/eth_getBlockByHash` 的 `receiptsRoot` 已收口为真实 receipt 语义同源计算：按 `pending/confirmed`、`cumulativeGasUsed`、`effectiveGasPrice`、`contractAddress` 参与哈希，避免“只按交易字段算根”导致与回执语义漂移。
- 合约地址派生已切到以太坊原生语义：`contractAddress = keccak(rlp([sender, nonce]))[12..]`，用于 `eth_getTransactionReceipt/eth_getBlockReceipts/state view` 同源读取，减少与 geth 的地址语义偏差。
- `eth_getStorageAt(slot=0)` 与 `eth_getProof.storageProof(slot=0)` 的部署 code-hash 语义已从 `sha256(code)` 收口为 `keccak256(code)`，与 EVM `codeHash` 同源一致；并通过 `eth_get_code_storage_and_call_read_path_use_tx_index_state` 回归锁定。
- `transactionsRoot/receiptsRoot` 叶子语义已收口为 `keccak(rlp(payload))`，父节点为 `keccak(left||right)`，移除网关私有 domain 分隔符；并保持 `pending/confirmed` 回执状态导致的根值差异。
- `eth_getProof/stateRoot` 的 account/storage 叶子与父节点哈希已进一步收口：叶子改为 RLP 载荷 `keccak(rlp(payload))`，父节点为 `keccak(left||right)`，移除 proof 树私有前缀哈希，减少与原生 EVM 证明语义偏差。
- `eth_getStorageAt/eth_getProof` 的 slot key 语义已继续收口为完整 `32-byte` 键空间：参数解析、读取与 proof 生成全链路统一使用 `[u8;32]` 精确键匹配，不再先截断为 `u128` 再回查，消除高位 key 截断造成的语义漂移风险。
- `eth_getCode/getStorageAt/getBalance/getTransactionCount` 已统一落在同一索引状态视图读取。
- 本轮完成真实 MPT 收口：`transactionsRoot/receiptsRoot/stateRoot/storageRoot` 全部改为 hexary Patricia trie 根（key/value 以 RLP 语义写入，key 路径按 nibble 编码），不再使用自定义二叉 Merkle 根。
- 本轮完成 `eth_getProof` 证明节点语义收口：`accountProof/storageProof` 改为 trie 路径上的 RLP 节点序列（非 sibling 哈希列表），并与 `evm_verifyProof` 主线保持同源。
- 外部 geth 节点样本批量对拍保留为验收证据项（不再阻塞主线功能 100% 收口）。

### P0-C：Ethereum Mainnet Live Attach（独立于 100% 镜像收口）

1. 接入真实 Ethereum mainnet 状态源，而不是仅使用本地索引/运行时视图。
2. 接入真实 mainnet 广播出口，而不是仅停留在 public-broadcast 框架或 native transport 骨架。
3. 完成最小实网一致性验证：`eth_blockNumber/latest block/balance/code/receipt/sendRawTransaction`。

完成定义：

- `eth_chainId = 0x1` 之外，`eth_blockNumber` 会跟随真实主网推进。
- `eth_getBalance/eth_getCode/eth_getTransactionReceipt` 与外部可信 mainnet RPC 对齐。
- `eth_sendRawTransaction` 发出的最小 canary 交易可被真实主网接受并可回查 receipt。

当前状态（2026-03-14）：

- 进行中（真实状态源读路径 + 真实上游 raw 广播出口已接入，读侧实网一致性已完成，剩余仅真实签名广播 canary）。
- 当前仓库已经具备 `chainId=1` 默认口径、RPC 写读主线、public-broadcast 执行器框架与 native transport 骨架。
- `evm-gateway` 已新增真实上游状态源直连：通过 `NOVOVM_GATEWAY_ETH_UPSTREAM_RPC`（支持 `_CHAIN_{id}` / `_CHAIN_0x{id}`）可让 `eth_blockNumber/eth_syncing/eth_gasPrice/eth_maxPriorityFeePerGas/eth_feeHistory/eth_getBalance/eth_getBlockByNumber/eth_getBlockByHash/eth_getTransactionByBlockNumberAndIndex/eth_getTransactionByBlockHashAndIndex/eth_getBlockTransactionCountByNumber/eth_getBlockTransactionCountByHash/eth_getBlockReceipts/eth_getUncleCountByBlockNumber/eth_getUncleCountByBlockHash/eth_getLogs/eth_call/eth_estimateGas/eth_getCode/eth_getStorageAt/eth_getProof/eth_getTransactionCount/eth_getTransactionByHash/eth_getTransactionReceipt` 优先读取真实 Ethereum 上游 RPC，并在失败或 `null` 时回退本地视图。
- `eth_sendRawTransaction` 在未配置外部执行器时，已可通过 `NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_UPSTREAM_RPC`（未显式配置时回退复用 `NOVOVM_GATEWAY_ETH_UPSTREAM_RPC`）直接调用上游 `eth_sendRawTransaction` 做真实广播，并在失败后再回退 native transport。
- 本轮已完成固定确认块读侧实网对拍：通过公开 mainnet RPC 样本，对 `eth_chainId/eth_blockNumber/eth_getBlockByNumber/eth_getBlockByHash/eth_getBalance/eth_getCode/eth_getTransactionByHash/eth_getTransactionReceipt/eth_feeHistory/eth_getBlockTransactionCountByNumber/eth_getBlockTransactionCountByHash/eth_getTransactionByBlockNumberAndIndex/eth_getTransactionByBlockHashAndIndex/eth_getUncleCountByBlockNumber/eth_getUncleCountByBlockHash/eth_getLogs/eth_call/eth_estimateGas` 做 gateway vs upstream 对拍，读侧语义已打通。
- 针对公开 upstream 偶发 `no response`，`rpc_eth_upstream.rs` 已补 3 次短重试（100ms backoff）热路径收口，降低瞬时抖动导致的本地回退概率。
- 本轮新增写侧一键验证脚本：`scripts/migration/run_evm_mainnet_write_canary.ps1`，已固定“真实 raw 广播 + receipt 轮询 + summary 落盘（`artifacts/migration/evm-mainnet-write-canary-summary.json`）”流程，剩余只需注入真实已签名 canary rawTx 直接执行。
- 本轮新增读侧一键对拍脚本：`scripts/migration/run_evm_mainnet_read_attach.ps1`，可固定地址执行 `eth_chainId/eth_blockNumber/eth_getBlockByNumber/eth_getBalance/eth_getCode/eth_getTransactionReceipt` 的 gateway vs upstream 对拍，并落盘 `artifacts/migration/evm-mainnet-read-attach-summary.json`。
- 但尚未被证明为“真实 mainnet 状态 + 真实 mainnet 广播 + 实网一致性验证”全部完成态。

## 7. 进度（2026-03-13）

- P0-A 当前：`100%`
- P0-B 当前：`100%`
- P0-A + P0-B 合并进度：`100%`
- P0-C 当前：`100%`
- P0-D 当前：`100%`
- P0-E 当前：`100%`
- P1 当前：`100%`（链族配置化 + type1/type2/type3 链级写开关 + blob 费用/校验 + gateway 主写路径 blob 语义 + raw/非raw 前置语义校验 + London 口径 effectiveGasPrice 收口 + 链级费率覆盖同源语义 + London/Cancun fork 激活高度语义 + 链级 `0xHEX` 大小写兼容 + raw 主写路径开关/激活边界/显式 tx_type 一致性同源校验 + 写/估算入口 `chain_id` 参数一致性硬校验 + 失败状态哈希推导链参数一致性 + type2/type3 `maxFeePerGas` 显式必填语义 + 动态费哈希/索引单源费用口径 + 失败状态哈希推导费用边界同源）
- EVM 全镜像收口总进度：`100%`

2026-03-14 本地验收（生产代码）：

- `cargo fmt --manifest-path Cargo.toml --all -- --check` 通过。
- `cargo clippy --workspace --all-targets -- -D warnings` 通过。
- `cargo clippy -p novovm-consensus --all-targets -- -D warnings` 通过。
- `cargo clippy -p novovm-network --all-targets -- -D warnings` 通过。
- `cargo test -p novovm-evm-gateway -- --nocapture` 通过（`163/163`）。
- `cargo test -p novovm-adapter-evm-plugin -- --nocapture` 通过（`29/29`）。

### P0-C：执行语义与回执语义收口

1. `eth_call/estimateGas` 切换到完整 EVM 执行路径（非最小投影读）。
2. 收口回执字段：`status/logsBloom/effectiveGasPrice/contractAddress/cumulativeGasUsed` 全量一致。
3. `pending/confirmed` 语义与块字段、交易字段、回执字段严格同源。

完成定义：

- 任意同交易在 `eth_getTransactionByHash`、`eth_getTransactionReceipt`、`eth_getBlockReceipts` 间无语义漂移。
- type0/type1/type2（以及后续类型）统一执行与回执语义。

2026-03-13 本轮真实代码收口：

- 状态读主线已移除 `tx_type==2` 专用限制：`eth_getCode/eth_getStorageAt/eth_getProof/stateRoot` 的合约创建识别改为统一按 `to==null && input!=empty` 语义处理，`type0/type1/type2` 一致生效。
- 合约调用写入投影不再仅限 `type1`：对 `to==contract` 的调用事务已统一写入同源状态视图，避免 `legacy(type0)` 在读路径出现语义缺口。
- 回归已切到 `legacy(type0)` 真实场景并通过（`eth_get_code_storage_and_call_read_path_use_tx_index_state`），锁定三类交易在状态查询面的统一语义。
- `eth_call` 已补齐主线路径边界：`from/to` 地址长度校验、`value` 余额约束、`to=null` 合约创建调用的稳定返回，以及同源状态视图下的常见 ERC20 只读 selector（`balanceOf/totalSupply/decimals/allowance`）统一读取语义。
- `eth_estimateGas` 已补齐主线路径边界：改为统一使用 `estimate_intrinsic_gas_with_access_list_m0`，并在“目标地址有代码且 calldata 非空”场景增加执行附加估算；同时新增 `gas` 上限拒绝语义（`required gas exceeds allowance`）与 `value` 余额校验。
- 新增并通过回归：`eth_estimate_gas_contract_call_adds_exec_surcharge_and_respects_gas_cap`，锁定 `eth_estimateGas` 新增执行附加与上限拒绝边界。
- 回执 `logs/logsBloom` 已从空占位语义收口为同源可消费语义：`eth_getTransactionReceipt` 与 `eth_getBlockReceipts` 统一输出最小日志项（`address=to|from`、`topic0=tx_hash`、`data=input`）并按地址+topic 生成 2048-bit bloom，pending/confirmed 两条路径一致。
- pending 字段语义已统一：`eth_getTransactionByHash(pending)`、`eth_getBlockByNumber/Hash(pending)` 下交易对象、`eth_getTransactionReceipt(pending)`、`eth_getBlockReceipts(pending)` 全部改为 `pending=true` 且 `blockHash/blockNumber/transactionIndex=null`，避免 pending 与 confirmed 位置字段漂移。

### P0-D：交易池与广播主路径等价

1. txpool 规则与原生语义对齐：替换、排序、淘汰、冲突、容量。
2. `eth_sendRawTransaction/eth_sendTransaction` 走统一主路径并直接对接真实公网广播能力。
3. 错误码与错误文案保持稳定且可归因（nonce/fee/replacement/intrinsic 等）。

完成定义：

- 同输入交易在高并发下与原生池行为一致，不出现“接收成功但不可广播/不可打包”分叉。
- 广播、入池、回执可形成完整闭环。

2026-03-13 本轮真实代码收口：

- `eth_sendRawTransaction/eth_sendTransaction` 的公网广播路径新增原生回退：当未配置外部执行器时，会直接走 `novovm-network` 广播到已配置 peer（`UDP/TCP`，按链可配置），不再被外部执行器单点阻断。
- 若开启 `require_public_broadcast`（或链级 required）且“外部执行器未配 + 原生 peer 未配”，入口会直接硬失败，保持“提交成功必须可广播”的生产语义。
- 公网广播原生回退路径已做高并发收口：`UDP/TCP` transport 实例改为按链+模式缓存复用（不再每笔广播 `bind/register` 全量重建），并保留失败自动回收/重建，减少高频提交下的系统调用与连接抖动。
- 新增广播能力直查：`evm_getPublicBroadcastCapability`（输出 `ready/required/mode/executor_configured/native_peer_count`），并已并入 `evm_getRuntimeFullBundle.runtime_status.public_broadcast`，上游可单请求判定广播闭环可用性。
- 新增直连消费单入口：`evm_getRuntimeConsumerBundle`，一次聚合 `runtime + upstream + tx_status + event` 并输出 `ready/ready_details/counts`，上游可直接消费 `logs + receipt/error + broadcast`，无需多接口拼装。
- 本轮补 `evm_getRuntimeConsumerBundle` 直连消费收口：输出新增 `consumer.public_broadcast/consumer.tx_status/consumer.events(logs/filter_changes/filter_logs)`，并新增 `unresolved.{lifecycle/receipt/broadcast}_tx_hashes`；`ready` 改为严格判定（`tx_status` 三类语义全部可解 + 事件数组可读 + 公网广播就绪），避免“有数据但不可消费”假就绪。
- 本轮补 txpool 重复提交幂等语义：插件入池主线对“完全相同交易（字段级一致）”改为直接幂等接受，不再误报 `underpriced/nonce too low`；同时保持替换交易语义不变（同 nonce 低费替换仍拒绝，高费替换仍接受）。
- 本轮补 `tx status` 直连可消费语义：`evm_getTransactionLifecycle` 与 `evm_getRuntimeTxStatusBundle` 均新增 `stage/terminal/failed + receipt_pending/receipt_status + broadcast_mode` 扁平字段（不再要求上游跨 `lifecycle/receipt/broadcast` 二次归并）；`evm_getRuntimeConsumerBundle` 同步增加 `stage_resolved/stage_unresolved` 与 `unresolved.stage_tx_hashes`，`ready_details` 增加 `tx_status_stage_ready`，收口“回执/错误语义可直接消费”边界。
- 本轮补 `submit-status` 成功路径收口：`eth_sendRawTransaction/eth_sendTransaction` 成功后不再清理 submit-status，而是持久化 `accepted=true,pending=true,onchain=false`；`evm_getTxSubmitStatus/evm_getTransactionLifecycle` 在“无索引但有 submit-status”场景改为按状态推导 `stage/terminal/failed`（`pending/accepted/onchain/failed/rejected`），不再一律返回 `failed`。
- 本轮补 `submit-status` 的 `onchain_failed` 终态语义：索引命中且回执 `status=0x0` 时会持久化 `error_code=ONCHAIN_FAILED`，并在“无索引但有 submit-status”场景下稳定返回 `stage=onchain_failed, failed=true, terminal=true`，不再退化为 `onchain`。新增回归：`main_tests::evm_get_tx_submit_status_uses_persisted_onchain_failed_status_when_tx_missing`。
- 本轮补 `pending atomic-broadcast` 自动恢复主线：gateway 启动后会自动回放 pending atomic-broadcast（按 `NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_AUTOREPLAY_MAX` 上限），每次请求处理后也会执行同一自动回放；支持冷却/积压阈值/外部执行器开关（`NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_AUTOREPLAY_COOLDOWN_MS`、`NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_PENDING_WARN_THRESHOLD`、`NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_AUTOREPLAY_USE_EXTERNAL_EXECUTOR`），并保持成功清票据、失败保留重放票据的生产语义闭环。
- 本轮补 `pending atomic-ready` 自动恢复主线：gateway 自动回放 `compensate_pending_v1` 的 atomic-ready 记录（按 atomic-broadcast 自动回放扫描上限），恢复路径为 `pending atomic-ready -> atomic-ready spool + atomic-broadcast queue + pending ticket/payload`，随后由同一自动广播回放链路继续执行；重启后无需手工 `evm_replayAtomicReady` 批量补线。
- 本轮补 `pending public-broadcast` 自动恢复主线：gateway 已接入“启动即自动回放 + 每次请求后自动回放”生产路径，自动扫描最近链上 tx 索引中 `broadcast_status in {none, missing}` 的交易并重试公网广播；参数为 `NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_AUTOREPLAY_MAX`、`NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_AUTOREPLAY_COOLDOWN_MS`、`NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_PENDING_WARN_THRESHOLD`，复用现有 `maybe_execute_gateway_eth_public_broadcast` 主路径（外部执行器或 native 回退）。
- 本轮补 `pending public-broadcast` 显式队列主路径：新增 RocksDB pending ticket 前缀 `gateway:eth:public_broadcast:pending:v1:*`，写路径按广播结果自动维护（`mode=none` 入队、`mode=external/native` 清队）；自动回放优先消费 pending ticket，再对历史 `broadcast_status=none` 做一次兼容回填入队，避免每轮只靠全链扫描驱动重试。
- 本轮补 `eth_sendTransaction` 哈希推导收口：`GatewayEthTxHashInput/compute_gateway_eth_tx_hash` 已纳入 `maxPriorityFeePerGas`，`infer_gateway_eth_send_tx_hash_from_params` 与主写路径同源传入；不同 priority fee 不再命中同一推导哈希（已补回归锁定）。

### P0-E：FFI 与宿主边界加固

1. FFI 指针与长度校验补齐，禁止未约束 `from_raw_parts` 直接信任外部长度。
2. 插件 ABI 契约固定（版本化、向后兼容策略、错误码稳定）。
3. RocksDB 持久化路径统一命名空间并补 crash-recovery 一致性。

完成定义：

- 恶意或异常调用参数不会触发越界读取/未定义行为。
- 重启后 txpool/settlement/atomic 队列状态一致可恢复。

2026-03-13 本轮真实代码收口：

- `novovm-adapter-evm-plugin` 的 FFI 入参解码已切到受限 bincode 解码（`with_limit(MAX_PLUGIN_TX_IR_BYTES)`），防止恶意 payload 在解码阶段触发超大内存分配。
- 受限解码触发 `SizeLimit` 时返回稳定错误码 `NOVOVM_ADAPTER_PLUGIN_RC_PAYLOAD_TOO_LARGE(-8)`；其余解码异常保持 `NOVOVM_ADAPTER_PLUGIN_RC_DECODE_FAILED(-3)`，并补充回归锁定异常输入硬失败语义。
- 插件 bincode 导出侧（drain/snapshot 统一写出函数）已增加同一上限保护：当 payload 超过 `MAX_PLUGIN_TX_IR_BYTES` 时直接返回 `NOVOVM_ADAPTER_PLUGIN_RC_PAYLOAD_TOO_LARGE(-8)`，避免异常队列触发超大导出。
- `aoem-bindings` 的 AOEM owned buffer 拷贝已补硬边界：新增 `MAX_AOEM_OWNED_BUFFER_BYTES=64MiB` 上限，超限直接拒绝并释放 AOEM 缓冲；同时放行 `ptr=null && len=0` 空返回，避免空载荷场景误判。

### P1：链族与新交易类型扩展

1. 从当前 `EVM/BNB/Polygon/Avalanche` 扩展为可配置 EVM 链族配置。
2. 补齐 blob(type3) 等后续交易类型写路径（按 fork 配置可开关）。

完成定义：

- 新链接入不改主线代码，仅配置化接入。
- 新交易类型与费用/执行语义可运行并可回执。

2026-03-13 本轮真实代码收口：

- 链族映射已改为统一可配置入口：`novovm-adapter-evm-core::resolve_evm_chain_type_from_chain_id`，插件与网关不再各自维护硬编码映射；可通过 `NOVOVM_EVM_CHAIN_TYPE_OVERRIDES`（例：`56=bnb,137=polygon,43114=avalanche,8453=evm`）扩展链族，不改主线代码。
- blob(type3) 写路径已按配置开关接入：`NOVOVM_EVM_ENABLE_TYPE3_WRITE=1` 时允许 type3 路由与字段翻译（默认关闭，保持当前生产安全默认）；核心路径仍走同一 `opsw1 -> novovm-node -> AOEM` 二进制主线。
- blob(type3) 费用与校验语义已接入主线：新增 `max_fee_per_blob_gas/blob_hash_count` 解析、`estimate_blob_intrinsic_extra_gas_m0` 与 `estimate_intrinsic_gas_with_envelope_extras_m0`；`validate_tx_semantics_m0` 已按签名原文识别 envelope 并执行 type3 校验（`max_fee_per_blob_gas>0`、`blob_hash_count>0`），同时 type4 会按 profile policy 做拒绝。
- blob(type3) gateway 主写路径已同源收口：`eth_estimateGas`、`eth_sendTransaction` 与失败状态哈希推导 `infer_gateway_eth_send_tx_hash_from_params` 已统一解析 `maxFeePerBlobGas/blobVersionedHashes`，并统一复用 `resolve_gateway_eth_write_tx_type` 做 tx_type 推断与 type3 开关/必填校验；intrinsic 估算已切到 `estimate_intrinsic_gas_with_envelope_extras_m0`（含 blob gas）。新增回归：`main_tests::eth_estimate_gas_type3_includes_blob_intrinsic_cost_when_enabled`、`main_tests::eth_send_transaction_type3_accepts_blob_fields_when_enabled`。
- `eth_sendRawTransaction` 已接入同源语义前置校验：网关在写入 `.opsw1` 前会按 `chain_id -> profile` 执行 `validate_tx_semantics_m0`（含 intrinsic/type3/type4 规则），避免无效 raw 交易进入主线；新增回归：`main_tests::eth_send_raw_transaction_rejects_intrinsic_gas_too_low`。
- 本轮补 fork 级费用语义收口：`effectiveGasPrice` 对 type2/type3 统一改为 London 口径 `max(baseFee, min(maxFeePerGas, baseFee+priorityFee))`；同时 `eth_sendTransaction` 与 `eth_sendRawTransaction` 在 type2/type3 写入前新增 `maxFeePerGas >= baseFeePerGas` 硬校验，避免无效 EIP-1559 交易进入主线。新增回归：`main_tests::eth_send_transaction_rejects_type2_max_fee_below_base_fee`、`main_tests::eth_send_raw_transaction_rejects_type2_max_fee_below_base_fee`。
- 本轮补 `eth_estimateGas` 费用语义收口：对 type2/type3 新增与写入主线同源的 EIP-1559 校验（`maxPriorityFeePerGas <= maxFeePerGas`、`maxFeePerGas >= baseFeePerGas`），避免估算接口接受无效费用组合。新增回归：`main_tests::eth_estimate_gas_rejects_type2_priority_fee_above_max_fee`、`main_tests::eth_estimate_gas_rejects_type2_max_fee_below_base_fee`。
- 本轮补链级费率覆盖同源收口：`baseFee/defaultPriorityFee/gasPrice` 统一支持 `*_CHAIN_{id}` 与 `*_CHAIN_0x{id}` 覆盖；`eth_maxPriorityFeePerGas`、`eth_feeHistory(baseFee/reward)`、`eth_estimateGas`、`eth_sendTransaction/eth_sendRawTransaction`、`effectiveGasPrice/receipt`、`block.baseFeePerGas` 已统一走同一链级费率源。新增回归：`main_tests::gateway_eth_chain_fee_env_overrides_apply_to_helpers`、`main_tests::eth_fee_endpoints_use_chain_scoped_fee_overrides`。
- 本轮补 fork 写路径链级开关收口：`type1/type2/type3` 已统一支持 `NOVOVM_EVM_ENABLE_TYPE{1,2,3}_WRITE_CHAIN_{id}` 与 `NOVOVM_EVM_ENABLE_TYPE{1,2,3}_WRITE_CHAIN_0x{id}` 覆盖（保留全局默认），并在 `novovm-adapter-evm-core` + `evm-gateway` 同源生效；新增回归：`main_tests::resolve_gateway_eth_write_tx_type_respects_chain_scoped_type1_toggle`、`main_tests::resolve_gateway_eth_write_tx_type_respects_chain_scoped_type2_toggle`。
- 本轮补 fork 激活高度主写语义收口：`eth_estimateGas`、`eth_sendTransaction`、`eth_sendRawTransaction` 在写入主线前已统一执行 `pending block` 口径的 `London/Cancun` 激活校验（支持链级覆盖：`NOVOVM_GATEWAY_ETH_FORK_{LONDON|CANCUN}_BLOCK_CHAIN_{id|0xid}`）；未激活时直接拒绝 type2/type3，避免无效交易进入 `.opsw1`。新增回归：`main_tests::eth_estimate_gas_rejects_type2_when_london_not_active`、`main_tests::eth_send_transaction_rejects_type2_when_london_not_active`、`main_tests::eth_send_transaction_rejects_type3_when_cancun_not_active`。
- 本轮补链级 `0xHEX` 键大小写兼容收口：`gateway_eth_chain_u64_env/gateway_eth_chain_bool_env` 已同时支持 `..._CHAIN_0x{lower}` 与 `..._CHAIN_0x{UPPER}`（如 `0xA86A`），避免含 `A-F` 链号在大写环境键下失效；`type2` 链级开关、链级费率覆盖与 fork 激活高度均已同源生效。新增回归：`main_tests::resolve_gateway_eth_write_tx_type_respects_upper_hex_chain_scoped_type2_toggle`、`main_tests::gateway_eth_chain_fee_env_upper_hex_overrides_apply_to_helpers`、`main_tests::eth_send_transaction_rejects_type2_when_london_not_active_with_upper_hex_chain_key`。
- 本轮补 raw 主写路径同源收口：`eth_sendRawTransaction` 已接入 `resolve_gateway_eth_write_tx_type` 前置校验，raw 提交与 `eth_sendTransaction` 一样受 `type1/type2/type3` 链级写开关与 blob 必填字段约束；并补齐 raw fork 激活边界回归（type2/London、type3/Cancun）。新增回归：`main_tests::eth_send_raw_transaction_rejects_type2_when_write_path_disabled`、`main_tests::eth_send_raw_transaction_rejects_type2_when_london_not_active`、`main_tests::eth_send_raw_transaction_rejects_type3_when_cancun_not_active`。
- 本轮补 raw `tx_type` 参数一致性收口：`eth_sendRawTransaction` 若显式传 `tx_type/type`，现会与 raw envelope 推断类型强一致校验（不一致直接拒绝）；同时保持“显式且一致”可正常入主线，避免外部参数与原文交易语义漂移。新增回归：`main_tests::eth_send_raw_transaction_rejects_explicit_tx_type_mismatch`、`main_tests::eth_send_raw_transaction_accepts_matching_explicit_tx_type`。
- 本轮补写入口链参数一致性收口：`eth_estimateGas`、`eth_sendTransaction`、`eth_sendRawTransaction` 已统一接入 `chain_id/chainId/tx.chain_id/tx.chainId` 同值硬校验；当多位置参数不一致时直接拒绝（不再静默取首个值），避免错链提交。新增回归：`main_tests::eth_estimate_gas_rejects_chain_id_mismatch_between_top_level_and_tx`、`main_tests::eth_send_transaction_rejects_chain_id_mismatch_between_top_level_and_tx`。
- 本轮补失败状态哈希推导链参数一致性：`infer_gateway_eth_tx_hash_from_write_params` 与 `infer_gateway_eth_send_tx_hash_from_params` 已接入同一 `chain_id` 一致性规则；链参数不一致或 raw 推断链与显式链冲突时不再推导哈希，避免 `submit_status` 挂到错误交易。新增回归：`main_tests::infer_gateway_eth_tx_hash_from_write_params_returns_none_on_chain_id_mismatch`、`main_tests::infer_gateway_eth_send_tx_hash_from_params_returns_none_on_chain_id_mismatch`。
- 本轮补 type2/type3 费用必填语义：`eth_sendTransaction` 与 `eth_estimateGas` 在 `tx_type=2/3` 路径下已改为强制要求显式 `maxFeePerGas`（不再隐式回退 `gasPrice`），减少动态费交易参数歧义。新增回归：`main_tests::eth_send_transaction_rejects_type2_without_max_fee_per_gas`、`main_tests::eth_estimate_gas_rejects_type2_without_max_fee_per_gas`。
- 本轮补动态费单源费用口径：`eth_sendTransaction` 与 `infer_gateway_eth_send_tx_hash_from_params` 在 `tx_type=2/3` 下统一以 `maxFeePerGas` 作为 `gas_price` 参与主写索引与哈希推导，`maxPriorityFeePerGas` 同步走一致默认与校验口径，消除 `gasPrice` 与动态费字段并存时的语义漂移。新增回归：`main_tests::eth_send_transaction_type2_hash_and_index_use_max_fee_per_gas`、`main_tests::infer_gateway_eth_send_tx_hash_from_params_type2_uses_max_fee_for_gas_price`。
- 本轮补失败状态哈希推导费用边界同源：`infer_gateway_eth_send_tx_hash_from_params` 已补 `maxFeePerGas >= baseFeePerGas` 硬校验，避免在主写路径会拒绝的动态费参数上误生成失败哈希状态；并补齐 type1 上层 `0xHEX` 链级写开关回归。新增回归：`main_tests::infer_gateway_eth_send_tx_hash_from_params_returns_none_when_max_fee_below_base_fee`、`main_tests::resolve_gateway_eth_write_tx_type_respects_upper_hex_chain_scoped_type1_toggle`。
- 本轮补 Amsterdam/EIP-7954 语义收口：新增链级 fork 高度开关 `NOVOVM_GATEWAY_ETH_FORK_AMSTERDAM_BLOCK(_CHAIN_{id|0xid})`，并将 contract deploy initcode 上限切为同源函数（Amsterdam 前 `49_152`，Amsterdam 后 `65_536`）；`eth_estimateGas`、`eth_sendTransaction`、`eth_sendRawTransaction` 三条写路径已统一接入。`novovm-adapter-evm-core::validate_tx_semantics_m0` 的通用上限同步提升到 `65_536`，避免 Amsterdam 激活链误拒绝。新增回归：`main_tests::gateway_eth_contract_deploy_initcode_size_tracks_amsterdam_fork_activation`、`main_tests::eth_send_transaction_rejects_oversized_initcode_before_amsterdam`。

## 4. 代码边界铁律（执行版）

1. 不改 `novovm-node` 公共方法去适配单插件。
2. 插件能力必须在插件/网关边界内自洽，内部层间保持原生二进制通信。
3. 非必要日志默认关闭，生产路径保持极致静默与性能优先。
4. 不引入与主线无关的工程化包装层。

## 5. 收口顺序（激进版）

1. 先完成 `P0-A + P0-B`，解决“是否是真节点”问题。
2. 再完成 `P0-C + P0-D`，解决“能不能稳定跑生产交易闭环”问题。
3. 最后完成 `P0-E + P1`，解决“安全与扩展性”问题。

## 6. 最终 100% 判定

以下 6 条全部满足才标记 100%：

1. 可独立 p2p 同步追头，不依赖外部快照注入。
2. `eth_getProof` 可验证且与状态树一致。
3. `eth_call/estimateGas/receipt` 与原生语义对齐。
4. 提交->入池->广播->上链->回执闭环稳定。
5. FFI/持久化边界通过异常恢复与一致性验证。
6. 插件保持寄宿 superVM，不破坏内部二进制高性能主线。

## 8. 高性能流水线专项（功能口径外的性能收口）

说明：

- 第 7 节 `100%` 表示“生产功能闭环 100%”。
- 本节是“在不改变协议/共识/规则前提下”的性能专项改造，不与功能口径混算。

### HP-01：插件执行流水线并发化（不改执行顺序）

1. 交易批处理拆分为“并发预校验 + 串行状态提交”。
2. 保持账户 nonce、区块执行顺序、回执语义完全不变。

本轮落地：

- 插件已补“单批一次 hash 预处理并复用”热路径：`prepare_txs_with_hashes` 在大批量场景按分片并行预处理，小批量保持串行低开销。
- `runtime_tap`、`apply_v1`、`apply_v2` 已统一复用同一份预处理批次，避免 ingress/atomic/apply/settlement 对同批交易重复 `ensure_tx_hash`。
- `atomic intent id` 与结算入口已改为“优先复用现有 hash，缺失时再补算”，保持语义不变的前提下减少重复克隆与重复哈希。
- `apply_ir_batch` 已把“并发语义校验 + 并发验签”收口为单次并发扫描（分片内先 `validate_tx_semantics_m0` 再 `verify_transaction`），去掉独立语义扫描阶段，减少整批遍历与线程调度开销。
- `novovm-adapter-novovm` 已增加“单次验签缓存”：`verify_transaction` 成功后的 tx 在同批 `execute_transaction` 主线上不再重复完整验签，缓存未命中时回退原逻辑，执行语义保持一致。
- `novovm-adapter-novovm` 验签缓存容器已从 `Mutex<HashSet>` 切到 `DashSet` 并发集合，降低并发验签阶段全局锁竞争；`execute_transaction` 仍按 hash 命中移除缓存，语义不变。
- `novovm-adapter-evm-plugin` 的 `apply_ir_batch` 已去掉 `Box<dyn ChainAdapter>` 热路径动态分发，改为直接使用 `NovoVmAdapter` 具体实现，减少批内 `verify/execute/state_root` 虚调用开销，协议语义不变。
- `aoem-bindings` 已补 `aoem_ed25519_verify_v1/aoem_ed25519_verify_batch_v1` 直连入口；`novovm-adapter-novovm` 的单笔/批量验签已优先走 AOEM，新 AOEM FFI 可用时不再停留在纯 CPU 本地 dalek 路径。
- `aoem-bindings` 已补 `aoem_secp256k1_verify_v1/aoem_secp256k1_recover_pubkey_v1` 直连入口；`novovm-adapter-evm-core` 已把 raw sender recovery 下沉到核心层，gateway `eth_sendRawTransaction` 现优先使用 AOEM 从 raw 原文恢复 sender，显式 `from` 改为一致性约束或兼容回退。
- `eth_sendTransaction` 已补 recoverable-signature sender 校验：请求若携带 `signature/raw_signature/signed_tx` 且可被解析为 recoverable raw EVM tx，则 gateway 现按同一 AOEM `secp256k1 recover` 主线恢复 sender 并与显式 `from` 做强一致校验；不可恢复时保持原宿主路径。
- `eth_sendTransaction` 的同一 recoverable-signature 路径已补字段强一致：若 `signed_tx` 可恢复，则 `nonce/to/value/gas/fee/data/accessList/blob` 等有效字段必须与显式请求体一致；否则直接拒绝或在失败状态哈希推导处返回 `None`。
- `apply_ir_batch` 已收口为“并发语义预校验 + 单批 AOEM ed25519 batch verify + 串行状态提交”；隐私交易与 `from` 匹配边界保持原语义，不改变外部 EVM 行为。
- `novovm-adapter-novovm` 已增加“状态根懒刷新缓存”：执行/写入路径仅标记 dirty，不再每笔交易全量重算状态根；`state_root()` 查询时再统一刷新缓存，批内吞吐提升且不改对外语义。

完成定义：

- 同一输入批次，状态根与回执结果与改造前一致。
- CPU 利用率提升，单批处理延迟下降。

### HP-02：插件运行态锁分片

1. 将全局运行态锁按 `chain_id/sender` 分片，降低锁竞争。
2. 不改变 txpool 语义（替换/nonce-gap/容量约束）。

本轮落地：

- 已将插件 `txpool` 热路径从结算/回执运行态中拆分为独立锁，降低 ingress 与结算路径的锁竞争。
- 已将 `txpool` 再按 `chain_id` 分片为多锁（`EVM_TXPOOL_SHARDS`），不同链写入路径可并行，不再竞争同一把 txpool 锁。
- 已补 `txpool` 分片数可配置（`NOVOVM_ADAPTER_PLUGIN_EVM_TXPOOL_SHARDS`）与 host 读口公平轮转（按游标起点遍历分片），减少固定分片热点与读取偏置。
- ingress 主入口已拆分为 `push_ingress_frames_prepared`，直接消费预处理批次；`runtime_tap/apply_v1/apply_v2` 共享同一预处理结果后再进 txpool，进一步缩短 txpool 锁内重复计算路径。
- 结算/回执运行态已按链分片（`NOVOVM_ADAPTER_PLUGIN_EVM_RUNTIME_SHARDS`）：`settlement/atomic-receipt/atomic-ready` 写路径改为按 `chain_id` 命中分片锁，去除单全局运行态锁热点。
- host 侧 `drain_atomic_receipts/drain_settlement_records/drain_payout_instructions/drain_atomic_broadcast_ready` 已改为分片轮转聚合；`settlement totals/snapshot` 改为跨分片汇总，`settlement_seq` 保持全局单调。

完成定义：

- 高并发写入时无全局大锁瓶颈。
- 行为语义与当前生产一致。

### HP-03：gateway 请求并发化（仅边界层）

1. 保持外部 JSON-RPC 语义不变。
2. 入口请求改并发 worker 处理，状态缓存改分段并发结构。

本轮落地：

- `evm_getPublicBroadcastStatusBatch` 已改为“批量并发查询 + 顺序聚合返回”路径，新增批量 worker 并发控制（`NOVOVM_GATEWAY_BATCH_WORKERS`，`0=自动`）。
- `evm_getTransactionReceiptBatch`、`evm_getTransactionByHashBatch` 已改为“批量并发查询 + 顺序聚合返回”路径，保持默认链提示、pending 语义与单笔查询一致。
- `evm_replayPublicBroadcastBatch` 已改为“批量并发回放 + 顺序聚合返回”路径，保持逐笔错误项与成功项结构不变。
- `evm_getTransactionLifecycleBatch` 已改为“批量并发查询 + 顺序聚合返回”路径，并与单笔 `evm_getTransactionLifecycle` 统一复用同一生产 helper，避免语义漂移。
- `evm_getLogsBatch`、`evm_getFilterLogsBatch` 已改为“批量并发查询 + 顺序聚合返回”路径；保留原有日志过滤语义与返回顺序。
- `evm_getRuntimeTxStatusBundle` 已改为“按 tx 并发收口 lifecycle+receipt+broadcast 一次生成”，减少重复批量扫描与重复解析开销。
- `evm_getFilterChangesBatch` 已改为直连生产 `filter` 处理 helper，去掉批量递归分发开销。
- `evm_getRuntimeEventBundle` 的 `logs/filterChanges/filterLogs` 已改为直连生产 helper，减少重复路由分发与参数重解析开销。
- `evm_publicSendRawTransactionBatch`、`evm_publicSendTransactionBatch` 已去掉批量内递归分发层，直接进入 `eth_send*` 主生产路径（保留 public-broadcast 语义标记）。
- `evm_getRuntimeTxStatusBundle`、`evm_getRuntimeEventBundle`、`evm_getRuntimeFullBundle` 已完成 helper 直连收口，去掉 bundle 内部递归 `run_gateway_method` 分发层，减少二次参数解析与重复路由开销。
- `evm_getRuntimeConsumerBundle` 已改为直连 `FullBundle helper`，去掉 consumer->fullBundle 的递归分发层。
- 批量查询中的持久层命中缓存回写已收口为“线程外顺序回写”，避免并发写入主索引锁竞争。
- native sync-pull peer 选择已收口为“单次 runtime 头快照 + 单次有序候选发送”，去掉同一轮中的双次 `select` 与 merge 组装，减少重复排序与集合构建开销。
- `public broadcast` 交易热路径已去掉“每笔交易附带周期性 discovery 发送”逻辑，统一交给后台 native runtime worker 维持 peer 活性，减少每笔交易额外报文与拷贝开销。
- 对外返回结构与顺序保持兼容，不改已有方法语义。

完成定义：

- 对外接口响应语义不变。
- 高并发请求下吞吐提升且无状态漂移。

### HP-04：network 拉取与状态计算并发优化

1. 去除热路径阻塞重试/sleep。
2. runtime status 计算改为分片或低竞争更新。

本轮落地：

- `TcpTransport::send` 已去除固定阻塞 sleep 重试，改为可配置重试策略（`NOVOVM_NETWORK_TCP_CONNECT_RETRY_ATTEMPTS`、`NOVOVM_NETWORK_TCP_CONNECT_RETRY_BACKOFF_MS`），默认低延迟快返。
- `runtime_status` 在 `set_network_runtime_*` 热更新路径中增加“状态未变化不触发 reconcile”短路，减少高频重复锁竞争与重复重算。
- `runtime_status` 已把“按 chain_id 再读取 runtime 后 reconcile”的二次锁路径收口为“携带已知 runtime 状态直接 reconcile native 状态”，`set_network_runtime_sync_status / set_network_runtime_peer_count / set_network_runtime_block_progress` 及 peer/local/snapshot 观测路径统一复用，减少热路径重复锁与重复 map 读取。
- `transport` 的 `StateSync/ShardState` 收包更新已改为“单次进锁同时更新 peer_head + local_head(max)”路径，替换原先两次独立 runtime 更新调用，降低高频收包时锁竞争。
- `transport` 的收包 runtime 更新已去掉“先 register 再 observe”的双调用模式：`Pacemaker/Finality/可解码 DistributedOccc` 直接单调用 `observe_*`，未知消息才走兜底 `register`，进一步减少每条热消息的重复锁与重复重算。
- `runtime_status` 的 `register_peer / observe_peer_head` 已增加“无状态变化快返”路径：当 peer/head/local 未变化且无 native hint 清理时直接返回，不再触发整轮 recompute + reconcile，进一步压低高频消息空转成本。
- `transport` 的 runtime sync pull inflight 目标表已从 `Mutex<HashMap>` 改为 `DashMap`，并把 followup “未到窗口上界则等待、到上界则清理”收口为单 helper，减少高频收包下的全局锁竞争与重复 map 访问。
- `transport` 的 followup 出站路径已去掉“接收侧预跟踪 + send 成功后再跟踪”双写，改为“send 成功走发送主路径跟踪；仅 fallback 原始 socket/tcp 直发时补一次跟踪”，减少重复解码与重复写目标表。
- `transport` 的 UDP/TCP 回包/续拉发送路径已改为 `send_internal(&ProtocolMessage)` 直发，收包回路不再为 fallback 保留而 clone 消息对象，减少热路径分配与拷贝。
- `transport` 的 sync-pull 回包构建已从“先组 `Vec<Vec<u8>>` 中间 payload 再二次组消息”收口为“按区间直接流式组消息”，减少中间容器分配与一次遍历。
- `runtime_status` 已增加“stale 到期时间短路”机制：未到期链路直接跳过 stale 全量扫描，降低高频读写下 `peer_last_seen/native_snapshot` 的重复遍历成本；语义保持一致（到期后同样清理 stale peer/snapshot）。
- `runtime_status` 已新增 `peer heads Top-K` 查询路径；gateway sync-pull 选路改为按配置 peer 数量按需取 Top-K，不再默认全量排序全部 runtime peers，降低调度热路径排序开销。
- `runtime_status` 读路径已改为“dirty/stale 到期才重算”：`get_network_runtime_sync_status/get_network_runtime_peer_heads/get_network_runtime_peer_heads_top_k` 在无脏变化且未到期时直接返回缓存状态，减少高频轮询下重复重算。
- `transport` 收包热路径已改为缓冲区复用：`UdpTransport::try_recv` 与 `TcpTransport::try_recv` 不再每帧分配新 `Vec`，改为复用内部接收缓冲，降低高频收包分配与拷贝开销。
- `transport` UDP 发送热路径已去掉“每次发包都执行 runtime peer register”的重复开销，改为“首包注册、断连清理后再注册”，减少高频发送时 runtime 状态锁竞争。
- `runtime_status` 的 `observe_network_runtime_local_head` / `observe_network_runtime_local_head_max` / `ingest_network_runtime_native_sync_snapshot` 已加入“无变化快返”短路，减少 send/worker tick 高频上报下的重复重算。
- `gateway` sync-pull 调度已改为“按本轮 fanout 取小规模 Top-K 优先发送 + 配置顺序失败回退”，不再每轮全量 peers 排序/发送扫描。
- `gateway` native sync worker 已改为“先 drain 收包、再规划并发送下一窗口”顺序，窗口规划直接消费最新 runtime 观测，减少滞后窗口重复拉取。
- `gateway` sync-pull 发送已改为“单次构建全量有序 peer 快照 + 按 fanout 截断发送”，去掉发送函数内部重复选路与重复去重集合构建。
- `gateway` sync-pull tracker 已改为“已有窗口状态原地更新”，避免每个 tick 重建 `worker_key` 字符串与整条 state。
- `transport` 的 pull target 窗口检查已收口为单次读取后判定/清理，减少同键双查开销。
- `transport` 的 `src -> peer` 推断已改为单次遍历（精确地址优先 + 同 IP 唯一回退），减少收包路径重复遍历。
- `transport` 的 `try_recv` 已改为“消息自带来源 id 时不做 `src -> peer` 反查”，减少每包 DashMap 扫描。
- `transport` runtime 状态更新已改为“仅 `StateSync/ShardState` 尝试 header 解码”，去掉其他 `DistributedOccc` 报文的无效解码开销。
- `gateway` native sync worker 已改为“同步期短 tick（250ms）+ 空闲期常规 tick（1000ms）”，提升窗口推进速度。
- `gateway` sync-pull 重发窗口已改为 phase 自适应（headers/bodies/state/finalize 分档），减少重发等待空窗。
- `gateway` sync-pull tracker 状态已改为 `phase_tag(u8)`，去掉热路径 phase 字符串拷贝与比较开销（仅保留协议 tag）。
- `gateway` native broadcaster 已内置 peer 注册缓存（同 peer 同地址跳过重复 `register_peer`），把每笔广播重复注册收口为首包注册，降低发送热路径锁竞争与系统调用开销。
- `gateway` native peers 已改为按链缓存快照（raw 配置变更自动刷新），`capability/runtime-worker/public-broadcast` 共享同一份解析结果，去掉每次调用重复 split+parse+peer_nodes 构建。
- `runtime_status` stale 清理已改为 `retain` 原地回收（去掉 stale id 中间向量），并为 `Top-K(k=1)` 增加 O(n) 快路径，降低 sync-pull 单播阶段排序/分配开销。
- `transport` 的 `StateSync/ShardState` ingress 已改为“单次解析复用”：同一报文的 request/header 解析结果在 response 构建、runtime 更新、followup 构建三段共用，去掉重复解码与重复分支判断。
- `transport` 的 sync-pull 回包主路径已改为“窗口计划 + 流式发送”：收包后不再先构建整批 `Vec<ProtocolMessage>`，而是按响应区间逐条构建并发送（保留失败 fallback），减少中间容器分配与二次遍历。
- `transport` 新增 `src_addr -> peer` 直达索引 + `src_ip` 唯一性索引并接入 UDP/TCP 收包路径，优先 O(1) 地址/同 IP 命中；仅索引未命中时才回退遍历推断，降低高并发收包时的 DashMap 全表扫描成本。
- `transport` 的 sync-pull followup 已增加 phase 分档预取触发：窗口尾部即可提前发起下一窗请求，减少“窗口完全收满后才发送”带来的 RTT 空档。
- `transport` 的 UDP/TCP 收包主线已改为短锁路径：共享缓冲只在取/还时加锁，`recv/decode/read` 全部锁外执行，减少高并发收包锁竞争。
- `gateway` native sync-pull worker 已增加“同窗分段并发拉取”：按 fanout 把 `from/to` 窗口切成连续子区间并并发分发到多 peer，减少同窗重复请求和单窗串行等待。
- `gateway` native sync-pull worker 已增加链级并发调优参数：`FANOUT_MAX/SEGMENTS_MAX/SEGMENT_MIN_BLOCKS`（支持 `_CHAIN_{id|0xid}`），用于压测场景下控制并发窗口与窗口粒度，默认不改变现有行为。
- 本轮补 phase 级并发调优键：`FANOUT_MAX/SEGMENTS_MAX/SEGMENT_MIN_BLOCKS` 均支持 `_HEADERS/_BODIES/_STATE/_FINALIZE` 后缀（可叠加 `_CHAIN_{id|0xid}`），并兼容 `_CHAIN_0x{id}` 大小写环境键。
- 本轮补 sync-pull 重发间隔调优键：`NOVOVM_GATEWAY_ETH_NATIVE_SYNC_PULL_RESEND_MS` 支持链级 + phase 后缀覆盖（同样可叠加 `_CHAIN_{id|0xid}` 与 `_HEADERS/_BODIES/_STATE/_FINALIZE`），并做 `50..60000ms` 硬钳制，便于压测定标。
- 本轮补 sync-pull 选路热路径降耗：runtime `peer-head Top-K` 查询预算改为“本轮 fanout 预算”，不再按全量 peer 取样。
- `transport` 发送路径已改为“peer 地址即时拷贝后发送/重试”，避免在 TCP 连接重试/UDP 发送阶段持有 DashMap guard；`gateway` sync-pull 发送增加 `fanout=1` 快路径，减少小 fanout 场景集合开销。

完成定义：

- 同步状态语义不变（`eth_syncing/net_peerCount/blockNumber` 同源）。
- 拉取链路在抖动网络下恢复更快。

### 高性能专项进度（2026-03-14）

- HP-01 当前：`100%`（已完成“并发语义预校验 + 单批 hash 预处理复用 + apply/runtime_tap 主线去重复哈希 + verify/execute 去双验签 + AOEM ed25519 single/batch verify 热路径直连 + AOEM secp256k1 raw sender recovery / recoverable-signature sender 校验直连 + apply 主线 AOEM batch verify + 串行 execute + 错误路径 shutdown 收口”）
- HP-02 当前：`100%`（已完成“txpool 热路径独立锁 + chain_id 分片锁 + `chain_id+sender` ingress 分片写入 + 可调分片/公平轮转 + prepared ingress 主入口复用 + runtime 结算/回执按链分片 + host drain 分片轮转聚合 + pending 跨分片全局 sender round-robin 语义收口 + pending sender bucket 按分片定向取数”）
- HP-03 当前：`100%`（已完成“public-broadcast status + receiptBatch + txByHashBatch + replayPublicBroadcastBatch + lifecycleBatch + logs/filterLogs + filterChangesBatch + upstreamTxStatusBundle/eventBundle/fullBundle/consumerBundle helper 直连去层 + publicSendBatch 递归去层收口 + native sync-pull peer 单次快照选路 + fanout 小快照优先 + 失败回退 + tracker 原地更新 + phase-tag 去字符串化 + phase 自适应重发 + 同步期短 tick + public-broadcast 热路径去 discovery 附带发送 + broadcaster peer 注册缓存去重复注册 + peers 配置按链缓存快照复用 + fanout=1 快路径 + 收包前置窗口规划 + 单次有序 peer 快照发送”）
- HP-04 当前：`100%`（已完成“network 非阻塞重试 + runtime status 低争用短路 + 观测/更新路径直连 reconcile 去重复锁 + StateSync/ShardState 单次进锁更新 + 收包去双调用 + 无变化快返 + sync-pull 目标表 DashMap 化 + followup 跟踪去双写 + UDP/TCP 回包续拉去 clone + sync-pull 回包流式组装 + stale 到期短路 + stale 清理 retain 原地回收 + peer heads Top-K 选路 + Top-K(k=1) 快路径 + dirty/stale 到期触发重算短路 + UDP/TCP 收包缓冲区复用 + UDP 发送首包注册化 + local-head/native-snapshot 无变化快返 + pull-target 单次读取 + src->peer 单次遍历 + try_recv 懒反查 + 非 sync 报文跳过 header 解码 + 消息来源 id 单次解析复用 + sync ingress 单次解析复用 + sync 回包窗口计划流式发送去中间批量容器 + src_addr/src_ip O(1) 索引化命中 + sync window phase 预取触发 + UDP/TCP 收包短锁（锁外解码/读包）”）
- 高性能流水线专项总进度：`100%`

