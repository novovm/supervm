# NOVOVM EVM 插件配置/设置/使用手册（Configuration, Setup, Usage）- 2026-03-16

## 1. 文档目的（Purpose）

本手册用于团队并行操作时的统一口径，回答三件事：

1. EVM 插件怎么配置（Configuration）。
2. 节点怎么启动（Setup）。
3. 查询与发交易怎么用（Usage）。

同时明确两条边界：

- 默认不会自动占满主网流量或无限吃盘。
- `fullnode_only` 与 `upstream_proxy` 是不同模式，生产主线推荐 `fullnode_only`。

---

## 2. 运行模式（Run Modes）

### 2.1 被动模式（Passive, 默认轻量）

- 不配置 native peers，不配置 upstream RPC。
- 可用于本地开发/接口联调。
- 不会自动形成持续主网同步流量。

### 2.2 全节点仅本机模式（Full-node Only, 推荐生产）

- 只走本机原生栈（native transport）。
- 禁止上游代理回退（no upstream proxy fallback）。
- 对外仍是标准 EVM JSON-RPC 语义。

### 2.3 上游代理模式（Upstream Proxy, 迁移/临时）

- 通过上游 RPC 获取状态或做广播。
- 只用于迁移和临时验证，不是最终主线形态。

### 2.4 端口与网络边界（Port & Network Boundary）

- 以太坊（geth）外部 P2P 默认端口是 `30303`（TCP），发现协议默认也使用 `30303`（UDP，可单独改）。
- SUPERVM 主网采用另一套隐私网络形态，不与以太坊公网 P2P 端口语义混用。
- 实践上建议把两类端口分层：
  - SUPERVM 网络端口：例如 `39001`、`39002`（示例）。
  - EVM 插件/以太坊兼容端口：例如 `30303`、`30304`。
- 通过路由策略做自适应分类，避免把 `enode://...` 与 `nodeId@host:port` 混在同一语义里。

---

## 3. 关键环境变量（Key Environment Variables）

## 3.1 基础变量（Base）

| 变量 | 说明 |
|---|---|
| `NOVOVM_GATEWAY_BIND` | Gateway 监听地址，默认 `127.0.0.1:9899` |
| `NOVOVM_GATEWAY_ETH_DEFAULT_CHAIN_ID` | 默认链 ID，默认 `1` |

## 3.2 Full-node Only（Native）相关

| 变量 | 说明 |
|---|---|
| `NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_NATIVE_TRANSPORT` | `udp` 或 `tcp` |
| `NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_NATIVE_NODE_ID` | 本机 native 节点 ID（十进制或 `0x`） |
| `NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_NATIVE_LISTEN` | 本机 native 监听地址，例如 `127.0.0.1:39001` |
| `NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_NATIVE_PEERS` | 对端列表，支持混合输入：`nodeId@host:port`、`nodeId=host:port`、`enode://...@host:port` |
| `NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_ROUTE_POLICY` | 路由策略：`auto`（默认）/`supvm_only`/`plugin_only` |
| `NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_PLUGIN_PORTS` | 插件协议端口白名单（默认 `30303,30304`），`auto` 模式按端口分类路由 |
| `NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_PLUGIN_SESSION_PROBE_MODE` | 插件会话探测模式：`enode`（默认，仅探测 `enode://`）/`all`/`disabled` |
| `NOVOVM_NETWORK_ENABLE_GOSSIP_SYNC_COMPAT` | 旧兼容开关；EVM 进程默认注入 `0`（关闭旧 gossip 同步） |

说明（当前实现状态）：

- `auto` 模式会把 `30303/30304`（可配置）以及 `enode://...` 归类为插件路由候选。
- `nodeId@host:port` 且端口不在插件端口表时，走 SUPERVM 路由。
- 插件路由会维护会话阶段指标：`disconnected/tcp_connected/auth_sent/ack_seen/ready`，用于区分“仅 TCP 可达”和“有协议回包”。
- 目前插件路由仍处于接入阶段，能力输出会标记 `plugin_route_pending`，用于避免混淆与误连。

## 3.3 Upstream（代理）相关

| 变量 | 说明 |
|---|---|
| `NOVOVM_GATEWAY_ETH_UPSTREAM_RPC` | 读路径上游 RPC |
| `NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_UPSTREAM_RPC` | 广播上游 RPC |
| `NOVOVM_GATEWAY_ETH_UPSTREAM_RPC_TIMEOUT_MS` | 上游调用超时 |
| `NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_EXEC` | 外部广播执行器路径（可选） |
| `NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_REQUIRED` | 是否强制广播成功 |

---

## 4. 存储策略（Storage Strategy）与原生 EVM 对比

## 4.1 原生 geth（参考）

- Full node 会持续同步区块/状态，磁盘持续增长。
- Archive 增长更快。

## 4.2 NOVOVM EVM 插件（当前）

- **统一账户存储（Unified Account Store）**：默认 `rocksdb` 持久化。
- **EVM 交易索引（ETH TX Index）**：默认 `memory`（重启丢失，不持续吃盘）。
- 可显式改为 `rocksdb` 获得重启可恢复能力。

