# NOVOVM EVM / geth 等价性 Readiness 矩阵（2026-06-06）

## 当前结论

当前结论是：

`SUPERVM 已具备 Novo 主网可控 EVM 插件执行闭环，并通过 geth ethapi 样本级 parity；但不能声明等价 geth / 以太坊全节点。`

这个文档用于约束产品口径：

1. 已通过的能力可以作为 Novo 主网 EVM 插件能力线。
2. 未覆盖的能力不能包装成“以太坊节点等价”。
3. 每次推进必须用可复现实跑命令更新本矩阵，而不是只更新描述。

## 本轮实跑证据

### 1. 默认 geth parity fixture

命令：

```powershell
cargo test -p novovm-node mainline_query::tests::eth_end_to_end_geth_sample_batch_parity_report_from_files_v1 -- --nocapture
```

结果：

- `sampleCount = 11`
- `totalMismatchCount = 0`
- `failedSamples = []`

覆盖样本包括：

- blob tx success/failure
- create contract with access list
- deploy success/fail
- dynamic fee failure
- legacy logs
- reorg canonical/noncanonical log ownership
- type2 intrinsic gas / fee edge failure

### 2. 外部 go-ethereum ethapi export parity

本机 go-ethereum：

- path: `D:\WEB3_AI\go-ethereum`
- commit: `13d8df63f core/types/bal: improve the bal validation (#35110)`

同步 dry-run：

```powershell
$env:NOVOVM_GETH_REPO_ROOT='D:\WEB3_AI\go-ethereum'
cargo run -p novovm-node --bin supervm-mainline-geth-sample-sync -- --dry-run
```

结果：

- `source = D:\WEB3_AI\go-ethereum\internal\ethapi\testdata`
- `processed = 11`

外部 parity：

```powershell
$env:NOVOVM_GETH_REPO_ROOT='D:\WEB3_AI\go-ethereum'
$env:NOVOVM_GETH_PARITY_SAMPLE_DIR='D:\WEB3_AI\SUPERVM\crates\novovm-node\tests\fixtures\geth-parity-external'
cargo test -p novovm-node mainline_query::tests::eth_end_to_end_geth_sample_batch_parity_report_from_files_v1 -- --nocapture
```

结果：

- `sampleCount = 11`
- `totalMismatchCount = 0`
- `failedSamples = []`

覆盖的外部 geth ethapi 数据包括：

- `eth_getTransactionReceipt-blob-tx.json`
- `eth_getTransactionReceipt-create-contract-tx.json`
- `eth_getTransactionReceipt-create-contract-with-access-list.json`
- `eth_getTransactionReceipt-dynamic-tx-with-logs.json`
- `eth_getTransactionReceipt-normal-transfer-tx.json`
- `eth_getTransactionReceipt-with-logs.json`
- `eth_getBlockReceipts-*` 样本

### 3. Mainline EVM host + BAL 严格扫描

本轮已跑通真实链路：

`novovm-txgen -> novovm-node --mainline-evm-host -> canonical store -> novovmctl evm-block-access-list-scan`

严格扫描结果：

- `scanned = 1`
- `problems = 0`
- `payload_present = 1`
- `complete = 1`
- `hash_present = 1`
- `complete_with_hash = 1`

这证明的是 controlled mainline transfer smoke 的 BAL 生产、canonical 落盘和 scanner 校验闭环，不等于全部 EVM 交易类型的 BAL 完整性。

### 4. Contract call/deploy BAL metadata

命令：

```powershell
cargo test -p novovm-adapter-novovm execute_transaction_with_observed_metadata_emits_complete -- --nocapture
cargo test -p novovm-adapter-evm-plugin plugin_apply_v2_exports_complete_contract_call_bal_metadata -- --nocapture
cargo test -p novovm-adapter-evm-plugin plugin_apply_v2_exports_complete_contract_deploy_bal_metadata -- --nocapture
cargo test -p novovm-adapter-evm-plugin plugin_apply_v2_can_export_and_ingest_execution_receipts -- --nocapture
```

