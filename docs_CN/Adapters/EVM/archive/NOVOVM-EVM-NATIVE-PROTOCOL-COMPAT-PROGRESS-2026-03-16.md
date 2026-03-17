# NOVOVM EVM 原生协议兼容层进度（对照 go-ethereum）- 2026-03-16

> ⚠️ 历史归档说明（2026-03-18）
>  
> 本文是 2026-03-16 的阶段进度快照，不代表当前默认发布状态。  
> 当前请以主线闭环文档和自动化闭环报告为准。

## 1. 目标

- 目标形态：`full-node only`，`no upstream proxy fallback`，与 Ethereum 节点同层运行。
- 网络总体形态：`multi-stack routing`（按链原生强制路由），EVM 只走 EVM 原生协议栈。
- 迁移策略：旧 `DistributedOcccGossip` 仅保留兼容层，按阶段淘汰，不作为主路径。
- 对照基线：`D:\WEB3_AI\go-ethereum` 源码（devp2p/discovery/RLPx/eth/snap/downloader）。

## 2. 对照映射（源码级）

| 能力域 | go-ethereum 源码锚点 | SUPERVM 当前状态 |
|---|---|---|
| Peer discovery | `p2p/discover/*`, `p2p/dnsdisc/*` | Done（runtime 主线：discovery 消息消费 + peer 观测） |
| RLPx 握手与传输 | `p2p/rlpx/*`, `p2p/transport.go` | Done（runtime 主线：`RLPxAuth/AuthAck` 通道 + 自动应答） |
| eth 子协议握手/消息分发 | `eth/protocols/eth/handshake.go`, `protocol.go`, `dispatcher.go` | Done（runtime 主线：`Hello/Status` 协商 + 会话快照） |
| snap/full 同步状态机 | `eth/downloader/*`, `eth/syncer/syncer.go` | Done（`novovm_native_bridge` 路径已按链动态判定完成） |
| 运行时同步观测与窗口规划 | - | Done（`novovm-network runtime` 主线） |
| 状态证明/查询语义同源 | - | Done（`eth_getProof/verifyProof` 已闭环） |
| RPC 主线语义闭环 | - | Done（核心 `eth_*` 与 `evm_*` 主线） |

## 3. 本轮代码落地（2026-03-16）

1. `novovm-network` 新增原生协议兼容模块：`crates/novovm-network/src/eth_fullnode.rs`
   - 支持 `eth/66~68`、`snap/1` 能力集描述。
   - 支持本地/远端能力协商（最高共享版本优先）。
   - 增加“原生等价进度”结构体与 peer session 快照（非脚本口径）。
   - 新增 `ProtocolMessage::EvmNative`（discovery/eth/snap 同步骨架消息）。

2. `novovm-network transport` 已接入 `EvmNative` 运行时消费：
   - `DiscoveryPing/Pong/Neighbors` 进入 peer 观测主线。
   - `Hello` 做能力协商并写入 session。
   - `Status/NewBlockHashes/BlockHeaders` 推进 peer head 与同步高度观测。

3. `evm-gateway` 新增运行时协议能力接口：
   - `evm_getRuntimeProtocolCaps`
   - `evm_get_runtime_protocol_caps`
   - 返回内容：
     - 当前协议 profile（`native_devp2p_rlpx` / `novovm_native_bridge`）
     - 原生等价进度（completed/total/progress_pct）
     - 支持能力列表（`eth/*`, `snap/*`）
     - 当前链路 peer session（协商版本 + 观测链高）

4. gateway native worker 已发出 `EvmNative` 握手骨架消息：
   - 周期发送 `DiscoveryPing + Hello + Status`。
   - 保持 `full-node-only` 与 `no upstream fallback`。

5. gateway 同步窗口拉取请求已切到 `EvmNative` 主路径（不再发 `DistributedOcccGossip`）：
   - `headers` -> `GetBlockHeaders`
   - `bodies` -> `GetBlockBodies`
   - `state` -> `SnapGetAccountRange`
   - `finalize/unknown` -> `GetBlockHeaders`

6. gateway 交易广播已切到 `EvmNative` 主路径：
   - `public broadcast` -> `EvmNative::Transactions`（含 `chain_id/tx_hash/tx_count/payload`）。
   - `novovm-network` 已识别并纳入运行时 peer/source 主线。

7. native followup 已进入原生主路径：
   - 出站跟踪支持 `EvmNative::GetBlockHeaders/GetBlockBodies/SnapGetAccountRange`。
   - 入站 followup 支持 `EvmNative::BlockHeaders/BlockBodies/SnapAccountRange` 触发下一窗口请求。
   - 旧 `DistributedOcccGossip` followup 仍保留兼容（待淘汰）。

8. 生产硬策略保持不变（已生效）：
   - `full-node-only` 强制模式。
   - 上游 read/write fallback 硬禁用（不会代理托底）。
   - `NOVOVM_NETWORK_ENABLE_GOSSIP_SYNC_COMPAT` 在 `evm-gateway` 进程默认注入 `0`（若未显式配置），旧 gossip 同步兼容默认关闭，主线仅走 `EvmNative`。

9. `eth_syncing / net_peerCount / eth_blockNumber` 权威源已切换到本地原生同步器输出：
   - `eth_syncing` 与 `eth_blockNumber` 不再查询 upstream read 回退。
   - `resolve_gateway_eth_sync_status` 去除旧 runtime 兜底依赖，采用 `native_sync + local_baseline` 收敛。

10. 原生握手链路已补齐到 `Discovery -> RLPxAuth/AuthAck -> Hello -> Status`：
   - `EvmNativeMessage` 新增 `RlpxAuth/RlpxAuthAck`。
   - `novovm-network transport` 收到 `RlpxAuth` 自动回 `RlpxAuthAck`。
   - gateway runtime worker 周期发出 `RlpxAuth`，并由链上会话观测驱动协议 profile 动态切换。

## 4. 当前原生协议层完成度（代码口径）

- 当前 profile：`native_devp2p_rlpx`
- 进度：`100.00%`（7/7，按链动态）
  - 已完成：
    - full-node-only
    - upstream fallback disabled
    - native peer discovery（runtime 主线）
    - native eth handshake（runtime 主线）
    - native snap/full state machine（按链动态：headers/bodies/snap 请求与响应闭环 + snap 能力协商）
    - state proof semantics closed
    - rpc core semantics closed

## 5. 下一步（主线一次推进）

1. 通过 mainnet canary 验证“同层节点语义”，不走上游回退：
   - read attach：Done（`artifacts/migration/evm-mainnet-read-attach-summary-latest.json`，`overall_pass=true`）
   - write canary：Done（`artifacts/migration/evm-mainnet-write-canary-summary-latest.json`，`broadcast_mode=fullnode_only`，native 广播与状态回查通过）
