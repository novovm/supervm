# NOVOVM EVM Product v1 签收记录（2026-06-13）

## 结论

`NOVOVM EVM Product v1` 已签收。

当前版本不再定位为 EVM 研究验证，而是 NovoVM 宿主里的最小可用 EVM RPC 产品能力：

```text
用户
  -> HTTP JSON-RPC
  -> eth_sendRawTransaction
  -> gateway
  -> native pending runtime
  -> gateway pending consumer
  -> mainline EVM execution
  -> canonical batch
  -> IncludedCanonical
  -> eth_getTransactionReceipt
```

## 冻结指针

- product tag: `evm-product-v1`
- product implementation commit: `8d99321 Add EVM JSON-RPC product smoke`
- product tag target: 包含本签收文档的提交
- prerequisite tag: `evm-mainnet-long-sync-v1`
- prerequisite commit: `d48846475bb42abdc12ee5cbd179f5f7bc756065`

## v1 产品面

本版本签收的最小产品闭环是：

- `eth_sendRawTransaction` 接收 raw transaction，并立即返回标准 `tx_hash`。
- raw transaction 进入 NovoVM native pending runtime。
- gateway 后台 pending consumer 消费 pending raw transaction。
- mainline EVM 执行交易并写入 canonical batch。
- native pending lifecycle 更新为 `IncludedCanonical`。
- `eth_getTransactionReceipt` 返回标准 receipt 本体。
- JSON-RPC 产品入口可按真实用户路径完成 `sendRaw -> receipt`。

## 已验证门禁

```powershell
cargo test -p novovm-evm-gateway json_rpc_eth_send_raw_then_receipt_product_smoke -- --nocapture
cargo test -p novovm-evm-gateway gateway_pending_consumer_executes_raw_tx_into_mainline_canonical -- --nocapture
cargo test -p novovm-evm-gateway eth_send_raw_transaction_without_uca_id_uses_binding_owner -- --nocapture
cargo check -p novovm-evm-gateway
cargo fmt --check
git diff --check
```

核心验收点：

- `eth_sendRawTransaction` 不阻塞等待执行完成。
- `eth_sendRawTransaction` 返回值保持 Ethereum JSON-RPC 习惯：只返回 `tx_hash`。
- receipt 查询通过 canonical 投影返回，不依赖临时 pending 视图。
- smoke 从 HTTP JSON-RPC 入口发起，而不是只调用内部 Rust 函数。
- receipt 至少包含 `status`、`blockHash`、`blockNumber`、`transactionHash`、`gasUsed`。

## 明确边界

本 tag 不声明：

- 完整 geth 替代品。
- archive node。
- debug/admin/trace 全量 RPC 面。
- 继续追逐 geth 内部实现一致。
- Amsterdam / EIP-8037 提前纳入 v1。

本 tag 声明：

- NovoVM 宿主已具备最小可用 EVM RPC 产品闭环。
- EVM 能力已从插件验证进入宿主产品路径。
- 后续重点转向钱包/RPC/账户体系的真实用户接入，而不是继续扩展 geth 全节点目标。

## 后续方向

后续只允许围绕用户接入做最小推进：

- 钱包通过 RPC 使用 `eth_sendRawTransaction`。
- 用户通过 `eth_getTransactionReceipt` 查询交易结果。
- EVM persona 映射到 NovoVM 账户体系。
- 必要时补齐 `eth_getTransactionByHash`、`eth_getBlockByNumber`、`eth_call`、`eth_estimateGas` 的产品 smoke，但不扩大到 debug/admin/trace。

钱包/用户入口接入契约见：

- `docs_CN/Adapters/EVM/NOVOVM-EVM-WALLET-RPC-ENTRY-V1-2026-06-13.md`