结果：

- transfer BAL complete: pass
- contract call BAL complete: pass
- contract call plugin metadata hash: pass
- contract deploy BAL complete: pass
- contract deploy plugin metadata hash: pass
- contract call + deploy mixed batch complete/hash present: pass

这证明成功 contract call 路径的 nonce、balance 和 storage write 已进入 BAL；成功 contract deploy 路径的合约账户、余额、runtime code、deploy code-hash storage 已进入 BAL，并能上升到 plugin block metadata hash。

### 5. Raw Ethereum transaction mainline host smoke

命令：

```powershell
$env:NOVOVM_AVAILABILITY_FORCE_MODE='normal'
$env:NOVOVM_NODE_VERBOSE='1'
$env:NOVOVM_ETH_SEND_RAW_TX='0xf864808504a817c800825208943535353535353535353535353535353535353535018025a0cb1ae5eeb22ada6e0cc8090f480d614711af806a2534b7651ab9577617cf6078a0420db11989647a09a73eefbba26361a2b065ffd41c41ba84089584ce267f7fbe'
cargo run -p novovm-node --bin novovm-node -- `
  --mainline-evm-host `
  --mainline-evm-chain-id 1 `
  --mainline-evm-canonical-store-path artifacts/mainline/evm-raw-real-smoke/canonical-raw-20260606.json `
  --d1-ingress-mode auto

cargo run -p novovmctl -- evm-block-access-list-scan `
  --store-path artifacts/mainline/evm-raw-real-smoke/canonical-raw-20260606.json `
  --latest-count 16 `
  --require-payload `
  --require-complete `
  --require-hash-when-complete