关键变量：

| 变量 | 默认 | 说明 |
|---|---|---|
| `NOVOVM_GATEWAY_UA_STORE_BACKEND` | `rocksdb` | `rocksdb` 或 `bincode_file` |
| `NOVOVM_GATEWAY_UA_STORE_PATH` | `artifacts/gateway/unified-account-router.rocksdb` | UA 存储路径 |
| `NOVOVM_GATEWAY_ETH_TX_INDEX_BACKEND` | `memory` | `memory` 或 `rocksdb` |
| `NOVOVM_GATEWAY_ETH_TX_INDEX_PATH` | `artifacts/gateway/eth-tx-index.rocksdb` | TX 索引路径 |

结论：

- 默认不会像原生 full node 那样自动无限吃盘。
- 只有明确切换到持久化索引/全镜像策略时，磁盘会持续增长。

---

## 5. 推荐配置模板（Recommended Profiles）

## 5.1 Full-node Only（推荐）

```powershell
$env:NOVOVM_GATEWAY_BIND = "127.0.0.1:9899"
$env:NOVOVM_GATEWAY_ETH_DEFAULT_CHAIN_ID = "1"

# 禁用上游代理
$env:NOVOVM_GATEWAY_ETH_UPSTREAM_RPC = ""
$env:NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_UPSTREAM_RPC = ""
$env:NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_EXEC = ""

# 原生栈
$env:NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_NATIVE_TRANSPORT = "udp"
$env:NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_NATIVE_NODE_ID = "1"
$env:NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_NATIVE_LISTEN = "127.0.0.1:39001"
$env:NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_ROUTE_POLICY = "auto"
$env:NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_PLUGIN_PORTS = "30303,30304"
$env:NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_NATIVE_PEERS = "2@127.0.0.1:39001,enode://<pubkey>@<eth-peer-ip>:30303"
```

## 5.2 Passive（轻量）

```powershell
$env:NOVOVM_GATEWAY_BIND = "127.0.0.1:9899"
$env:NOVOVM_GATEWAY_ETH_UPSTREAM_RPC = ""
$env:NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_UPSTREAM_RPC = ""
$env:NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_EXEC = ""
$env:NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_NATIVE_PEERS = ""
```

---

## 6. 启动步骤（Setup）

```powershell
cd D:\WEB3_AI\SUPERVM
cargo build -p novovm-evm-gateway
.\target\debug\novovm-evm-gateway.exe
```

---

## 7. 使用示例（Usage）

## 7.1 查询余额（Query Balance）

```json
{"jsonrpc":"2.0","id":1,"method":"eth_getBalance","params":["0xB7CF4018906212Dd49C2CBda288D64176Ea82A3b","latest"]}
```

## 7.2 查询运行时协议能力（Runtime Protocol Caps）

```json
{"jsonrpc":"2.0","id":2,"method":"evm_getRuntimeProtocolCaps","params":{"chain_id":1}}
```

关注字段：

- `native_peer_discovery`
- `native_eth_handshake`
- `native_snap_sync_state_machine`
- `profile`（应为 `native_devp2p_rlpx`）

## 7.3 提交已签名交易（Send Raw Transaction）

```json
{"jsonrpc":"2.0","id":3,"method":"eth_sendRawTransaction","params":{"chain_id":1,"raw_tx":"0x...","require_public_broadcast":true,"return_detail":true}}
```

## 7.4 查询提交状态（Submit Status）

```json
{"jsonrpc":"2.0","id":4,"method":"evm_getTxSubmitStatus","params":{"chain_id":1,"tx_hash":"0x..."}}
```

## 7.5 上报插件会话阶段（Plugin Session Report）

可用于“协议封装在 EVM 插件中，网关只消费阶段状态”的场景。

方法别名：

- `evm_reportPublicBroadcastPluginSession`
- `evm_report_public_broadcast_plugin_session`
- `evm_reportPluginSession`
- `evm_report_plugin_session`

请求示例：

```json
{
  "jsonrpc":"2.0",
  "id":5,
  "method":"evm_reportPublicBroadcastPluginSession",
  "params":{
    "chain_id":1,
    "sessions":[
      {"endpoint":"enode://<pubkey>@<peer-ip>:30303","stage":"ready","updated_ms":1731686400000},
      {"endpoint":"enode://<pubkey2>@<peer-ip>:30303","stage":"ack_seen"}
    ]
  }
}
```

阶段支持：

- `disconnected`
- `tcp_connected`
- `auth_sent`
- `ack_seen`
- `ready`

## 7.6 查询插件 peer 列表（Plugin Peer List）

```json
{"jsonrpc":"2.0","id":6,"method":"evm_getPublicBroadcastPluginPeers","params":{"chain_id":1}}
```

## 7.7 geth 公网连接桥接（一步接公网）

如果插件网络栈由 geth 托管，可直接用脚本把 `admin_peers` 同步到网关阶段缓存：

`scripts/migration/run_evm_geth_plugin_peer_bridge.ps1`

