# NOVOVM EVM Wallet RPC Entry v1（2026-06-13）

## 目标

本文件定义 `evm-product-v1` 之后的钱包/用户入口接入契约。

目标不是继续扩展 EVM RPC，也不是实现完整 geth 钱包生态面，而是让真实钱包或前端按标准 JSON-RPC 路径使用 NovoVM EVM 产品能力：

```text
NOVO Wallet / EIP-1193 provider
  -> RPC endpoint
  -> eth_sendRawTransaction
  -> NovoVM EVM Product v1
  -> eth_getTransactionReceipt
  -> receipt / block / status
```

## 用户入口边界

钱包侧只依赖以下最小接口：

- `eth_chainId`
- `eth_sendRawTransaction`
- `eth_getTransactionReceipt`
- `eth_getTransactionByHash`
- `eth_getBlockByNumber`
- `eth_call`
- `eth_estimateGas`

本阶段不要求钱包依赖：

- `debug_*`
- `trace_*`
- `admin_*`
- archive query
- geth internal txpool control

## 一次性账户绑定

NovoVM 宿主仍以 UCA 作为账户主体。EVM 钱包地址作为 persona 绑定到 UCA。

产品接入时，钱包或上层账户服务必须先通过主线统一账户入口完成。该入口唯一归属：

```text
novovm-node -> mainline_query -> unified_account_surface
```

`evm-gateway` 不再作为 UCA 创建、绑定、策略或资产入口；它只作为 EVM RPC adapter。账户服务必须先完成：

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "ua_createUca",
  "params": {
    "uca_id": "uca:user-001"
  }
}
```

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "ua_bindPersona",
  "params": {
    "uca_id": "uca:user-001",
    "persona_type": "evm",
    "chain_id": 1,
    "external_address": "0x..."
  }
}
```

绑定后，`eth_sendRawTransaction` 可以从 raw transaction 恢复 sender，并映射到 UCA owner。

## 钱包发送交易

钱包侧保持 Ethereum 习惯：本地签名，提交 raw transaction。

```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "eth_sendRawTransaction",
  "params": [
    "0x..."
  ]
}
```

成功响应必须是标准 tx hash：

```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "result": "0x..."
}
```

约束：

- `eth_sendRawTransaction` 不等待执行完成。
- gateway 作为 EVM RPC adapter 接收后写入 native pending runtime。
- 后台 consumer 负责进入 mainline EVM execution。
- 用户通过 receipt 轮询观察最终结果。

## 查询 receipt

钱包或前端轮询：

```json
{
  "jsonrpc": "2.0",
  "id": 4,
  "method": "eth_getTransactionReceipt",
  "params": [
    "0x..."
  ]
}
```

成功后至少依赖以下字段：

- `status`
- `transactionHash`
- `blockHash`
- `blockNumber`
- `gasUsed`

## EIP-1193 前端形态

前端或 NOVO Wallet provider 可保持标准调用形态：

```javascript
const txHash = await provider.request({
  method: "eth_sendRawTransaction",
  params: [rawTx],
});

let receipt = null;
while (!receipt) {
  receipt = await provider.request({
    method: "eth_getTransactionReceipt",
    params: [txHash],
  });
  if (!receipt) {
    await new Promise((resolve) => setTimeout(resolve, 1000));
  }
}
```

## 已有产品证据

`evm-product-v1` 已包含真实 HTTP JSON-RPC smoke：

```powershell
cargo test -p novovm-evm-gateway json_rpc_eth_send_raw_then_receipt_product_smoke -- --nocapture
```

该 smoke 已验证：

- HTTP JSON-RPC `eth_sendRawTransaction` 返回 `tx_hash`。
- gateway 后台 consumer 执行 pending raw transaction。
- `eth_getTransactionReceipt` 返回 canonical receipt。
- receipt 包含 `status/blockHash/blockNumber/transactionHash/gasUsed`。

## 下一步

下一步只做最小用户入口接入：

1. NOVO Wallet 配置 RPC endpoint。
2. 钱包地址绑定到 UCA。
3. 钱包本地签名 raw transaction。
4. 通过 `eth_sendRawTransaction` 提交。
5. 通过 `eth_getTransactionReceipt` 展示状态。

不在本阶段扩展 debug/admin/trace，也不重新打开 geth 全节点追平目标。