```

结果：

- signed legacy raw tx -> recovered sender -> EVM `TxIR`: pass
- raw tx mainline host execution: `submitted_total=1 processed_total=1 success_total=1 canonical_batches_total=1`
- raw tx canonical BAL strict scan: `problems=0 complete_with_hash=1`
- signed type1 access-list transfer smoke: pass, BAL strict scan `problems=0 complete_with_hash=1`
- signed type2 dynamic-fee transfer smoke: pass, BAL strict scan `problems=0 complete_with_hash=1`
- signed type3 blob transfer smoke: pass with `NOVOVM_EVM_ENABLE_TYPE3_WRITE_CHAIN_1=1`, BAL strict scan `problems=0 complete_with_hash=1`
- signed type1 contract call/deploy smoke: pass, BAL strict scan `problems=0 complete_with_hash=1`
- signed type2 contract call/deploy smoke: pass, BAL strict scan `problems=0 complete_with_hash=1`
- signed type3 contract call smoke: pass with `NOVOVM_EVM_ENABLE_TYPE3_WRITE_CHAIN_1=1`, BAL strict scan `problems=0 complete_with_hash=1`
- raw signed legacy nonce gap -> adapter unified-account ingress reject: pass, `nonce rejected: expected 1, got 9`
- typed type2 intrinsic gas too low semantic reject: pass, `intrinsic gas too low`
- contract failure/revert artifact baseline matrix: pass, covers revert/out-of-gas/invalid/deploy-failed classifications and receipt gas metadata
- execution-spec/fork-rule smoke matrix: pass, covers intrinsic gas, access-list gas, Amsterdam calldata/access-list floor, precompile set, create/call/revert, storage write and rebuilt logs
- eth/71 BAL wire smoke: pass, covers GetBlockAccessLists/BlockAccessLists payload encode/decode, frame roundtrip, malformed BAL rejection, and safe negotiation fallback to eth/70 when remote advertises eth/71

这证明 `NOVOVM_ETH_SEND_RAW_TX(_FILE)` 可以作为 Novo mainline EVM host 的真实输入源，执行后产出 canonical batch 和完整 BAL hash。当前覆盖 signed legacy/type1/type2/type3 transfer smoke，以及 type1/type2 call/deploy、type3 call smoke；type3 仍是显式开关能力，不能外推到全部 fork rule / gas / blob sidecar 语义。

失败路径方面，当前已经证明 raw signed transaction 在解码和签名恢复后，会进入 Novo 统一账户控制面并被 nonce gate 拒绝；typed gas 语义和 contract failure/revert artifact 仍是 adapter 层样本门禁，不声明覆盖全部 geth txpool / execution failure 行为。

fork-rule 方面，当前只有最小 smoke matrix：覆盖 EVM core gas/precompile 规则和 adapter create/call/revert 执行结果，不等价于 Ethereum execution-spec 全量 fixture。

eth/71 BAL wire 方面，当前只证明 BAL request/response payload 和 RLPx frame 可解析，并证明本产品不会在未完成 eth/71 peer sync 前误协商到 eth/71；这仍不是完整 eth/71 peer sync。

### 6. Gateway JSON-RPC 产品面 smoke

命令：

```powershell
cargo test -p novovm-evm-gateway json_rpc_parity_surface_smoke_block_tx_filter_call_estimate_v1 -- --nocapture
```

结果：

- pass
- 覆盖 `eth_blockNumber`
- 覆盖 `eth_getBlockByNumber`
- 覆盖 `eth_getBlockByHash`
- 覆盖 `eth_getTransactionByHash`
- 覆盖 `eth_newFilter`
- 覆盖 `eth_getFilterLogs`
- 覆盖 `eth_getFilterChanges`
- 覆盖 `eth_call`
- 覆盖 `eth_estimateGas`

这证明 gateway 层的 EVM JSON-RPC 控制面已经能把 block、tx、filter/log、read-only call 和 gas estimation 串成一个最小产品面；仍不等于 geth 全 RPC、tracing/debug/admin 或完整以太坊节点等价。

### 7. Gateway JSON-RPC indexed block/tx/receipt smoke

命令：

```powershell
cargo test -p novovm-evm-gateway json_rpc_indexed_block_tx_receipt_uncle_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 覆盖 `eth_getTransactionByBlockNumberAndIndex`
- 覆盖 `eth_getTransactionByBlockHashAndIndex`
- 覆盖 `eth_getBlockTransactionCountByNumber`
- 覆盖 `eth_getBlockTransactionCountByHash`
- 覆盖 `eth_getBlockReceipts`
- 覆盖 `eth_getTransactionReceipt`
- 覆盖 `eth_getUncleCountByBlockNumber`
- 覆盖 `eth_getUncleCountByBlockHash`
- 覆盖 `eth_getUncleByBlockNumberAndIndex`
- 覆盖 `eth_getUncleByBlockHashAndIndex`

这证明 gateway 层常用 indexed block/tx/receipt 查询面有独立回归门禁；uncle 当前按 minimal mirror mode 返回空/0，不能解释成完整以太坊 uncle 数据支持。

## Readiness 矩阵