示例：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/migration/run_evm_geth_plugin_peer_bridge.ps1 `
  -GatewayUrl "http://127.0.0.1:9899" `
  -GethUrl "http://127.0.0.1:8545" `
  -ChainId 1
```

## 7.8 插件 mempool 持续吸入后的自动出池（确认回收 + TTL 淘汰）

为避免 `txpool.pending` 长期单向累积，网关支持在 ingest worker 内自动回收：

- 链上确认回收（按 tx hash 轮询 receipt）
- 本地 stale TTL 淘汰（按 `observed_at_unix_ms`）

新增环境变量：

- `NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_CONFIRM_MAX_CHECK_PER_TICK`
  默认 `128`，每个 tick 最多检查多少本地 pending 哈希用于 receipt 回收。可设 `0` 关闭。
- `NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_STALE_TTL_MS`
  默认 `1800000`（30 分钟），超过该年龄的本地 ingress 交易会被淘汰。可设 `0` 关闭。

可在 `evm_getPublicBroadcastStatus` 中观察：

- `native_plugin_mempool_ingest_evicted_total`
- `native_plugin_mempool_ingest_evicted_confirmed_total`
- `native_plugin_mempool_ingest_evicted_stale_total`
- `native_plugin_mempool_ingest_confirm_max_check_per_tick`
- `native_plugin_mempool_ingest_stale_ttl_ms`

---

## 8. 写入 Canary（Full-node Only）

脚本：

`scripts/migration/run_evm_mainnet_write_canary.ps1`

当前默认：

- `BroadcastMode=fullnode_only`
- 不再默认使用 upstream 代理

示例：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/migration/run_evm_mainnet_write_canary.ps1 `
  -BroadcastMode fullnode_only `
  -RawTx "0x<signed_raw_tx>" `
  -FromAddress "0x<sender>"
```

结果文件：

`artifacts/migration/evm-mainnet-write-canary-summary-latest.json`

---

## 8.1 2026-03-17 根因闭环与回归基线

同一 peer（`157.90.35.166:30303`）A/B 结论：

- `go-ethereum` 原生：`ready` 后首帧为 `new_pooled_hashes(0x18)`，可持续 `getPooled -> pooledTxs`。
- `SUPERVM + EVM 插件`：修复后已可复现同一路径（`ready/new_pooled/get_pooled/pooled` 均为正）。

本次根因：

- RLPx 读帧在 `partial read + timeout` 场景下发生“流失步”，导致后续 `header MAC` 失配并掉线。

修复点（代码）：

- `crates/gateways/evm-gateway/src/rpc_gateway_exec_cfg.rs`
  - `gateway_eth_rlpx_read_exact_with_partial`：已读部分后遇到 timeout-like 错误时继续补读，避免丢字节。
  - 增强 timeout-like 识别（含 Windows `os error 10060/10035`）。
  - 新增 `single-session` 模式与 `ready` 后首帧 trace 日志。

建议最小回归命令（固定 peer，单会话，无重连噪声）：

```powershell
powershell -ExecutionPolicy Bypass -File scripts/migration/run_evm_uniswap_observation_window.ps1 `
  -SkipBuild -EnablePluginMempoolIngest -RlpxSingleSession -SmokeAssert `
  -FixedPluginEnode "enode://4aeb4ab6c14b23e2c4cfdce879c04b0748a20d8e9b59e25ded2a08143e265c6c25936e74cbc8e641e3312ca288673d91f2f93f8e277de3cfa444ecdaaf982052@157.90.35.166:30303" `
  -DurationMinutes 12 -IntervalSeconds 5 -WarmupSeconds 6 `
  -PluginMinCandidates 1 -RlpxMaxPeersPerTick 1 -RlpxHelloProfile geth
```

`-SmokeAssert` 默认断言：

- `ready >= 1`
- `new_pooled_hashes >= 1`
- `pooled_txs >= 1`
- `first_post_ready_frame_code == 0x18`

---

## 9. 常见问题（FAQ）

### Q1: 会不会一启动就和以太坊主网全量同步，占满流量？

不会。是否进入持续同步取决于你是否配置了 native peers 或 upstream。

### Q4: 端口会不会冲突？SUPERVM 主网和以太坊是否会混线？

不会，前提是按策略分层：

- 以太坊公网默认 P2P 是 `30303`（TCP/UDP 发现）。
- SUPERVM 主网走隐私网络形态，建议使用独立端口段（如 `39xxx`）。
- 启用 `NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_ROUTE_POLICY=auto` + `NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_PLUGIN_PORTS`，可把不同端口映射到不同路由策略，降低混线风险。

### Q2: 会不会像原生 full node 一样持续吃盘？

默认不会。`ETH TX Index` 默认是 `memory`，不会持续落盘。切到 `rocksdb` 才会持续增长。

### Q3: fullnode_only 成功标准是什么？

主线标准是“本机 native 广播成功 + 提交状态可回查（pending/accepted/onchain）”。  
是否强制等待 receipt 由场景决定。

