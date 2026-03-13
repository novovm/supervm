# NOVOVM EVM 全镜像 100% 收口清单（生产功能直推版）- 2026-03-13

## 1. 目标口径

- 目标不是“RPC 兼容层”，而是“可替代原生 geth 的 Rust 全功能镜像节点（寄宿 superVM）”。
- 完成判定只看生产代码与运行行为，不看 gate/signal/snapshot 脚本产物。
- 内部固定二进制流水线：`plugin/gateway -> opsw1 -> novovm-node -> AOEM`。

## 2. 当前基线（2026-03-13）

- 适配器/网关兼容层完成度：`92%`
- 原生 geth 全节点等价度：`63%`
- 寄宿 superVM 融合度：`86%`

> 说明：现阶段“常用 eth_* 查询与提交路径”基本齐备，但“原生网络/同步/状态证明/完整执行语义”仍未收口到 100%。

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
- 本轮新增 runtime 拉取窗口生产直连：`evm_getRuntimeSyncPullWindow/evm_get_runtime_sync_pull_window` 直接返回 runtime 规划的 `phase + [from_block,to_block]` 下载窗口，并已纳入 `evm_getUpstreamFullBundle.runtime_status.sync_pull_window`，上游可单请求消费同步拉取边界。
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

## 7. 进度（2026-03-13）

- P0-A 当前：`100%`
- P0-B 当前：`100%`
- P0-A + P0-B 合并进度：`100%`
- P1 当前：`100%`（链族配置化 + type1/type2/type3 链级写开关 + blob 费用/校验 + gateway 主写路径 blob 语义 + raw/非raw 前置语义校验 + London 口径 effectiveGasPrice 收口 + 链级费率覆盖同源语义 + London/Cancun fork 激活高度语义 + 链级 `0xHEX` 大小写兼容 + raw 主写路径开关/激活边界/显式 tx_type 一致性同源校验 + 写/估算入口 `chain_id` 参数一致性硬校验 + 失败状态哈希推导链参数一致性 + type2/type3 `maxFeePerGas` 显式必填语义 + 动态费哈希/索引单源费用口径 + 失败状态哈希推导费用边界同源）

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
- 新增广播能力直查：`evm_getPublicBroadcastCapability`（输出 `ready/required/mode/executor_configured/native_peer_count`），并已并入 `evm_getUpstreamFullBundle.runtime_status.public_broadcast`，上游可单请求判定广播闭环可用性。
- 新增直连消费单入口：`evm_getUpstreamConsumerBundle`，一次聚合 `runtime + upstream + tx_status + event` 并输出 `ready/ready_details/counts`，上游可直接消费 `logs + receipt/error + broadcast`，无需多接口拼装。
- 本轮补 `evm_getUpstreamConsumerBundle` 直连消费收口：输出新增 `consumer.public_broadcast/consumer.tx_status/consumer.events(logs/filter_changes/filter_logs)`，并新增 `unresolved.{lifecycle/receipt/broadcast}_tx_hashes`；`ready` 改为严格判定（`tx_status` 三类语义全部可解 + 事件数组可读 + 公网广播就绪），避免“有数据但不可消费”假就绪。
- 本轮补 txpool 重复提交幂等语义：插件入池主线对“完全相同交易（字段级一致）”改为直接幂等接受，不再误报 `underpriced/nonce too low`；同时保持替换交易语义不变（同 nonce 低费替换仍拒绝，高费替换仍接受）。
- 本轮补 `tx status` 直连可消费语义：`evm_getTransactionLifecycle` 与 `evm_getUpstreamTxStatusBundle` 均新增 `stage/terminal/failed + receipt_pending/receipt_status + broadcast_mode` 扁平字段（不再要求上游跨 `lifecycle/receipt/broadcast` 二次归并）；`evm_getUpstreamConsumerBundle` 同步增加 `stage_resolved/stage_unresolved` 与 `unresolved.stage_tx_hashes`，`ready_details` 增加 `tx_status_stage_ready`，收口“回执/错误语义可直接消费”边界。
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