| 能力域 | 当前状态 | 证据 | 产品口径 |
| --- | --- | --- | --- |
| Novo mainline EVM host 执行闭环 | Pass | `submitted_total=16 processed_total=16 success_total=16 writes_total=16` | 可作为 Novo 主网控制 EVM 插件能力线 |
| Canonical store + BAL payload | Pass | strict scan `problems=0 complete_with_hash=1` | transfer smoke 可用 |
| contract call BAL 完整性 | Pass | adapter + plugin metadata tests pass, hash present | 成功 contract call 样本可声明 BAL 完整 |
| contract deploy BAL 完整性 | Pass | adapter + plugin metadata tests pass, hash present | 成功 contract deploy 样本可声明 BAL 完整 |
| geth ethapi receipt/log parity | Pass | 默认 fixture `sampleCount=11 totalMismatchCount=0` | 样本级兼容可声明 |
| 最新 go-ethereum ethapi export parity | Pass | external fixture `sampleCount=11 totalMismatchCount=0` | 对当前本机 geth ethapi 测试数据无 mismatch |
| typed tx failure / revert / fee edge parity | Pass | parity sections `typedTxFailure.mismatchCount=0` | 样本级可声明 |
| reorg canonical/noncanonical log view | Pass | parity sections `logs.mismatchCount=0` | 样本级可声明 |
| eth/71 BAL 相关 wire 能力 | Partial | BAL payload/canonical/scanner pass；eth/71 BAL wire encode/decode/frame + safe negotiation gate pass；未证明完整 eth/71 peer sync | 可声明 eth/71 BAL wire smoke；不能声明完整 eth/71 等价 |
| Ethereum fork rules / gas accounting / precompiles | Partial | execution-spec/fork-rule smoke matrix pass；未跑 Ethereum execution-spec 全量 fixture | 可声明样本级 fork-rule gate；不能声明 EVM 语义全等价 |
| raw Ethereum transaction ingestion/execution | Partial | signed legacy/type1/type2/type3 transfer + typed call/deploy smoke pass；raw nonce gap reject pass；typed gas/revert artifact gate pass；BAL strict scan pass | 可声明 raw transfer/call/deploy smoke 可执行且关键失败路径有 gate；不能声明 raw tx 全等价 |
| JSON-RPC full-node surface | Partial | mainline query receipt/log 样本 pass；gateway block/tx/filter/call/estimateGas smoke pass；indexed block/tx/receipt/uncle smoke pass；未覆盖 tracing/debug/admin 和全 geth RPC 行为 | 可声明 gateway JSON-RPC 产品面样本可用；不能声明 geth RPC 等价 |
| devp2p/RLPx peer sync / block import | Partial | 有 gateway/network 代码和 canary，但未作为本矩阵通过项 | 不能声明以太坊全节点 |

## 当前产品判定

可以声明：

`SUPERVM 当前具备 Novo 主网可控 EVM 插件执行能力，能产出 canonical EVM block metadata，并对 BAL payload 进行严格扫描；对 geth ethapi receipt/log/typed-failure 样本具备 parity。`

不能声明：

`SUPERVM 是 geth 等价实现。`

`SUPERVM 是完整以太坊全节点。`

`SUPERVM 已完整支持 eth/71 P2P 同步和全部 BAL wire 行为。`

## 下一步门禁顺序

1. 如继续扩展 JSON-RPC parity，可补 pending/runtime 与 store recovery 的产品 gate；tracing/debug/admin 仍不作为 Novo EVM 插件主线优先项。
2. 如需要继续强化 raw tx 产品面，再补 txpool replacement、account balance/fee debit、access-list/storage warmup 的分层 gate。
3. 如需要提高 fork-rule 置信度，再接入 Ethereum execution-spec 官方 fixture 子集，但仍作为插件门禁，不改变 SUPERVM 主产品边界。
4. 如需要提高 eth/71 置信度，再做真实 peer sync/capability negotiation 集成门禁，但仍不把 SUPERVM 产品口径改成 geth 全节点。

## 回归命令

默认 geth parity：

```powershell
cargo test -p novovm-node mainline_query::tests::eth_end_to_end_geth_sample_batch_parity_report_from_files_v1 -- --nocapture
```

外部 geth parity：

```powershell
$env:NOVOVM_GETH_REPO_ROOT='D:\WEB3_AI\go-ethereum'
$env:NOVOVM_GETH_PARITY_SAMPLE_DIR='D:\WEB3_AI\SUPERVM\crates\novovm-node\tests\fixtures\geth-parity-external'
cargo test -p novovm-node mainline_query::tests::eth_end_to_end_geth_sample_batch_parity_report_from_files_v1 -- --nocapture
```

BAL 严格扫描：

```powershell
cargo run -p novovmctl -- evm-block-access-list-scan `
  --store-path artifacts/mainline/evm-bal-real-smoke/canonical-complete.json `
  --latest-count 16 `
  --require-payload `
  --require-complete `
  --require-hash-when-complete
