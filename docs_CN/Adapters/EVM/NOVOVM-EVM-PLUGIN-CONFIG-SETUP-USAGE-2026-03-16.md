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
| `NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_NATIVE_PEERS` | 对端列表，格式：`nodeId@host:port,nodeId@host:port` |
| `NOVOVM_NETWORK_ENABLE_GOSSIP_SYNC_COMPAT` | 旧兼容开关；EVM 进程默认注入 `0`（关闭旧 gossip 同步） |

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
$env:NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_NATIVE_PEERS = "2@127.0.0.1:39001"
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

## 9. 常见问题（FAQ）

### Q1: 会不会一启动就和以太坊主网全量同步，占满流量？

不会。是否进入持续同步取决于你是否配置了 native peers 或 upstream。

### Q2: 会不会像原生 full node 一样持续吃盘？

默认不会。`ETH TX Index` 默认是 `memory`，不会持续落盘。切到 `rocksdb` 才会持续增长。

### Q3: fullnode_only 成功标准是什么？

主线标准是“本机 native 广播成功 + 提交状态可回查（pending/accepted/onchain）”。  
是否强制等待 receipt 由场景决定。