```

Raw Ethereum tx mainline host smoke：

```powershell
$env:NOVOVM_AVAILABILITY_FORCE_MODE='normal'
$env:NOVOVM_ETH_SEND_RAW_TX='0xf864808504a817c800825208943535353535353535353535353535353535353535018025a0cb1ae5eeb22ada6e0cc8090f480d614711af806a2534b7651ab9577617cf6078a0420db11989647a09a73eefbba26361a2b065ffd41c41ba84089584ce267f7fbe'
cargo run -p novovm-node --bin novovm-node -- `
  --mainline-evm-host `
  --mainline-evm-chain-id 1 `
  --mainline-evm-canonical-store-path artifacts/mainline/evm-raw-real-smoke/canonical-raw-20260606.json `
  --d1-ingress-mode auto
```

Raw typed tx BAL strict scan stores：

```powershell
cargo run -p novovmctl -- evm-block-access-list-scan --store-path artifacts/mainline/evm-raw-real-smoke/canonical-type1-20260606.json --latest-count 16 --require-payload --require-complete --require-hash-when-complete
cargo run -p novovmctl -- evm-block-access-list-scan --store-path artifacts/mainline/evm-raw-real-smoke/canonical-type2-20260606.json --latest-count 16 --require-payload --require-complete --require-hash-when-complete
cargo run -p novovmctl -- evm-block-access-list-scan --store-path artifacts/mainline/evm-raw-real-smoke/canonical-type3-20260606.json --latest-count 16 --require-payload --require-complete --require-hash-when-complete
cargo run -p novovmctl -- evm-block-access-list-scan --store-path artifacts/mainline/evm-raw-real-smoke/canonical-type1-call-20260606.json --latest-count 16 --require-payload --require-complete --require-hash-when-complete
cargo run -p novovmctl -- evm-block-access-list-scan --store-path artifacts/mainline/evm-raw-real-smoke/canonical-type1-deploy-20260606.json --latest-count 16 --require-payload --require-complete --require-hash-when-complete
cargo run -p novovmctl -- evm-block-access-list-scan --store-path artifacts/mainline/evm-raw-real-smoke/canonical-type2-call-20260606.json --latest-count 16 --require-payload --require-complete --require-hash-when-complete
cargo run -p novovmctl -- evm-block-access-list-scan --store-path artifacts/mainline/evm-raw-real-smoke/canonical-type2-deploy-20260606.json --latest-count 16 --require-payload --require-complete --require-hash-when-complete
cargo run -p novovmctl -- evm-block-access-list-scan --store-path artifacts/mainline/evm-raw-real-smoke/canonical-type3-call-20260606.json --latest-count 16 --require-payload --require-complete --require-hash-when-complete
```

Raw/failure path regression gates：

```powershell
cargo test -p novovm-node eth_send_raw_tx_ingress_tests --bin novovm-node -- --nocapture
cargo test -p novovm-adapter-novovm typed_type2_semantics_reject_intrinsic_gas_too_low_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_equivalence_baseline_matrix_receipt_revert_gas_v1 -- --nocapture
```

Execution-spec/fork-rule smoke gate：

```powershell
cargo test -p novovm-adapter-novovm evm_execution_spec_fork_rule_smoke_matrix_v1 -- --nocapture
```

eth/71 BAL wire smoke gate：

```powershell
cargo test -p novovm-network eth71_bal_wire_roundtrip_and_negotiation_gate_v1 -- --nocapture
```

Gateway JSON-RPC 产品面 smoke：

```powershell
cargo test -p novovm-evm-gateway json_rpc_parity_surface_smoke_block_tx_filter_call_estimate_v1 -- --nocapture
```

Gateway JSON-RPC indexed block/tx/receipt smoke：

```powershell
cargo test -p novovm-evm-gateway json_rpc_indexed_block_tx_receipt_uncle_surface_smoke_v1 -- --nocapture
```
