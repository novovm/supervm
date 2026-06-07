# NOVOVM EVM / geth 等价性 Readiness 矩阵（2026-06-06）

## 当前结论

当前结论是：

`SUPERVM 已具备 Novo 主网可控 EVM 插件执行闭环，并通过 geth ethapi 样本级 parity；可以按有限门禁声明协议可观察等价 v1，但不能声明完整 geth / 以太坊全节点。`

协议可观察等价 v1 的收口标准见：

- `docs_CN/Adapters/EVM/NOVOVM-EVM-PROTOCOL-OBSERVABLE-EQUIVALENCE-V1-2026-06-07.md`

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
- CREATE/CALL failure state invariant smoke: pass, covers failed CALL no value/storage/log commit and failed CREATE no contract account/code/storage/BAL contract entry
- execution-spec/fork-rule smoke matrix: pass, covers intrinsic gas, access-list gas, Amsterdam calldata/access-list floor, precompile set, create/call/revert, storage write and rebuilt logs
- eth/71 BAL wire smoke: pass, covers GetBlockAccessLists/BlockAccessLists payload encode/decode, frame roundtrip, malformed BAL rejection, and safe negotiation fallback to eth/70 when remote advertises eth/71
- eth/71/BAL plugin real RLPx response gate: pass on no-snap plugin path, covers inbound real `GetBlockAccessLists` frame -> protocol-valid `BlockAccessLists` response with request_id/count preserved, mainline canonical BAL materialized into network runtime, local BAL RLP returned, and missing sentinel for unavailable local BAL payload
- snap/1 AccountRange real RLPx gate: pass, covers eth/70+snap/1 capability offset (`0x22/0x23`) -> State-phase `GetAccountRange` using native head `stateRoot` -> matched `AccountRange` response observed；prevents snap wire codes from being misparsed as BAL when snap is negotiated
- snap/1 AccountRange -> StorageRanges/ByteCodes follow-up/cache gate: pass, covers non-empty slim account response -> storage root / code hash extraction -> real `GetStorageRanges` + `GetByteCodes` follow-up requests -> matched responses populate native snap account/storage/code cache, with bytecode codeHash checked before caching；still does not claim proof/root verification or full trie heal
- snap/1 sidecar service real RLPx gate: pass, covers inbound real `GetStorageRanges`/`GetByteCodes`/`GetTrieNodes` -> protocol-valid empty `StorageRanges`/`ByteCodes`/`TrieNodes` responses；prevents negotiated snap/1 peers from seeing silent drops on these service requests, but still does not claim full snap state heal/download/store
- novovm-node direct RLPx sync entry: pass for finite live mainnet run via `NOVOVM_NODE_MODE=eth_rlpx_sync`, covers real Status -> Headers -> Bodies -> Receipts and native current advancing from 0 to 5120；默认候选池已从 Ethereum mainnet geth bootnodes 扩展到 geth DNS discovery `all.mainnet.ethdisco.net` ENR 解析，实测候选从 4 扩到 64；still does not claim DNS tree signature verification, full discv4 peer churn, or full long-haul catch-up
- RLPx remote-best sticky target gate: pass, 75 tick live soak 暴露断线后 `highest` 回落到 local current 的问题；runtime sync status 已增加 5 分钟 remote-best hint，后续 live run 中断线后 `highest` 保持远端高度、不再丢失追高目标（实测 `current=5120` 时 `highest=25267501`）；still does not claim full long-haul catch-up
- RLPx dead session cleanup gate: pass, 60 tick live soak 暴露 EOF/remote-closed 后 dead RLPx stream 留在 session map、导致后续 tick 选中坏 session 但不继续发下一段 headers 的问题；EOF/remote-closed 现在会 unregister runtime peer、删除 live session、下一轮重新 bootstrap，实测 current 已越过此前卡住的 2048 并推进到 5120；still does not claim geth-grade peer churn
- novovm-node RLPx checkpoint gate: pass, `NOVOVM_ETH_RLPX_CHECKPOINT_ENABLED` 默认开启，`NOVOVM_ETH_RLPX_CHECKPOINT_PATH` 可覆盖路径；产品入口启动时会恢复 checkpoint 中的 current/highest，tick 后写回最新 sync/header 进度，实测临时 checkpoint `current=1234/highest=5678` 可恢复到 tick 输出；still does not claim full block/state/receipt durable store
- RLPx NewBlockHashes gate: pass, covers inbound real `NewBlockHashes` announcement -> peer head/highest update -> follow-up `GetBlockHeaders`
- RLPx BlockBodies gate: pass, covers real `BlockHeaders`/`BlockBodies` sync -> body raw transaction MPT `transactionsRoot` validated -> native body snapshot import
- RLPx Receipts gate: pass, covers real `BlockHeaders`/`BlockBodies` sync -> follow-up eth/70 `GetReceipts(firstBlockReceiptIndex=0)` -> complete `Receipts(lastBlockIncomplete=false)` parsed -> receipt count 与 body tx count 对齐 -> raw receipt MPT `receiptsRoot` validated before peer sync ready -> native receipt snapshot 落地 -> 本地 `GetReceipts` 可回放已验证 raw receipts -> 父块已保留时 empty/no-withdrawal block stateRoot continuity validation；incomplete/block-count/count/root mismatch 和可判定 stateRoot continuity mismatch 会拒绝
- RLPx pooled tx gates: pass, covers inbound real `NewPooledTransactionHashes` -> `GetPooledTransactions` -> raw `PooledTransactions` materialized into pending tx payload, and inbound real `GetPooledTransactions` -> local raw tx `PooledTransactions` response
- RLPx NewBlock gate: pass, covers inbound real non-empty `NewBlock` announcement -> Ethereum transaction trie `transactionsRoot` validation -> empty ommers/withdrawals validation -> native header/body snapshot import -> peer head/highest update -> follow-up `GetReceipts` -> raw receipt MPT `receiptsRoot` validation -> native receipt snapshot

这证明 `NOVOVM_ETH_SEND_RAW_TX(_FILE)` 可以作为 Novo mainline EVM host 的真实输入源，执行后产出 canonical batch 和完整 BAL hash。当前覆盖 signed legacy/type1/type2/type3 transfer smoke，以及 type1/type2 call/deploy、type3 call smoke；type3 仍是显式开关能力，不能外推到全部 fork rule / gas / blob sidecar 语义。

失败路径方面，当前已经证明 raw signed transaction 在解码和签名恢复后，会进入 Novo 统一账户控制面并被 nonce gate 拒绝；typed gas 语义和 contract failure/revert artifact 仍是 adapter 层样本门禁，不声明覆盖全部 geth txpool / execution failure 行为。

fork-rule 方面，当前只有最小 smoke matrix：覆盖 EVM core gas/precompile 规则和 adapter create/call/revert 执行结果，不等价于 Ethereum execution-spec 全量 fixture。

CREATE/CALL failure 方面，当前已证明 failed CALL 不提交 value transfer、target storage write、event logs；failed CREATE 即使 artifact 携带 contract_address/runtime_code，也不会创建 contract account/code/storage，也不会产出 contract BAL entry。

eth/71/BAL wire 方面，当前已证明 BAL request/response payload 和 RLPx frame 可解析；在无 snap 协商的插件路径里，真实 RLPx peer 请求 BAL 时产品会返回协议合法响应；mainline canonical batch append 后会把 persisted block BAL materialize 到 network runtime，对已 materialize 的本地 BAL 返回真实 RLP，对缺失 payload 返回 missing sentinel，并证明本产品不会在未完成 eth/71 peer sync 前误协商到 eth/71。主网 eth/70+snap/1 下，global code `0x22/0x23` 属于 snap `GetAccountRange/AccountRange`，不会再被 BAL 插件抢占；已协商 snap/1 peer 入站请求 `GetStorageRanges`、`GetByteCodes`、`GetTrieNodes` 时，SUPERVM 会返回协议合法空 `StorageRanges`、`ByteCodes`、`TrieNodes` 响应，不再静默丢弃这些服务面请求。这仍不是完整 eth/71 peer sync，也不声明所有 block BAL payload 都已可用。

RLPx 主网同步方面，当前已新增 `NewBlock` / `NewBlockHashes` 公告处理、pooled tx hash/request/response 链路、receipts wire-level 同步链路、最小 snap/1 AccountRange 拉取链路和 snap/1 sidecar 服务面：真实 peer 发出新头公告后，SUPERVM 会更新 peer head/highest 并主动发起后续同步；`NewBlock` 已按 Ethereum raw transaction trie 校验 `transactionsRoot`，空交易 body 也会校验 Ethereum empty trie root、empty ommers hash 和可见的 empty withdrawals root，不再无条件导入明显错误的块体；`BlockBodies` 拉取路径也会用 body raw tx 复算 header `transactionsRoot`，校验通过才导入 native body snapshot；`BlockBodies` 或 `NewBlock` 返回后会继续发 eth/70 `GetReceipts(firstBlockReceiptIndex=0)`，收到 `Receipts` 后会拒绝 `lastBlockIncomplete=true`、block count mismatch 和 receipt count mismatch，并按 raw receipt MPT 校验 header `receiptsRoot`；对 empty/no-withdrawal block，如果父块已在本地 canonical runtime 中保留，还会要求子块 `stateRoot` 等于父块 `stateRoot`，不一致会拒绝本轮 import ready；全部通过才把 raw receipts 落到 native receipt snapshot、更新 canonical block receipt/stateRoot readiness，并把本轮 peer sync 标记 ready。State phase 遇到 eth/70+snap/1 peer 时，会按 geth capability offset 发出 `GetAccountRange`，root 使用 native head `stateRoot`，收到匹配 request_id 的 `AccountRange` 才记录 snap response evidence；非空 slim account 会被解析出 storage root / code hash，并继续发出 `GetStorageRanges` 和 `GetByteCodes`，匹配响应会落入 native snap account/storage/code cache，bytecode 在缓存前按 codeHash 校验；已协商 snap/1 peer 入站请求 `GetStorageRanges`、`GetByteCodes`、`GetTrieNodes` 时，会得到 request_id 保持一致的协议合法空响应。`novovm-node` 已有直接产品入口 `NOVOVM_NODE_MODE=eth_rlpx_sync`，可不经临时脚本启动 native Ethereum RLPx worker；有限主网 run 已观察到 Status -> Headers -> Bodies -> Receipts，native current 从 0 推进到 5120；入口默认以 geth mainnet bootnodes 为基础，并通过 geth DNS discovery root 解析 ENR 扩展候选池，`NOVOVM_ETH_RLPX_MAX_PEERS` 只限制活跃并发，`NOVOVM_ETH_RLPX_CANDIDATE_PEERS` 控制候选池。runtime sync status 已保留短期 remote-best hint，断线/peer unregister 后不会立刻把 `highest` 压回本地 current，避免长期追高中丢失远端目标；EOF/remote-closed 的 RLPx stream 不再留在 session map 里阻塞后续请求，而是清理 session 并允许重新 bootstrap 后继续同步；`NOVOVM_ETH_RLPX_CHECKPOINT_ENABLED` 默认会让产品入口写回 current/highest/header checkpoint，重启时先恢复追高位置。`NewPooledTransactionHashes` 会触发 `GetPooledTransactions` 并 materialize raw tx payload，本地 pending raw tx 也能响应远端 `GetPooledTransactions`。本地响应 `GetReceipts` 时会优先回放已验证 native receipt snapshot；只有能证明空交易 body 时才返回空 receipts，不伪造缺失 receipt 数据。这仍不是完整 geth peer 行为；DNS tree signature verification、完整 discv4 peer churn、proof/root verification、完整历史 receipt store、完整 snap state heal/download/store、完整 state root execution validation、完整 block/state/receipt durable store、eth/71 完整协商和长稳主网 soak 仍未封口。

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

### 8. Gateway JSON-RPC pending/runtime smoke

命令：

```powershell
cargo test -p novovm-evm-gateway json_rpc_pending_runtime_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 覆盖 runtime pending txpool snapshot
- 覆盖 `eth_pendingTransactions`
- 覆盖 pending `eth_getBlockByNumber`
- 覆盖 pending `eth_getBlockByHash`
- 覆盖 pending `eth_getTransactionByHash`
- 覆盖 pending `eth_getBlockReceipts`
- 覆盖 pending `eth_getTransactionReceipt`
- 覆盖 pending logs/filter changes
- 覆盖 confirmed index 优先于 runtime pending snapshot

这证明 gateway 层 pending/runtime 读面有独立回归门禁；它证明的是 Novo runtime pending view 的产品行为，不等同于完整 geth txpool replacement/eviction 策略。

### 9. Gateway JSON-RPC store recovery smoke

命令：

```powershell
cargo test -p novovm-evm-gateway json_rpc_store_recovery_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 覆盖 block filter changes 从 store 恢复
- 覆盖 tx/receipt confirmed position 从 store block index 恢复
- 覆盖 `eth_getBlockReceipts` 从 store 恢复
- 覆盖 `eth_feeHistory` 从 store block usage 恢复
- 覆盖 block number/hash 查询从 store 恢复
- 覆盖 logs/filter logs 从 store block/hash index 恢复

这证明 gateway 层在内存 scan window 被截断时，仍能从持久化索引恢复常用 JSON-RPC 读取面；这不是完整以太坊历史归档节点声明。

### 10. Gateway raw tx 写入面 smoke

命令：

```powershell
cargo test -p novovm-evm-gateway raw_tx_gateway_write_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 覆盖 `eth_sendTransaction` pending nonce view
- 覆盖 type2 dynamic-fee 推断、canonical fee hash/index 和 fee reject
- 覆盖 recoverable signature sender/nonce mismatch reject
- 覆盖 type1 access-list 推断和 access-list intrinsic gas reject
- 覆盖 type3 显式开关写入和 Cancun fork gate
- 覆盖 `eth_sendRawTransaction` UCA binding owner、execution policy、explicit chain/tx-type mismatch reject
- 覆盖 raw tx intrinsic gas、Prague calldata floor gas、London/type2 gate、Cancun/type3 gate

这证明 gateway raw 写入面具备独立产品门禁；它证明的是 Novo 控制面如何接收和拒绝 raw/typed transaction，不等同于完整 geth txpool 或完整 Ethereum transaction pool 行为。

### 11. Gateway txpool 错误面 smoke

命令：

```powershell
cargo test -p novovm-evm-gateway raw_tx_gateway_txpool_error_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 覆盖 replacement underpriced、nonce too low、nonce too high、pool full 的 gateway error code
- 覆盖 geth-style txpool error message
- 覆盖 structured txpool reject data
- 覆盖 reject reason 优先级高于 counters

这证明 gateway 能把 plugin/runtime txpool reject 转成稳定 JSON-RPC 产品错误面；这仍不是完整 geth txpool policy 等价声明。

### 12. EVM plugin txpool / fee settlement smoke

命令：

```powershell
cargo test -p novovm-adapter-evm-plugin txpool_replacement_and_reject_surface_smoke_v1 -- --nocapture
cargo test -p novovm-adapter-evm-plugin fee_settlement_ingress_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 覆盖 txpool replacement price bump reject/accept
- 覆盖 duplicate tx idempotent accept
- 覆盖 nonce gap、per-sender pending cap、contiguous executable nonce sequence
- 覆盖 pending sender bucket snapshot、pending drain、sender round-robin drain
- 覆盖 tx hash eviction、stale frame eviction
- 覆盖 runtime tap reject reason summary
- 覆盖 ingress frame、settlement record、payout instruction 和 fee reserve/payout totals

这证明实际 EVM plugin 层已具备 txpool replacement/reject 和 fee settlement 的回归门禁；账户余额扣费和 storage warmup 仍需在 adapter 执行语义层继续补强。

### 13. Adapter balance / fee / access-storage smoke

命令：

```powershell
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
cargo test -p novovm-adapter-novovm -- --nocapture
cargo test -p novovm-adapter-evm-plugin -- --nocapture
```

结果：

- pass
- 覆盖 tracked sender 成功 transfer 后扣 `value + gas_used * effective_gas_price`
- 覆盖 tracked sender 失败 contract call 后只扣 fee、不转 value
- 覆盖 tracked sender 成功 contract call 后扣 `value + fee`，target 增加 `value`，sender/target post-balance 写入 observed BAL
- 覆盖 tracked sender 成功 contract deploy 后扣 `value + fee`，contract 增加 `value`，contract balance/code 写入 observed BAL
- 覆盖 tracked sender 失败 contract deploy 后只扣 fee、不转 value、不创建 contract account、不产出 contract BAL entry
- 覆盖 CREATE existing-account collision：artifact 声称成功但目标已有 nonce/code/storage 时，降级失败、只扣 fee、不覆盖原 account/code/storage、不产出 contract BAL entry
- 覆盖 CREATE2 artifact collision：artifact 携带 CREATE2 派生地址且目标已存在时，同样降级失败、只扣 fee、不覆盖原 account/code/storage、不产出 contract BAL entry
- 覆盖 CREATE fallback contract address：无 artifact contract address 时，adapter 使用 geth `crypto.CreateAddress(sender, nonce)` / `keccak256(rlp([sender, nonce]))[12:]` 地址派生
- 覆盖 EIP-1559 `effectiveGasPrice` fee settlement：当 `effectiveGasPrice < max_fee/gas_price` 时，sender fee debit 使用 `effectiveGasPrice`，不按 max fee cap 多扣
- 覆盖余额不足时拒绝执行且不推进 nonce
- 覆盖 sender post-balance 写入 observed BAL balance change
- 覆盖 type1 access-list intrinsic gas extras
- 覆盖 raw type1 access-list address/storage key 贯通到 TxIR
- 覆盖 raw type1 access-list declared storage read 写入 observed BAL
- 覆盖 access-list warm storage read、SLOAD accessed_storage_keys 顺序语义和 adapter observed BAL
- 覆盖 SLOAD warm/cold fee debit：access-list 初始 warm slot、重复 SLOAD warm、未声明 slot cold->warm sequence 的 gas 进入 sender fee debit
- 覆盖 EIP-3529 SSTORE clear refund `4800`、refund cap `1/5`、SSTORE clean/dirty transition gas/refund delta 和 adapter post-refund gas fee debit
- 覆盖 EIP-3529 refund cap fee debit：当 refund counter 超过 `gas_used / 5` 时，sender fee debit 使用 cap 后 gas，不使用 uncapped over-refund gas
- 覆盖 contract call storage write 的 observed BAL
- 覆盖 contract deploy code/storage/balance observed BAL
- 覆盖 native adapter smoke 显式 funded sender
- 覆盖 EVM plugin 全包回归，确认 adapter 扣费语义未破坏 plugin apply/metadata 主线

这证明 adapter 在拿到 sender account pre-state 时，会执行生产级 value/fee debit、EIP-1559 effectiveGasPrice fee settlement、成功 value transfer、失败 fee-only debit、CREATE geth 地址派生、CREATE/CREATE2 existing-account collision 拒绝和余额不足拒绝；没有 sender account pre-state 的 plugin smoke 仍保持控制面执行，不伪造余额。当前已贯通 raw / gateway access-list entries 到 TxIR，并能把 declared storage read 写入 observed BAL；已补最小 warm/cold 成本、SLOAD accessed_storage_keys 顺序语义和 warm/cold fee debit、EIP-3529 refund/cap fee debit、SSTORE transition、CREATE/CALL failure invariant、CREATE/CREATE2 collision invariant、CREATE/CREATE2 address derivation、账户余额 value/fee invariant、effectiveGasPrice settlement、官方 EIP-1559 sender balance state fixture 子集、官方 SLOAD warm/cold state fixture 子集和 BAL 执行观测门禁，但仍未跑 Ethereum execution-spec 官方 refund/account 全量 fixture，因此不声明完整 EVM gas/refund/account/fee 等价。

### 14. Access-list entries 贯通 smoke

命令：

```powershell
cargo test -p novovm-adapter-evm-core translate_type1_fields_extracts_access_list_intrinsic_counts -- --nocapture
cargo test -p novovm-adapter-novovm execute_raw_type1_access_list_emits_declared_storage_reads_v1 -- --nocapture
cargo test -p novovm-evm-gateway eth_send_transaction_infers_type1_from_access_list -- --nocapture
```

结果：

- pass
- core 从 type1 raw RLP 解析具体 access-list address 和 storage keys
- `tx_ir_from_raw_fields_m0` 将 access-list entries 写入 `TxIR.evm_access_list`
- adapter observed BAL 为 declared access-list account 写入 `account_read`
- adapter observed BAL 为 declared access-list storage key 写入 `storage_read`
- gateway JSON-RPC `accessList` parser 保留具体 address/storage keys，并继续驱动 type1 推断和 intrinsic gas gate

这证明 access-list 不再只是 count-only gas 输入，已经进入 Novo EVM 插件的执行/观测数据面；warm/cold 的最小成本和 BAL 观测 smoke 已补，下一步若要继续提高语义置信度，应接入官方 fixture，而不是继续增加包装层。

### 15. Execution-spec access-list warm/cold smoke

命令：

```powershell
cargo test -p novovm-adapter-evm-core access_list_warm_storage_read_reduces_execution_gas_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core access_list_warm_account_access_reduces_execution_gas_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core sload_sequence_reuses_warm_storage_key_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core sload_sequence_respects_access_list_initial_warm_set_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core sload_sequence_keeps_address_and_slot_in_access_key_m0 -- --nocapture
cargo test -p novovm-adapter-novovm evm_execution_spec_access_list_warm_storage_smoke_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_execution_spec_sload_warm_cold_fee_debit_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
cargo test -p novovm-adapter-evm-core -- --nocapture
cargo test -p novovm-adapter-novovm -- --nocapture
cargo test -p novovm-adapter-evm-plugin -- --nocapture
```

结果：

- pass
- core 固化 EIP-2929 风格 cold account access `2600`、cold SLOAD `2100`、warm access/storage read `100`
- core 固化 access-list address 执行侧节省 `2500`，storage key 执行侧节省 `2000`
- core 固化单交易内 `(address, storageKey)` accessed_storage_keys 顺序语义：首次 cold、重复 warm、access-list initial warm、不同 address 不共享 slot warmth
- adapter 使用真实 `TxIR.evm_access_list` 执行 contract call，observed BAL 同时保留 declared warm storage read 和实际 contract call storage write，并用 SLOAD sequence 模型验证 declared slot 首读 warm
- adapter 使用 SLOAD sequence 模型的 `gas_used` 扣 sender fee，确认 access-list warm slot 不被按 all-cold 或 count-only cold overcharge
- core/adapter/plugin full package tests pass

这证明 Novo EVM 插件当前不是只记录 access-list intrinsic gas；它已经具备最小 warm/cold 成本模型、SLOAD accessed_storage_keys 顺序语义、warm/cold fee debit 和执行观测门禁。该门禁仍不是 opcode 级 geth EVM，也不是 Ethereum execution-spec 官方 fixture 全量通过。

### 16. Execution-spec SSTORE refund / transition smoke

依据本机最新 `D:\WEB3_AI\go-ethereum` 规则，当前 London/EIP-3529 后 `SstoreClearsScheduleRefundEIP3529 = 5000 - 2100 + 1900 = 4800`，refund cap 为 `gas_used / 5`。

命令：

```powershell
cargo test -p novovm-adapter-evm-core sstore_clear_refund_matches_eip3529_schedule_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core eip3529_refund_cap_limits_refunded_gas_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core sstore_transition_clean_slots_match_eip3529_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core sstore_transition_dirty_slots_match_eip3529_m0 -- --nocapture
cargo test -p novovm-adapter-novovm execute_success_call_debits_refunded_sstore_gas_used_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_execution_spec_sstore_refund_cap_fee_debit_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
cargo test -p novovm-adapter-evm-core -- --nocapture
cargo test -p novovm-adapter-novovm -- --nocapture
cargo test -p novovm-adapter-evm-plugin -- --nocapture
```

结果：

- pass
- core 固化 EIP-3529 SSTORE clear refund `4800`
- core 固化 refund cap `gas_used / 5`
- core 固化 post-refund gas 不低于 floor gas
- core 固化 SSTORE sentry `2300`、clean zero->nonzero `22100` cold gas、clean nonzero->zero `5000` cold gas + `4800` refund
- core 固化 dirty slot recreate `-4800` refund delta、dirty delete `+4800` refund delta、reset original existing `+2800`、reset original zero `+19900`
- adapter 对成功 contract call 使用 core SSTORE transition 推导出的 artifact post-refund `gas_used` 扣 fee，确认 refund 影响实际 sender fee debit
- adapter 对 refund counter 超过 `gas_used / 5` 的样本使用 cap 后 `gas_used` 扣 fee，确认不会按 uncapped refund over-credit sender
- core/adapter/plugin full package tests pass

这证明当前产品面已经能处理 post-refund gas fee settlement，并把 SSTORE clear refund、refund cap、clean/dirty transition 的关键数值锁进 core gate。它仍不是 opcode 级 SSTORE 执行器全量实现，后续若要声明完整等价，需要接 Ethereum execution-spec 官方 SSTORE fixture。

### 17. Execution-spec CREATE/CALL failure invariants smoke

命令：

```powershell
cargo test -p novovm-adapter-novovm evm_execution_spec_create_call_failure_state_invariants_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_execution_spec_create_existing_account_collision_invariants_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_equivalence_baseline_matrix_receipt_revert_gas_v1 -- --nocapture
```

结果：

- pass
- failed CALL 不提交 value transfer
- failed CALL 不写 target storage，且不产生 event logs / log bloom
- failed CALL 的 target BAL account entry 不包含 storage changes
- failed CREATE 即使 artifact 携带 `contract_address` / `runtime_code` / `runtime_code_hash`，resolved artifact 也会清空 contract/runtime 字段
- failed CREATE 不创建 contract account，不写 deploy/runtime storage，不产出 contract BAL account entry
- CREATE existing-account collision 即使 artifact 声称成功，也会降级为 failed execution
- CREATE existing-account collision 不转 value、不覆盖 existing contract account/code/storage，不产出 contract BAL account entry
- CREATE2 artifact collision 复用相同状态不变式：不转 value、不覆盖 existing contract account/code/storage，不产出 contract BAL account entry
- failure classification 仍落到 sender 侧 metadata，不伪造成合约状态变更

这证明当前 adapter 产品面已经把 CREATE/CALL 失败路径和 CREATE/CREATE2 existing-account collision 的状态不变式锁住。它仍不是官方 execution-spec 全量 fixture；后续要声明等价，需要把这些 invariant 接入官方 failure/account fixture 子集。

### 18. Execution-spec account balance value/fee invariants smoke

命令：

```powershell
cargo test -p novovm-adapter-novovm evm_execution_spec_account_balance_value_fee_invariants_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_equivalence_baseline_matrix_receipt_revert_gas_v1 -- --nocapture
```

结果：

- pass
- successful CALL: sender 扣 `value + gas_used * effective_gas_price`，target 增加 `value`，sender/target post-balance 进入 observed BAL
- successful CREATE: sender 扣 `value + gas_used * effective_gas_price`，contract 增加 `value`，contract balance/code 进入 observed BAL
- failed CREATE: sender 只扣 fee，不转 `value`，不创建 contract account，不产出 contract BAL entry
- 该门禁挂入 adapter balance/fee 聚合 smoke 和 baseline matrix

这证明当前 adapter 产品面已经把账户余额 value/fee 的核心不变式锁住，覆盖 CALL/CREATE 成功路径和 CREATE 失败路径。它仍不是官方 execution-spec 全量 account fixture；后续要声明完整等价，需要把这些 invariant 接入官方 state/account fixture 子集。

### 19. Execution-spec EIP-1559 effectiveGasPrice fee settlement smoke

命令：

```powershell
cargo test -p novovm-adapter-novovm evm_execution_spec_effective_gas_price_fee_debit_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_equivalence_baseline_matrix_receipt_revert_gas_v1 -- --nocapture
```

结果：

- pass
- 当 tx `gas_price/max_fee = 9` 且 artifact `effective_gas_price = 3` 时，sender fee debit 使用 `gas_used * 3`
- sender post-balance 写入 observed BAL
- resolved execution artifact 保留 `effective_gas_price = 3`
- 该门禁挂入 adapter balance/fee 聚合 smoke 和 baseline matrix

这证明当前 adapter fee settlement 使用 geth receipt 面的 `effectiveGasPrice`，不会在 EIP-1559 动态费交易上按 max fee cap 多扣。它仍不是官方 fee market fixture 全量；后续要声明完整等价，需要接入官方 EIP-1559 fee fixture 子集。

### 20. Execution-spec SSTORE refund cap fee debit smoke

命令：

```powershell
cargo test -p novovm-adapter-novovm evm_execution_spec_sstore_refund_cap_fee_debit_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_equivalence_baseline_matrix_receipt_revert_gas_v1 -- --nocapture
```

结果：

- pass
- pre-refund gas `24000`、refund counter `19200` 时，EIP-3529 cap 后 gas 为 `19200`
- uncapped over-refund gas 会是 `4800`，该值不会用于 sender fee debit
- sender post-balance 写入 observed BAL
- 该门禁挂入 adapter balance/fee 聚合 smoke 和 baseline matrix

这证明当前 adapter fee settlement 已经把 EIP-3529 refund cap 后的 gas 用到实际 sender fee debit，避免 over-refund。它仍不是官方 SSTORE opcode fixture 全量；后续要声明完整等价，需要接入官方 SSTORE/refund fixture 子集。

### 21. Execution-spec SLOAD warm/cold fee debit smoke

命令：

```powershell
cargo test -p novovm-adapter-novovm evm_execution_spec_sload_warm_cold_fee_debit_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_equivalence_baseline_matrix_receipt_revert_gas_v1 -- --nocapture
```

结果：

- pass
- access-list 初始 warm slot + 重复 SLOAD + 未声明 slot cold->warm sequence 的执行 gas 为 `2400`
- 同一 sequence 若不使用初始 warm set 为 `4400`
- count-only all-cold SLOAD gas 为 `8400`
- sender fee debit 使用 `2400` sequence gas 对应的 `gas_used`，不按 all-cold 或 count-only cold overcharge
- sender post-balance 和 declared warm storage read 写入 observed BAL
- 该门禁挂入 adapter balance/fee 聚合 smoke 和 baseline matrix

这证明当前 adapter fee settlement 已把 EIP-2929 SLOAD warm/cold sequence gas 进入实际 sender fee debit。它仍不是官方 SLOAD opcode fixture 全量；后续要声明完整等价，需要接入官方 access-list/SLOAD warm-cold fixture 子集。

### 22. Execution-spec CREATE existing-account collision smoke

命令：

```powershell
cargo test -p novovm-adapter-novovm evm_execution_spec_create_existing_account_collision_invariants_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_execution_spec_create_call_failure_state_invariants_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_equivalence_baseline_matrix_receipt_revert_gas_v1 -- --nocapture
```

结果：

- pass
- artifact 声称 CREATE 成功且携带 `contract_address` / `runtime_code` / `runtime_code_hash` 时，如果目标地址已有 nonce/code/storage，resolved artifact 降级为失败并清空 contract/runtime 字段
- sender 只扣 fee，不转 `value`
- existing contract account balance / nonce / code_hash 保持不变
- existing contract runtime code/hash storage 保持不变
- 不产出 contract BAL account entry
- adapter internal state 同步保留 existing contract pre-state，不与外部 runtime state 分叉

这证明当前 adapter 产品面已经锁住 CREATE existing-account collision，不会让 AOEM 成功 artifact 覆盖既有合约账户。它仍不是官方 CREATE/account fixture 全量；后续要声明完整等价，需要接入官方 CREATE collision / account-state fixture 子集。

### 23. Official geth CREATE address fixture subset

依据本机 `D:\WEB3_AI\go-ethereum\crypto\crypto.go`，geth `CreateAddress` 规则为 `keccak256(rlp([sender, nonce]))[12:]`。

命令：

```powershell
cargo test -p novovm-adapter-evm-core derive_create_contract_address_matches_geth_vectors_m0 -- --nocapture
cargo test -p novovm-adapter-novovm evm_execution_spec_create_address_derivation_matches_geth_v1 -- --nocapture
cargo test -p novovm-adapter-novovm execute_transaction_with_observed_metadata_emits_complete_contract_deploy_evm_bal -- --nocapture
cargo test -p novovm-adapter-novovm evm_equivalence_baseline_matrix_receipt_revert_gas_v1 -- --nocapture
```

结果：

- pass
- core 从 `crates/plugins/evm/core/tests/fixtures/ethereum-official-address-subset.json` 读取 geth 官方 `crypto/crypto_test.go::TestNewContractAddress` fixture 子集
- sender `970e8128ab834e8eac17ab8e3812f010678cf791`，nonce `0/1/2` 分别派生 `333c3310824b7c685133f2bedb2ca4b8b4df633d`、`8bda78331c916a08481428e4b07c96d3e916d165`、`c9ddedf451bc62ce88bf9292afb13df35b670699`
- adapter fallback `derive_contract_address` 使用 core geth-equivalent helper
- 无 artifact contract deploy 的 resolved artifact / state / BAL 使用同一个 Ethereum CREATE 派生地址
- 该门禁挂入 adapter balance/fee 聚合 smoke 和 baseline matrix

这证明当前 adapter 在没有 AOEM artifact contract address 时，不再使用 NovoVM 自定义 Sha256 地址派生，而是使用 geth CREATE 地址规则。它仍不是 CREATE2 opcode fixture；后续若要继续推进，应补 CREATE2 碰撞/执行边界。

### 24. Official geth CREATE2 address fixture subset

依据本机 `D:\WEB3_AI\go-ethereum\crypto\crypto.go`，geth `CreateAddress2` 规则为 `keccak256(0xff ++ address ++ salt ++ initCodeHash)[12:]`。

命令：

```powershell
cargo test -p novovm-adapter-evm-core derive_create2_contract_address_matches_geth_vectors_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core -- --nocapture
```

结果：

- pass
- core 从 `crates/plugins/evm/core/tests/fixtures/ethereum-official-address-subset.json` 读取 geth 官方 `core/vm/instructions_test.go::TestCreate2Addresses` 的 7 个固定向量
- 覆盖 zero address / zero salt / `0x00` init code
- 覆盖 nonzero origin、short salt left-pad、empty init code、long init code
- core 暴露 `derive_create2_contract_address_m0(from, salt, init_code_hash)`，按 geth `CreateAddress2` 规则派生地址

这证明当前 EVM core 地址语义已经锁住 CREATE2 地址派生公式。它仍不是 CREATE2 opcode 执行器，也未声明 CREATE2 state/account collision 全量等价；后续要声明完整等价，需要把 CREATE2 执行和碰撞样本接入官方 fixture 子集。

### 25. Execution-spec CREATE2 artifact collision smoke

命令：

```powershell
cargo test -p novovm-adapter-novovm evm_execution_spec_create2_artifact_collision_invariants_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_execution_spec_create_call_failure_state_invariants_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_equivalence_baseline_matrix_receipt_revert_gas_v1 -- --nocapture
```

结果：

- pass
- 使用 core `derive_create2_contract_address_m0` 生成 CREATE2 派生地址
- artifact 声称 deploy 成功并携带该 CREATE2 address 时，如果目标已有 nonce/code/storage，adapter 降级为失败
- sender 只扣 fee，不转 `value`
- existing contract balance / nonce / code_hash / runtime storage 保持不变
- 不产出 contract BAL account entry
- 该门禁挂入 CREATE/CALL failure invariant、adapter balance/fee 聚合 smoke 和 baseline matrix

这证明当前 adapter 主路径已经对 AOEM/host 传入的 CREATE2 派生地址执行 existing-account collision 保护，不会覆盖既有合约账户。它仍不是 CREATE2 opcode 执行器；后续要声明完整等价，需要接入官方 CREATE2 execution/collision fixture 子集。

### 26. Official geth address fixture subset

本次不引入通用 fixture runner，只把已有 CREATE/CREATE2 地址门禁改为直接读取官方 geth 向量子集：

- fixture: `crates/plugins/evm/core/tests/fixtures/ethereum-official-address-subset.json`
- source: `github.com/ethereum/go-ethereum`
- source cases: `crypto/crypto_test.go::TestNewContractAddress`
- source cases: `core/vm/instructions_test.go::TestCreate2Addresses`

命令：

```powershell
cargo test -p novovm-adapter-evm-core derive_create_contract_address_matches_geth_vectors_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core derive_create2_contract_address_matches_geth_vectors_m0 -- --nocapture
```

结果：

- pass
- CREATE 覆盖 sender + nonce `0/1/2`
- CREATE2 覆盖 zero/nonzero origin、zero/short salt、empty/short/long init code
- 现有 core 地址测试不再在 Rust 测试体内硬编码向量，而是消费官方 fixture 子集

这证明 EVM core 地址派生已经开始接官方 fixture 子集，而不是继续堆内部 smoke。该 fixture 只覆盖地址公式，不覆盖 opcode execution、state transition、account collision 全量语义；下一步应接 Ethereum execution-spec state fixture 子集。

### 27. Official state fixture subset: EIP-1559 sender balance

本次不引入通用 state-test runner，只接入一份官方 GeneralStateTests state fixture 子集，验证和当前产品主线直接相关的 EIP-1559 sender balance / fee debit：

- fixture: `crates/novovm-adapter-novovm/tests/fixtures/ethereum-official-state-subset/senderBalance.json`
- source archive: `ethereum/tests` `fixtures_general_state_tests.tgz`
- source case: `GeneralStateTests/stEIP1559/senderBalance.json::senderBalance-fork_[Cancun-Prague]-d0g0v0`
- source filler: `src/GeneralStateTestsFiller/stEIP1559/senderBalanceFiller.yml`

命令：

```powershell
cargo test -p novovm-adapter-novovm official_state_fixture_eip1559_sender_balance_fee_debit_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 使用 fixture `txbytes` 走 raw EVM sender recovery 和 adapter verify，不绕过生产验证路径
- fixture 中 `baseFee=0x0b`、`maxPriorityFeePerGas=0x64`、`maxFeePerGas=0x03e8`，有效价格为 `111`
- fixture 证明执行中 `BALANCE(sender)` 使用 `preBalance - gasLimit * effectiveGasPrice`，不是 `maxFeePerGas`
- adapter settlement 使用 official fixture 的 `gasUsed=43205` 和 `effectiveGasPrice=111`，sender post balance 对齐 fixture
- BAL sender post balance 对齐 fixture

这证明当前 SUPERVM EVM adapter 的 EIP-1559 fee settlement 已开始消费官方 state fixture 子集，并且验证了 raw tx recovery -> TxIR -> adapter verify -> execution artifact settlement 的产品路径。该门禁仍不是 opcode 级 state-test runner；fixture 中合约代码写 storage 的完整 EVM 执行仍由外部 AOEM/host artifact 承载。

### 28. Official state fixture subset: SLOAD warm/cold

本次不引入通用 state-test runner，只接入官方 `storageCosts` 中最小 warm/cold 对照 case：

- fixture: `crates/novovm-adapter-novovm/tests/fixtures/ethereum-official-state-subset/storageCosts-warm-cold.json`
- source archive: `ethereum/tests` `fixtures_general_state_tests.tgz`
- source case: `GeneralStateTests/stEIP2930/storageCosts.json::storageCosts-fork_[Cancun-Prague]-d[0-35]g0v0`
- source filler: `src/GeneralStateTestsFiller/stEIP2930/storageCostsFiller.yml`
- selected labels: `declaredKeyRead` / `undeclaredKeyRead`

命令：

```powershell
cargo test -p novovm-adapter-novovm official_state_fixture_sload_warm_cold_fee_debit_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 使用 fixture `txbytes` 走 type-1 raw EVM sender recovery、access-list decode 和 adapter verify，不绕过生产验证路径
- `declaredKeyRead` post-state 推导 `gasUsed=116377`
- `undeclaredKeyRead` post-state 推导 `gasUsed=118377`
- cold - warm = `2000` gas，对齐 EIP-2929 SLOAD warm/cold delta
- adapter 按 fixture 推导的 `gasUsed` 和 `gasPrice=10` 进行 sender fee debit，并对齐 official post sender balance
- BAL sender post balance 对齐 fixture

这证明当前 SUPERVM EVM adapter 已开始消费官方 SLOAD warm/cold state fixture 子集，并覆盖 raw type-1 access-list -> TxIR -> adapter verify -> artifact fee settlement 的产品路径。该门禁仍不是 opcode 级 state-test runner；fixture 中合约执行产生的完整 storage writes 仍由外部 AOEM/host artifact 承载。

### 29. Official state fixture subset: SSTORE refund cap / store clear

本次不引入通用 state-test runner，而是一次性接入官方 SSTORE/refund 相关 grouped 子集：

- fixture: `crates/novovm-adapter-novovm/tests/fixtures/ethereum-official-state-subset/sstore-refund-cap.json`
- source archive: `ethereum/tests` `fixtures_general_state_tests.tgz`
- selected sources: `stRefundTest/refundMax.json`、`stRefundTest/refund50percentCap.json`、`stRefundTest/refundSSTORE.json`、`stSStoreTest/sstoreGas.json`、`stTransactionTest/*StoreClears*Success.json`
- selected labels: `refundMax`、`refund50percentCap`、`refundSSTORE`、`sstoreGas`、`ContractStoreClearsSuccess`、`InternalCallStoreClearsSuccess`、`StoreClearsAndInternalCallStoreClearsSuccess`

命令：

```powershell
cargo test -p novovm-adapter-novovm official_state_fixture_sstore_refund_cap_fee_debit_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 使用 fixture `txbytes` 走 raw EVM sender recovery、field decode 和 adapter verify，不绕过生产验证路径
- `refundMax gasUsed=48842`，`refund50percentCap gasUsed=76336`，`refundSSTORE gasUsed=21210`
- `sstoreGas gasUsed=225910`，并保留官方 post storage 中 `0x1006=0x5654`、`0x1007=0x0898`、`0x1008=0x4e20`
- StoreClears 三组成功路径 gas 排序保持：`80324 > 64305 > 56848`
- adapter 按 official `gasUsed/gasPrice` 对 sender 做 fee debit，sender post balance 和 BAL sender post balance 对齐 fixture

这证明当前 SUPERVM EVM adapter 已消费官方 SSTORE refund/cap state fixture 子集，并把 raw tx -> TxIR -> adapter verify -> artifact fee settlement -> BAL sender post 的产品路径锁住。该门禁仍不是 opcode 级 state-test runner；fixture 中合约执行产生的完整 storage/internal balance transition 仍由外部 AOEM/host artifact 承载。

### 30. Official state fixture subset: failure/account no-commit

本次不引入通用 state-test runner，而是一次性接入官方 failure/account grouped 子集：

- fixture: `crates/novovm-adapter-novovm/tests/fixtures/ethereum-official-state-subset/failure-account.json`
- source archive: `ethereum/tests` `fixtures_general_state_tests.tgz`
- selected sources: `stRevertTest/RevertOpcode.json`、`stRevertTest/RevertOpcodeInInit.json`、`stRevertTest/RevertDepthCreateAddressCollision.json`、`stRevertTest/RevertSubCallStorageOOG.json`、`stTransactionTest/CreateMessageReverted.json`、`stTransactionTest/ContractStoreClearsOOG.json`、`stTransactionTest/InternalCallHittingGasLimit.json`
- selected labels: `topLevelRevertOpcode`、`deployInitRevertOpcode`、`depthCreateAddressCollisionNoValueTransfer`、`subCallStorageOogNoCommit`、`createMessageRevertedNoValueTransfer`、`contractStoreClearsOogNoCommit`、`internalCallHittingGasLimitNoValueTransfer`

命令：

```powershell
cargo test -p novovm-adapter-novovm official_state_fixture_failure_account_fee_debit_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 使用 fixture `txbytes` 走 raw EVM sender recovery、field decode 和 adapter verify，不绕过生产验证路径
- 覆盖顶层 `REVERT`、CREATE init revert、create/call OOG、create address collision/value no-transfer、store-clear OOG no-commit
- adapter 按 official `gasUsed/gasPrice` 对 sender 做 fee debit，sender post balance 和 BAL sender post balance 对齐 fixture
- 对 value>0 且官方 post 未转账的 4 个 case，adapter 保持 target balance 不变
- 对 `ContractStoreClearsOOG`，adapter 保持 target storage 与 official post 一致，不提交失败路径 storage clear
- 对 failed deploy，adapter 不创建 contract account，也不产出 contract BAL entry

这证明当前 SUPERVM EVM adapter 已消费官方 failure/account state fixture 子集，并把 raw tx -> TxIR -> adapter verify -> failed artifact settlement -> no value/storage/account commit -> BAL sender post 的产品路径锁住。该门禁仍不是 opcode 级 state-test runner；fixture 中失败发生的具体 opcode 执行仍由外部 AOEM/host artifact 承载。

### 31. Official state fixture subset: CREATE/CREATE2 account grouped

本次不引入通用 state-test runner，而是一次性接入官方 CREATE/CREATE2/account grouped 子集：

- fixture: `crates/novovm-adapter-novovm/tests/fixtures/ethereum-official-state-subset/create-account.json`
- source archive: `ethereum/tests` `fixtures_general_state_tests.tgz`
- selected sources: `stTransactionTest/CreateTransactionSuccess.json`、`stCreateTest/CreateTransactionCallData.json`、`stCreateTest/TransactionCollisionToEmptyButCode.json`、`stCreateTest/TransactionCollisionToEmptyButNonce.json`、`stCreateTest/CREATE2_CallData.json`、`stCodeSizeLimit/create2CodeSizeLimit.json`、`stCodeSizeLimit/createCodeSizeLimit.json`、`stCreateTest/createLargeResult.json`、`stCreateTest/CreateResults.json`
- selected labels: `createTransactionSuccess`、`createTransactionCallData`、`transactionCollisionToEmptyButCode`、`transactionCollisionToEmptyButNonce`、`create2CallData`、`create2CodeSizeLimit`、`createCodeSizeLimit`、`createLargeResult`、`createResults`

命令：

```powershell
cargo test -p novovm-adapter-novovm official_state_fixture_create_account_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 使用 fixture `txbytes` 走 raw EVM sender recovery、field decode、TxIR 构建和 adapter verify，不绕过生产验证路径
- 覆盖 2 个 top-level CREATE success、2 个 top-level CREATE collision、5 个 internal CREATE/CREATE2 projection，其中 CREATE2 projection 2 个
- top-level CREATE success 对齐官方派生地址、contract balance、contract nonce `1`、runtime code storage 和 contract BAL entry
- top-level CREATE collision 对齐官方 sender fee debit，拒绝覆盖已有 code/nonce account，不创建 contract BAL entry
- internal CREATE/CREATE2/code-size/large-result case 只锁 raw tx、sender fee debit、target balance 和 BAL sender post；internal created account 仍属于 host/AOEM artifact 责任，不在 adapter 层伪造

这证明当前 SUPERVM EVM adapter 已消费官方 CREATE/CREATE2/account state fixture grouped 子集，并把 raw tx -> TxIR -> adapter verify -> top-level deploy/collision state projection -> BAL sender/contract post 的产品路径锁住。该门禁仍不是 opcode 级 state-test runner；内部 CREATE/CREATE2 的完整 opcode 执行、内部 account materialization 和 code-size 边界仍由外部 AOEM/host artifact 承载。

### 32. Official state fixture subset: STATICCALL / precompile / return-data grouped

本次不引入通用 state-test runner，而是一次性接入官方 STATICCALL/precompile/return-data grouped 子集：

- fixture: `crates/novovm-adapter-novovm/tests/fixtures/ethereum-official-state-subset/staticcall-precompile-return.json`
- source archive: `ethereum/tests` `fixtures_general_state_tests.tgz`
- selected sources: `stStaticCall/StaticcallToPrecompileFromTransaction.json`、`stStaticCall/StaticcallToPrecompileFromCalledContract.json`、`stStaticCall/static_CallSha256_1.json`、`stStaticCall/static_CallIdentity_2.json`、`stStaticCall/static_CallRipemd160_1.json`、`stStaticCall/static_CallEcrecover0.json`、`stStaticCall/static_ReturnTest2.json`、`stStaticCall/static_CallToReturn1.json`、`stStaticCall/static_callOutput3partial.json`
- selected labels: `staticcallPrecompileFromTransaction`、`staticcallPrecompileFromCalledContract`、`staticCallSha256`、`staticCallIdentity`、`staticCallRipemd160`、`staticCallEcrecover`、`staticReturnTest2`、`staticCallToReturn1`、`staticCallOutput3partial`

命令：

```powershell
cargo test -p novovm-adapter-novovm official_state_fixture_staticcall_precompile_return_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 使用 fixture `txbytes` 走 raw EVM sender recovery、field decode、TxIR 构建和 adapter verify，不绕过生产验证路径
- 覆盖 6 个 STATICCALL/precompile projection、3 个 return/output projection，其中 precompile 包含 `ecrecover`、`sha256`、`ripemd160`、`identity`
- 修正 adapter 执行阶段 effective tx type：raw empty calldata `to` tx 在解码阶段仍可保持 state-agnostic `Transfer`，但当 runtime pre-state 证明目标账户有 code/runtime code 时，执行阶段按 `ContractCall` 应用 value、storage marker、receipt/BAL
- adapter 按 official `gasUsed/gasPrice` 对 sender 做 fee debit，sender post balance、value transfer 后 target post balance 和 BAL post 对齐 fixture
- 官方 post storage facts 作为 AOEM/host projection 事实锁住，不声明 adapter 已执行完整 STATICCALL/precompile/return-data opcode storage transition
- 官方 `logsHash` 为 empty logs hash；本门禁验证 empty raw artifact bloom/logs 路径，不声明日志 body 等价

这证明当前 SUPERVM EVM adapter 已消费官方 STATICCALL/precompile/return-data state fixture grouped 子集，并补齐了 empty calldata 到 code target 的状态感知 contract-call 执行分类。该门禁仍不是 opcode 级 state-test runner；precompile output、return-data copy 和 opcode storage writes 的完整执行仍由外部 AOEM/host artifact 承载。

### 33. Official state fixture subset: LOG / receipt grouped

本次不引入通用 state-test runner，而是一次性接入官方 LOG/receipt grouped 子集：

- fixture: `crates/novovm-adapter-novovm/tests/fixtures/ethereum-official-state-subset/log-receipt.json`
- source archive: `ethereum/tests` `fixtures_general_state_tests.tgz`
- selected sources: `stArgsZeroOneBalance/log0NonConst.json`、`stArgsZeroOneBalance/log1NonConst.json`、`stArgsZeroOneBalance/log2NonConst.json`、`stArgsZeroOneBalance/log3NonConst.json`
- selected labels: `log0NonConstZeroValue`、`log1NonConstZeroValue`、`log2NonConstZeroValue`、`log3NonConstZeroValue`

命令：

```powershell
cargo test -p novovm-adapter-novovm official_state_fixture_log_receipt_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 使用 fixture `txbytes` 走 raw EVM sender recovery、field decode、TxIR 构建和 adapter verify，不绕过生产验证路径
- 覆盖 LOG0、LOG1、LOG2、LOG3 的 non-const zero-value projection，全部为 Cancun/Prague 官方 post
- 选择 value=0 子集，避免把内部 CALL/value flow 混进 adapter 层门禁；sender fee debit、target zero-value balance stability 和 BAL sender post 可直接对齐官方 fixture
- official gasUsed 分别为 `21581`、`22059`、`22537`、`23015`，topic 阶梯保持 `478`
- official `logsHash` 均为非 empty logs hash，且 4 个 case hash 不同；adapter 验证 AOEM event log、topic count、log bloom 和 `aoem:last_event_logs` carry
- 官方 fixture 不提供完整 log body；本门禁不声明 LOG opcode body 等价，只声明官方 logs hash 分类和 adapter receipt/log/bloom carry 产品路径

这证明当前 SUPERVM EVM adapter 已消费官方 LOG/receipt state fixture grouped 子集，并把 raw tx -> state-aware contract-call execution -> official sender fee -> AOEM log/bloom carry -> BAL sender post 的产品路径锁住。该门禁仍不是 opcode 级 state-test runner；LOG opcode body、topic/data 精确内容和 receipt log hash 计算仍由外部 AOEM/host artifact 承载。

### 34. Official state fixture subset: RETURNDATA grouped

本次不引入通用 state-test runner，而是一次性接入官方 RETURNDATASIZE/RETURNDATACOPY grouped 子集：

- fixture: `crates/novovm-adapter-novovm/tests/fixtures/ethereum-official-state-subset/return-data.json`
- source archive: `ethereum/tests` `fixtures_general_state_tests.tgz`
- selected sources: `stReturnDataTest/returndatasize_initial.json`、`stReturnDataTest/returndatasize_initial_zero_read.json`、`stReturnDataTest/returndatacopy_following_call.json`、`stReturnDataTest/returndatacopy_following_revert.json`、`stReturnDataTest/returndatacopy_after_successful_staticcall.json`、`stReturnDataTest/returndatasize_after_successful_staticcall.json`、`stReturnDataTest/returndatasize_after_failing_staticcall.json`
- selected labels: `returndatasizeInitial`、`returndatasizeInitialZeroRead`、`returndatacopyFollowingCall`、`returndatacopyFollowingRevert`、`returndatacopyAfterSuccessfulStaticcall`、`returndatasizeAfterSuccessfulStaticcall`、`returndatasizeAfterFailingStaticcall`

命令：

```powershell
cargo test -p novovm-adapter-novovm official_state_fixture_return_data_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 使用 fixture `txbytes` 走 raw EVM sender recovery、field decode、TxIR 构建和 adapter verify，不绕过生产验证路径
- 覆盖初始 RETURNDATASIZE、zero read、CALL 后 RETURNDATACOPY、REVERT 后 RETURNDATACOPY、STATICCALL 成功/失败后的 return buffer projection
- 选择 value=0 子集，避免把内部 CALL value-flow 混进 adapter 层门禁；sender fee debit、target zero-value balance stability 和 BAL sender post 可直接对齐官方 fixture
- official gasUsed 分别为 `21205`、`21224`、`28668`、`28668`、`28664`、`28643`、`83825`
- `returndatacopyFollowingCall` 与 `returndatacopyFollowingRevert` 官方 post storage fact 一致，证明 revert return data 在官方语义中同样可被后续 RETURNDATACOPY 消费
- 官方 `logsHash` 全部为 empty logs hash；本门禁验证 empty raw artifact bloom/logs 路径，不声明日志 body 等价
- 官方 pre/post storage facts 作为 AOEM/host projection 事实锁住，不声明 adapter 已执行完整 RETURNDATASIZE/RETURNDATACOPY opcode storage transition

这证明当前 SUPERVM EVM adapter 已消费官方 RETURNDATA state fixture grouped 子集，并把 raw tx -> state-aware contract-call execution -> official sender fee -> empty receipt/log path -> BAL sender post 的产品路径锁住。该门禁仍不是 opcode 级 state-test runner；return buffer、RETURNDATACOPY 写 storage 和内部 STATICCALL failure 的完整执行仍由外部 AOEM/host artifact 承载。

### 35. Official state fixture subset: LOG4 / OOG receipt grouped

本次不引入通用 state-test runner，而是一次性接入官方 VMTests LOG4 receipt/no-log grouped 子集：

- fixture: `crates/novovm-adapter-novovm/tests/fixtures/ethereum-official-state-subset/log4-oog-receipt.json`
- source archive: `ethereum/tests` `fixtures_general_state_tests.tgz`
- selected source: `VMTests/vmLogTest/log4.json`
- selected labels: `log4EmptyMem`、`log4MemSizeZero`、`log4NonEmptyMem`、`log4Log01`、`log4Log311`、`log4Caller`、`log4MaxTopic`、`log4Pc`、`log4MemStartTooHigh`、`log4MemSizeTooHigh`

命令：

```powershell
cargo test -p novovm-adapter-novovm official_state_fixture_log4_oog_receipt_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 使用 fixture `txbytes` 走 raw EVM sender recovery、field decode、TxIR 构建和 adapter verify，不绕过生产验证路径
- 覆盖 8 个 LOG4 receipt/log projection 和 2 个 no-log memory 边界 projection，全部为 Cancun/Prague 官方 post
- 选择 `VMTests/vmLogTest/log4.json`，避免 `stLogTests/log4_*` 里不适合 adapter 层的复杂内部 value-flow；本子集 target balance 是简单 `pre + value`
- official gasUsed 分别为 `30717`、`30741`、`30997`、`30749`、`30749`、`30996`、`30773`、`30745`、`78750373`、`78750373`
- official LOG4 成功 case 均为 4 topics；no-log memory boundary case 的 official `logsHash` 为 empty logs hash
- 官方 post storage facts 作为 AOEM/host projection 事实锁住，成功路径 slot `0x00 = 0x600d`，no-log 边界 slot `0x00 = 0x0bad`

这证明当前 SUPERVM EVM adapter 已消费官方 LOG4 receipt/no-log state fixture grouped 子集，并把 raw tx -> contract-call execution -> official sender fee/value transfer -> AOEM log/bloom carry 或 no-log path -> BAL sender/target post 的产品路径锁住。该门禁仍不是 opcode 级 state-test runner；LOG4 topic/data 精确 body、memory expansion 和 storage writes 的完整执行仍由外部 AOEM/host artifact 承载。

### 36. Official state fixture subset: precompile failure / OOG grouped

本次不引入通用 state-test runner，而是一次性接入官方 STATICCALL/precompile failure、low-gas 和 input-validation grouped 子集：

- fixture: `crates/novovm-adapter-novovm/tests/fixtures/ethereum-official-state-subset/precompile-failure-oog.json`
- source archive: `ethereum/tests` `fixtures_general_state_tests.tgz`
- selected sources: `stStaticCall/static_CallEcrecover0_NoGas.json`、`stStaticCall/static_CallEcrecover0_Gas2999.json`、`stStaticCall/static_CallEcrecover0_gas3000.json`、`stStaticCall/static_CallSha256_4_gas99.json`、`stStaticCall/static_CallIdentity_4_gas17.json`、`stStaticCall/static_CallIdentity_4_gas18.json`、`stStaticCall/static_CallRipemd160_4_gas719.json`、`stStaticCall/static_CallEcrecoverCheckLengthWrongV.json`、`stStaticCall/static_CallEcrecoverCheckLength.json`
- selected labels: `ecrecoverNoGas`、`ecrecoverGas2999`、`ecrecoverGas3000`、`sha256Gas99`、`identityGas17`、`identityGas18`、`ripemd160Gas719`、`ecrecoverCheckLengthWrongV`、`ecrecoverCheckLength`

命令：

```powershell
cargo test -p novovm-adapter-novovm official_state_fixture_precompile_failure_oog_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 使用 fixture `txbytes` 走 raw EVM sender recovery、field decode、TxIR 构建和 adapter verify，不绕过生产验证路径
- 覆盖 4 个 gas failure projection、3 个 low-gas/boundary success projection、2 个 ecrecover input-validation projection
- 选择 simple top-level value transfer 子集，target balance 全部为 `pre + value`，避免复杂内部 value-flow
- official gasUsed 分别为 `27963`、`30962`、`90663`、`65414`、`45459`、`65360`、`46161`、`90495`、`90495`
- ecrecover `2999` 与 no-gas gasUsed 差值为 `2999`，`3000` case 进入 success marker；identity `17/18` 和 ecrecover `wrongV/checkLength` 都有官方边界事实锁定
- 官方 `logsHash` 全部为 empty logs hash；本门禁验证 empty raw artifact bloom/logs 路径
- 官方 post storage facts 作为 AOEM/host projection 事实锁住，不声明 adapter 已执行完整 precompile opcode/storage transition

这证明当前 SUPERVM EVM adapter 已消费官方 precompile failure/OOG state fixture grouped 子集，并把 raw tx -> state-aware contract-call execution -> official sender fee/value transfer -> empty receipt/log path -> BAL sender/target post 的产品路径锁住。该门禁仍不是 opcode 级 state-test runner；precompile result、low-gas failure 和 ecrecover input validation 的完整执行仍由外部 AOEM/host artifact 承载。

### 37. Official state fixture subset: CALL output grouped

本次不引入通用 state-test runner，而是一次性接入官方 CALL output full/partial success/failure grouped 子集：

- fixture: `crates/novovm-adapter-novovm/tests/fixtures/ethereum-official-state-subset/call-output.json`
- source archive: `ethereum/tests` `fixtures_general_state_tests.tgz`
- selected sources: `stCallCreateCallCodeTest/callOutput1.json`、`stCallCreateCallCodeTest/callOutput2.json`、`stCallCreateCallCodeTest/callOutput3.json`、`stCallCreateCallCodeTest/callOutput3partial.json`、`stCallCreateCallCodeTest/callOutput3Fail.json`、`stCallCreateCallCodeTest/callOutput3partialFail.json`
- selected labels: `callOutput1`、`callOutput2`、`callOutput3`、`callOutput3partial`、`callOutput3Fail`、`callOutput3partialFail`

命令：

```powershell
cargo test -p novovm-adapter-novovm official_state_fixture_call_output_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 使用 fixture `txbytes` 走 raw EVM sender recovery、field decode、TxIR 构建和 adapter verify，不绕过生产验证路径
- 覆盖 4 个 CALL output success projection 和 2 个 CALL output failure/no nested-storage-commit projection
- 选择 simple top-level value transfer 子集，顶层 `value = 100000`，target balance 全部为 `pre + value`，避免复杂内部 value-flow
- 6 个官方 case 故意共享同一个 top-level `txbytes`，通过不同合约 pre-state 形成不同 official post hash；门禁锁住 state-aware empty-calldata contract-call 分类
- official gasUsed 成功 case 均为 `67856`，failure case 均为 `95744`
- 官方 `logsHash` 全部为 empty logs hash；本门禁验证 empty raw artifact bloom/logs 路径
- 官方 target storage slot `0x00` 输出事实一致；success case nested target slot `0x00 = 0x02`，failure case nested target storage 为空，作为 AOEM/host projection 事实锁住

这证明当前 SUPERVM EVM adapter 已消费官方 CALL output state fixture grouped 子集，并把 raw tx -> state-aware contract-call execution -> official sender fee/value transfer -> empty receipt/log path -> BAL sender/target post 的产品路径锁住。该门禁仍不是 opcode 级 state-test runner；CALL opcode、output memory copy 和 nested state transition 的完整执行仍由外部 AOEM/host artifact 承载。

### 38. Official state fixture subset: CALL high-value / OOG grouped

本次不引入通用 state-test runner，而是一次性接入官方 CALL high-value / OOG grouped 子集，并只选择顶层 value flow 可直接归因到 sender/target 的 post：

- fixture: `crates/novovm-adapter-novovm/tests/fixtures/ethereum-official-state-subset/call-high-value.json`
- source archive: `ethereum/tests` `fixtures_general_state_tests.tgz`
- selected sources: `stCallCreateCallCodeTest/callWithHighValue.json`、`stCallCreateCallCodeTest/callWithHighValueAndGasOOG.json`、`stCallCreateCallCodeTest/callWithHighValueAndOOGatTxLevel.json`、`stCallCreateCallCodeTest/callWithHighValueOOGinCall.json`
- selected labels: `callWithHighValue`、`callWithHighValueAndGasOOGValue0`、`callWithHighValueAndOOGatTxLevelValue0`、`callWithHighValueOOGinCall`

命令：

```powershell
cargo test -p novovm-adapter-novovm official_state_fixture_call_high_value_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 使用 fixture `txbytes` 走 raw EVM sender recovery、field decode、TxIR 构建和 adapter verify，不绕过生产验证路径
- 覆盖 3 个 zero-value high-value/OOG projection 和 1 个 small top-level value transfer projection
- 选择 `value index 0` 子集，target balance 全部为 `pre + value`，明确排除 nested balance transfer 的复杂内部 value-flow post
- official gasUsed 分别为 `32530`、`52657`、`30524`、`64730`
- 官方 `logsHash` 全部为 empty logs hash；本门禁验证 empty raw artifact bloom/logs 路径
- 官方 target storage facts 锁住 high-value failure/OOG 结果：`callWithHighValueAndGasOOGValue0` 的 slot `0x01 = 0xffff...ffff`，`callWithHighValueOOGinCall` 的 slot `0x00 = 0x01`；nested balance/storage 均保持不提交

这证明当前 SUPERVM EVM adapter 已消费官方 CALL high-value/OOG state fixture grouped 子集，并把 raw tx -> state-aware contract-call execution -> official sender fee/value transfer -> empty receipt/log path -> BAL sender/target post 的产品路径锁住。该门禁仍不是 opcode 级 state-test runner；内部 CALL value-flow、OOG 执行和 storage transition 的完整执行仍由外部 AOEM/host artifact 承载。

### 39. Official state fixture subset: CALL depth / balance-too-low / OOG grouped

本次不引入通用 state-test runner，而是一次性接入官方 CALL depth、balance-too-low 和 OOG grouped 子集，并只选择顶层 value flow 可直接归因到 sender/target 的 post：

- fixture: `crates/novovm-adapter-novovm/tests/fixtures/ethereum-official-state-subset/call-depth-oog.json`
- source archive: `ethereum/tests` `fixtures_general_state_tests.tgz`
- selected sources: `stCallCreateCallCodeTest/Call1024BalanceTooLow.json`、`stCallCreateCallCodeTest/Call1024OOG.json`、`stCallCreateCallCodeTest/CallLoseGasOOG.json`
- selected labels: `Call1024BalanceTooLow`、`Call1024OOGGas0`、`Call1024OOGGas1`、`Call1024OOGGas2`、`Call1024OOGGas3`、`CallLoseGasOOG`

命令：

```powershell
cargo test -p novovm-adapter-novovm official_state_fixture_call_depth_oog_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 使用 fixture `txbytes` 走 raw EVM sender recovery、field decode、TxIR 构建和 adapter verify，不绕过生产验证路径
- 覆盖 1 个 CALL depth balance-too-low projection、4 个 CALL 1024 depth OOG gas variant projection、1 个 recursive lose-gas OOG projection
- 选择 simple top-level value transfer 子集，顶层 `value = 10`，target balance 全部为 `pre + value`，明确排除 `Call1024PreCalls` 这类内部账户净转移 post
- official gasUsed 分别为 `7481800`、`1751479`、`1716608`、`1748187`、`1745038`、`167771`
- 官方 `logsHash` 全部为 empty logs hash；本门禁验证 empty raw artifact bloom/logs 路径
- 官方 target storage facts 锁住 depth/OOG 结果：balance-too-low case slot `0x00 = 0x0401`、slot `0x01 = 0x01`；`Call1024OOG` gas variants 带 slot `0x00/0x01/0x02`；`CallLoseGasOOG` 带 slot `0x00 = 0x01`、slot `0x02 = 0x03e9`

这证明当前 SUPERVM EVM adapter 已消费官方 CALL depth/OOG state fixture grouped 子集，并把 raw tx -> state-aware contract-call execution -> official sender fee/value transfer -> empty receipt/log path -> BAL sender/target post 的产品路径锁住。该门禁仍不是 opcode 级 state-test runner；recursive CALL depth、OOG 执行和 storage transition 的完整执行仍由外部 AOEM/host artifact 承载。

### 40. Official state fixture subset: DELEGATECALL / CALLCODE account-context grouped

本次不引入通用 state-test runner，而是一次性接入官方 DELEGATECALL/CALLCODE account-context grouped 子集，并只选择顶层 value flow 可直接归因到 sender/target 的 post：

- fixture: `crates/novovm-adapter-novovm/tests/fixtures/ethereum-official-state-subset/delegatecall-callcode-context.json`
- source archive: `ethereum/tests` `fixtures_general_state_tests.tgz`
- selected sources: `stDelegatecallTestHomestead/delegatecallBasic.json`、`stDelegatecallTestHomestead/delegatecallSenderCheck.json`、`stDelegatecallTestHomestead/delegatecallValueCheck.json`、`stDelegatecallTestHomestead/delegatecallOOGinCall.json`、`stDelegatecallTestHomestead/callcodeOutput3.json`、`stDelegatecallTestHomestead/callcodeWithHighValueAndGasOOG.json`
- selected labels: `delegatecallBasic`、`delegatecallSenderCheck`、`delegatecallValueCheck`、`delegatecallOOGinCall`、`callcodeOutput3`、`callcodeWithHighValueAndGasOOG`

命令：

```powershell
cargo test -p novovm-adapter-novovm official_state_fixture_delegatecall_callcode_context_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 使用 fixture `txbytes` 走 raw EVM sender recovery、field decode、TxIR 构建和 adapter verify，不绕过生产验证路径
- 覆盖 4 个 DELEGATECALL account-context projection 和 2 个 CALLCODE context projection
- 选择 simple top-level value flow 子集；`delegatecallValueCheck` 顶层 `value = 0x17`，两个 CALLCODE case 顶层 `value = 100000`，target balance 全部为 `pre + value`
- official gasUsed 分别为 `67851`、`67832`、`67832`、`55727`、`45853`、`67869`
- 官方 `logsHash` 全部为 empty logs hash；本门禁验证 empty raw artifact bloom/logs 路径
- 官方 target storage facts 锁住 account-context：`delegatecallSenderCheck` slot `0x01 = sender`，`delegatecallValueCheck` slot `0x01 = 0x17`，CALLCODE output slot `0x00` 为官方 return-data word

这证明当前 SUPERVM EVM adapter 已消费官方 DELEGATECALL/CALLCODE account-context state fixture grouped 子集，并把 raw tx -> state-aware contract-call execution -> official sender fee/value transfer -> empty receipt/log path -> BAL sender/target post 的产品路径锁住。该门禁仍不是 opcode 级 state-test runner；DELEGATECALL/CALLCODE opcode account-context、output copy 和 storage transition 的完整执行仍由外部 AOEM/host artifact 承载。

### 41. Official state fixture subset: zero-value calls revert no-commit grouped

本次不引入通用 state-test runner，而是一次性接入官方 `stZeroCallsRevert` grouped 子集，覆盖零值 CALL/CALLCODE/DELEGATECALL/SUICIDE 在 OOG revert 下不提交账户/余额/storage 副作用：

- fixture: `crates/novovm-adapter-novovm/tests/fixtures/ethereum-official-state-subset/zero-calls-revert.json`
- source archive: `ethereum/tests` `fixtures_general_state_tests.tgz`
- selected sources: `stZeroCallsRevert/ZeroValue_CALL_*`、`stZeroCallsRevert/ZeroValue_CALLCODE_*`、`stZeroCallsRevert/ZeroValue_DELEGATECALL_*`、`stZeroCallsRevert/ZeroValue_SUICIDE_*`
- selected labels: 16 个 `ZeroValue_*_OOGRevert` Cancun/Prague projection case

命令：

```powershell
cargo test -p novovm-adapter-novovm official_state_fixture_zero_calls_revert_no_commit_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 使用 fixture `txbytes` 走 raw EVM sender recovery、field decode、TxIR 构建和 adapter verify，不绕过生产验证路径
- 覆盖 CALL、CALLCODE、DELEGATECALL、SUICIDE 各 4 个零值 OOG revert no-commit projection
- raw empty calldata tx 初始为 state-agnostic `Transfer`，目标账户带 runtime code pre-state 后提升为 `ContractCall`
- official gasUsed 分别锁住 `135000`、`100000`、`75000` 三类 full-gas failure debit
- 官方 `logsHash` 全部为 empty logs hash；本门禁验证 failed artifact empty bloom/logs 路径
- target 和 touched accounts 的 balance/nonce/storage 均保持官方 no-commit post；BAL 只要求 sender fee debit post，不为零值失败调用伪造 target balance change

这证明当前 SUPERVM EVM adapter 已消费官方 zero-value CALL/CALLCODE/DELEGATECALL/SUICIDE OOG revert no-commit state fixture grouped 子集，并把 raw tx -> state-aware contract-call execution -> official full-gas fee debit -> empty receipt/log path -> no target value/storage commit -> BAL sender post 的产品路径锁住。该门禁仍不是 opcode 级 state-test runner；内部调用、SELFDESTRUCT/OOG 执行和 storage transition 的完整执行仍由外部 AOEM/host artifact 承载。

### 42. Official state fixture subset: SELFDESTRUCT zero-value account preservation grouped

本次不引入通用 state-test runner，而是一次性接入官方 `stZeroCallsTest/ZeroValue_SUICIDE*` 成功侧 grouped 子集，覆盖 Cancun/Prague 下零值 SELFDESTRUCT/SUICIDE 成功后既有账户 code/storage 不消失：

- fixture: `crates/novovm-adapter-novovm/tests/fixtures/ethereum-official-state-subset/selfdestruct-zero-value.json`
- source archive: `ethereum/tests` `fixtures_general_state_tests.tgz`
- selected sources: `stZeroCallsTest/ZeroValue_SUICIDE.json`、`stZeroCallsTest/ZeroValue_SUICIDE_ToEmpty_Paris.json`、`stZeroCallsTest/ZeroValue_SUICIDE_ToNonZeroBalance.json`、`stZeroCallsTest/ZeroValue_SUICIDE_ToOneStorageKey_Paris.json`
- selected labels: `zeroValue_SUICIDE`、`zeroValue_SUICIDE_ToEmpty`、`zeroValue_SUICIDE_ToNonZeroBalance`、`zeroValue_SUICIDE_ToOneStorageKey`

命令：

```powershell
cargo test -p novovm-adapter-novovm official_state_fixture_selfdestruct_zero_value_account_preservation_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 使用 fixture `txbytes` 走 raw EVM sender recovery、field decode、TxIR 构建和 adapter verify，不绕过生产验证路径
- 覆盖 4 个 SELFDESTRUCT/SUICIDE zero-value success projection，全部为 Cancun/Prague 官方 post
- raw empty calldata tx 初始为 state-agnostic `Transfer`，目标账户带 runtime code pre-state 后提升为 `ContractCall`
- official gasUsed 全部为 `28603`，只扣 sender fee，不产生 target value BAL
- 官方 `logsHash` 全部为 empty logs hash；本门禁验证 success artifact empty bloom/logs 路径
- 官方 target/touched account 的 code hash、code bytes、balance、nonce、storage facts 全部保持 pre/post 一致；adapter 只校验产品面 account preservation，不声明执行 SELFDESTRUCT opcode

这证明当前 SUPERVM EVM adapter 已消费官方 SELFDESTRUCT/SUICIDE zero-value success account-preservation state fixture grouped 子集，并把 raw tx -> state-aware contract-call execution -> official fee debit -> empty receipt/log path -> target/touched account preservation -> BAL sender post 的产品路径锁住。该门禁仍不是 opcode 级 state-test runner；SELFDESTRUCT 的余额转移、删除规则和更复杂同交易创建/销毁语义仍由外部 AOEM/host artifact 承载。

### 43. Official state fixture subset: STATICCALL state-change / SUICIDE no-commit grouped

本次继续不引入通用 state-test runner，而是接入官方 `stStaticCall` 中 value=0、和当前插件产品面直接对齐的两个 STATICCALL/SUICIDE no-commit case：

- fixture: `crates/novovm-adapter-novovm/tests/fixtures/ethereum-official-state-subset/staticcall-state-change-no-commit.json`
- source archive: `ethereum/tests` `fixtures_general_state_tests.tgz`
- selected sources: `stStaticCall/static_CALL_ZeroVCallSuicide.json`、`stStaticCall/static_ZeroValue_SUICIDE_OOGRevert.json`
- selected labels: `staticCallZeroValueCallSuicide`、`staticZeroValueSuicideOogRevert`

命令：

```powershell
cargo test -p novovm-adapter-novovm official_state_fixture_staticcall_state_change_no_commit_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 使用 fixture `txbytes` 走 raw EVM sender recovery、field decode、TxIR 构建和 adapter verify，不绕过生产验证路径
- 覆盖 `STATICCALL_CALL_SUICIDE` 顶层成功但内部状态变更不落盘，以及 `STATICCALL_SUICIDE_OOG_REVERT` 顶层 OOG 失败 no-commit
- raw empty calldata tx 初始为 state-agnostic `Transfer`，目标账户带 runtime code pre-state 后提升为 `ContractCall`
- official gasUsed 分别锁住 `83618` 和 `1000000`；失败 case 消耗完整 gas limit
- 官方 `logsHash` 全部为 empty logs hash；本门禁验证 success/failed artifact empty bloom/logs 路径
- 官方 target/touched account 的 code hash、code bytes、balance、nonce、storage facts 全部保持 pre/post 一致；BAL 只要求 sender fee debit post，不为零值 STATICCALL/SUICIDE case 伪造非 sender balance change

这证明当前 SUPERVM EVM adapter 已消费官方 STATICCALL state-change/SUICIDE no-commit grouped state fixture 子集，并把 raw tx -> state-aware contract-call execution -> official success/failed fee debit -> empty receipt/log path -> target/touched account preservation -> BAL sender post 的产品路径锁住。该门禁仍不是 opcode 级 state-test runner；STATICCALL 内部执行、SELFDESTRUCT 语义和更复杂 CREATE/内部 value-flow 仍由外部 AOEM/host artifact 承载。

### 44. Official state fixture subset: STATICCALL OOG no-commit grouped

本次继续不引入通用 state-test runner，而是一次性接入官方 `stStaticCall` 中 value=0、full-gas OOG、非 sender 账户不变的 no-commit grouped 子集：

- fixture: `crates/novovm-adapter-novovm/tests/fixtures/ethereum-official-state-subset/staticcall-oog-no-commit.json`
- source archive: `ethereum/tests` `fixtures_general_state_tests.tgz`
- selected sources: `static_call_OOG_additionalGasCosts1.json`、`static_call_OOG_additionalGasCosts2_Paris.json`、`static_CallAndCallcodeConsumeMoreGasThenTransactionHas.json`、`static_CallContractToCreateContractAndCallItOOG.json`、`static_CallContractToCreateContractOOGBonusGas.json`、`static_CallGoesOOGOnSecondLevel.json`、`static_CallGoesOOGOnSecondLevel2.json`、`static_CheckCallCostOOG.json`、`static_CheckOpcodes4.json`、`static_ZeroValue_CALL_OOGRevert.json`
- selected labels: `staticCallOogAdditionalGasCosts1`、`staticCallOogAdditionalGasCosts2Paris`、`staticCallAndCallcodeConsumeMoreGasThanTxHas`、`staticCallContractToCreateContractAndCallItOog`、`staticCallContractToCreateContractOogBonusGas`、`staticCallGoesOogOnSecondLevel`、`staticCallGoesOogOnSecondLevel2Data0`、`staticCheckCallCostOog`、`staticCheckOpcodes4Oog`、`staticZeroValueCallOogRevert`

命令：

```powershell
cargo test -p novovm-adapter-novovm official_state_fixture_staticcall_oog_no_commit_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 使用 fixture `txbytes` 走 raw EVM sender recovery、field decode、TxIR 构建和 adapter verify，不绕过生产验证路径
- 覆盖 additional gas cost、CALL/CALLCODE gas exhaustion、CREATE 内部 OOG、二层调用 OOG、call-cost OOG、opcode/static-context OOG、zero-value CALL OOG 等 10 个 Cancun/Prague projection
- raw empty calldata tx 初始为 state-agnostic `Transfer`，目标账户带 runtime code pre-state 后提升为 `ContractCall`；带 calldata 的 raw tx 保持 `ContractCall`
- official gasUsed 全部等于 gasLimit，覆盖 `22000` 到 `2000000` 的 full-gas failure debit
- 官方 `logsHash` 全部为 empty logs hash；本门禁验证 failed artifact empty bloom/logs 路径
- 官方 target/touched account 的 code hash、code bytes、balance、nonce、storage facts 全部保持 pre/post 一致；BAL 只要求 sender fee debit post，不为零值 OOG no-commit case 伪造非 sender balance change

这证明当前 SUPERVM EVM adapter 已消费官方 STATICCALL OOG no-commit grouped state fixture 子集，并把 raw tx -> state-aware contract-call execution -> official full-gas failed fee debit -> empty receipt/log path -> target/touched account preservation -> BAL sender post 的产品路径锁住。该门禁仍不是 opcode 级 state-test runner；STATICCALL/CALL/CALLCODE/CREATE 内部执行和更复杂 value-flow 仍由外部 AOEM/host artifact 承载。

## Readiness 矩阵

| 能力域 | 当前状态 | 证据 | 产品口径 |
| --- | --- | --- | --- |
| Novo mainline EVM host 执行闭环 | Pass | `submitted_total=16 processed_total=16 success_total=16 writes_total=16` | 可作为 Novo 主网控制 EVM 插件能力线 |
| Canonical store + BAL payload | Pass | strict scan `problems=0 complete_with_hash=1` | transfer smoke 可用 |
| contract call BAL 完整性 | Pass | adapter + plugin metadata tests pass, hash present；official STATICCALL/precompile/return-data grouped state fixture subset pass；official precompile failure/OOG grouped state fixture subset pass；official CALL output grouped state fixture subset pass；official CALL high-value/OOG grouped state fixture subset pass；official CALL depth/OOG grouped state fixture subset pass；official DELEGATECALL/CALLCODE account-context grouped state fixture subset pass；official zero-value calls revert no-commit grouped state fixture subset pass；official SELFDESTRUCT zero-value account-preservation grouped state fixture subset pass；official STATICCALL state-change/SUICIDE no-commit grouped state fixture subset pass；official STATICCALL OOG no-commit grouped state fixture subset pass；official LOG/receipt grouped state fixture subset pass；official RETURNDATA grouped state fixture subset pass；official LOG4/OOG receipt grouped state fixture subset pass | 成功 contract call 样本可声明 BAL 完整；empty calldata raw tx to code target 已具备状态感知 contract-call 执行分类；precompile failure/OOG、CALL output/high-value/depth-OOG、DELEGATECALL/CALLCODE context、zero-value calls revert no-commit、SELFDESTRUCT zero-value account preservation、STATICCALL state-change/SUICIDE no-commit、STATICCALL OOG no-commit、LOG/receipt、LOG4/OOG receipt 和 RETURNDATA projection 有官方子集门禁 |
| contract deploy BAL 完整性 | Pass | adapter + plugin metadata tests pass, hash present；CREATE/CREATE2 official geth address fixture subset pass；official CREATE/CREATE2 account grouped state fixture subset pass；CREATE2 artifact collision smoke pass | 成功 contract deploy 样本可声明 BAL 完整，fallback contract address 使用 geth CREATE 规则；top-level CREATE success/collision 已有官方 state fixture grouped 门禁；CREATE2 地址公式和 artifact collision 已有门禁 |
| geth ethapi receipt/log parity | Pass | 默认 fixture `sampleCount=11 totalMismatchCount=0` | 样本级兼容可声明 |
| 最新 go-ethereum ethapi export parity | Pass | external fixture `sampleCount=11 totalMismatchCount=0` | 对当前本机 geth ethapi 测试数据无 mismatch |
| typed tx failure / revert / fee edge parity | Pass | parity sections `typedTxFailure.mismatchCount=0` | 样本级可声明 |
| reorg canonical/noncanonical log view | Pass | parity sections `logs.mismatchCount=0` | 样本级可声明 |
| eth/71 BAL 相关 wire 能力 | Partial | BAL payload/canonical/scanner pass；eth/71 BAL wire encode/decode/frame + safe negotiation gate pass；无 snap 插件路径真实 RLPx `GetBlockAccessLists` -> `BlockAccessLists` response gate pass，覆盖 mainline canonical BAL materialization、materialized BAL RLP 和 missing sentinel；eth/70+snap/1 时 `0x22/0x23` 归 snap AccountRange，不被 BAL 抢占；未证明完整 eth/71 peer sync 和完整 BAL payload availability | 可声明 eth/71 BAL wire smoke 和无 snap 插件请求/响应样本；不能声明完整 eth/71 等价 |
| Ethereum fork rules / gas accounting / precompiles | Partial | execution-spec/fork-rule smoke matrix pass；adapter balance/fee/access-storage smoke pass；access-list entries 贯通 smoke pass；access-list warm/cold 成本、SLOAD sequence 和 BAL smoke pass；SLOAD warm/cold fee debit smoke pass；EIP-3529 SSTORE refund/cap/transition smoke pass；adapter SSTORE refund cap fee debit smoke pass；CREATE/CREATE2 official geth address fixture subset pass；official EIP-1559 sender balance state fixture subset pass；official SLOAD warm/cold state fixture subset pass；official SSTORE refund cap state fixture subset pass；official failure/account state fixture subset pass；official CREATE/CREATE2 account grouped state fixture subset pass；official STATICCALL/precompile/return-data grouped state fixture subset pass；official precompile failure/OOG grouped state fixture subset pass；official CALL output grouped state fixture subset pass；official CALL high-value/OOG grouped state fixture subset pass；official CALL depth/OOG grouped state fixture subset pass；official DELEGATECALL/CALLCODE account-context grouped state fixture subset pass；official zero-value calls revert no-commit grouped state fixture subset pass；official SELFDESTRUCT zero-value account-preservation grouped state fixture subset pass；official STATICCALL state-change/SUICIDE no-commit grouped state fixture subset pass；official STATICCALL OOG no-commit grouped state fixture subset pass；official LOG/receipt grouped state fixture subset pass；official RETURNDATA grouped state fixture subset pass；official LOG4/OOG receipt grouped state fixture subset pass；CREATE/CALL failure invariant smoke pass；CREATE existing-account collision smoke pass；CREATE2 artifact collision smoke pass；account balance value/fee invariant smoke pass；EIP-1559 effectiveGasPrice settlement smoke pass；未跑 Ethereum execution-spec state fixture 全量 | 可声明样本级 fork-rule、gas/refund/SLOAD sequence/SSTORE transition、SLOAD warm/cold fee debit、SSTORE refund cap fee debit、CREATE/CREATE2 geth address derivation official fixture subset、EIP-1559 sender balance official state fixture subset、SLOAD warm/cold official state fixture subset、SSTORE refund cap official state fixture subset、failure/account official state fixture subset、CREATE/CREATE2 account official grouped state fixture subset、STATICCALL/precompile/return-data official grouped state fixture subset、precompile failure/OOG official grouped state fixture subset、CALL output official grouped state fixture subset、CALL high-value/OOG official grouped state fixture subset、CALL depth/OOG official grouped state fixture subset、DELEGATECALL/CALLCODE account-context official grouped state fixture subset、zero-value calls revert no-commit official grouped state fixture subset、SELFDESTRUCT zero-value account-preservation official grouped state fixture subset、STATICCALL state-change/SUICIDE no-commit official grouped state fixture subset、STATICCALL OOG no-commit official grouped state fixture subset、LOG/receipt official grouped state fixture subset、RETURNDATA official grouped state fixture subset、LOG4/OOG receipt official grouped state fixture subset、CREATE/CALL failure invariants、CREATE/CREATE2 existing-account collision invariant、account balance value/fee invariants、EIP-1559 effectiveGasPrice settlement、tracked-account fee/value debit、access-list read-set/warm-cold smoke/BAL gate；不能声明 EVM 语义全等价 |
| raw Ethereum transaction ingestion/execution | Partial | signed legacy/type1/type2/type3 transfer + typed call/deploy smoke pass；raw nonce gap reject pass；gateway raw write surface pass；gateway txpool error surface pass；plugin txpool replacement/reject pass；plugin fee settlement pass；adapter tracked-account value/fee debit pass；adapter account balance value/fee invariant pass；adapter effectiveGasPrice fee debit pass；access-list entries 贯通 pass；BAL strict scan pass | 可声明 raw transfer/call/deploy smoke 可执行，gateway 写入/拒绝面、plugin txpool/fee settlement、adapter tracked-account debit、account balance invariant、effectiveGasPrice settlement、access-list read-set 有 gate；不能声明 raw tx 全等价 |
| JSON-RPC full-node surface | Partial | mainline query receipt/log 样本 pass；gateway block/tx/filter/call/estimateGas smoke pass；indexed block/tx/receipt/uncle smoke pass；pending/runtime smoke pass；store recovery smoke pass；未覆盖 tracing/debug/admin 和全 geth RPC 行为 | 可声明 gateway JSON-RPC 产品面样本可用；不能声明 geth RPC 等价 |
| devp2p/RLPx peer sync / block import | Partial | 真实 RLPx handshake/Status + 入站 `Transactions` -> native pending tx raw RLP gate pass；出站 `Transactions` broadcast gate pass；pooled tx hash/request/response gate pass；block header/body/receipts sync gate pass；最小 reorg 回池 gate pass；`NewBlock` / `NewBlockHashes` gates pass；`NewBlock` 和 `BlockBodies` transaction trie root validation pass；`Receipts` completeness/count/root validation pass；validated native receipt snapshot + local `GetReceipts` replay pass；empty/no-withdrawal stateRoot continuity validation pass；snap/1 AccountRange offset/request/response gate pass；AccountRange -> StorageRanges/ByteCodes follow-up/native cache/codeHash check gate pass；snap/1 `GetStorageRanges`/`GetByteCodes`/`GetTrieNodes` service sidecar response gate pass；`novovm-node` direct RLPx sync entry live mainnet finite run pass；geth DNS discovery ENR 候选池扩容 pass；未覆盖 DNS tree signature verification、完整 discv4 peer churn、proof/root verification、完整历史 receipt store、完整 snap state heal/download/store、完整 state root execution validation、完整 eth/71 peer sync、长稳主网接受度和复杂多分支 reorg | 可声明最小 RLPx tx propagation/pooled-tx/header-body-receipts import/reorg 回池/NewBlock/NewBlockHashes、`transactionsRoot`、receipt completeness/count、`receiptsRoot` validation、已验证 receipts 回放、可判定 empty/no-withdrawal stateRoot continuity、snap AccountRange request/response、AccountRange 后续 StorageRanges/ByteCodes 请求和 native cache/codeHash 校验链路、node 直接 RLPx sync 入口、DNS ENR 候选池扩容、snap sidecar 空响应服务面可观察；不能声明以太坊全节点 |

## 当前产品判定

可以声明：

`SUPERVM 当前具备 Novo 主网可控 EVM 插件执行能力，能产出 canonical EVM block metadata，并对 BAL payload 进行严格扫描；对 geth ethapi receipt/log/typed-failure 样本具备 parity。`

`在本文门禁范围内，SUPERVM EVM 插件具备协议可观察等价 v1：execution observable、geth/RPC fixture observable、plugin receipt/BAL observable 均有聚合回归 gate。`

不能声明：

`SUPERVM 是完整 geth 替代品。`

`SUPERVM 是完整以太坊全节点。`

`SUPERVM 已完整支持 eth/71 P2P 同步和全部 BAL wire 行为。`

## 下一步门禁顺序

1. 已接入官方 geth address fixture 子集、官方 EIP-1559 sender balance state fixture 子集、官方 SLOAD warm/cold state fixture 子集、官方 SSTORE refund cap grouped state fixture 子集、官方 failure/account grouped state fixture 子集、官方 CREATE/CREATE2/account grouped state fixture 子集、官方 STATICCALL/precompile/return-data grouped state fixture 子集、官方 precompile failure/OOG grouped state fixture 子集、官方 CALL output grouped state fixture 子集、官方 CALL high-value/OOG grouped state fixture 子集、官方 CALL depth/OOG grouped state fixture 子集、官方 DELEGATECALL/CALLCODE account-context grouped state fixture 子集、官方 zero-value calls revert no-commit grouped state fixture 子集、官方 SELFDESTRUCT zero-value account-preservation grouped state fixture 子集、官方 STATICCALL state-change/SUICIDE no-commit grouped state fixture 子集、官方 STATICCALL OOG no-commit grouped state fixture 子集、官方 LOG/receipt grouped state fixture 子集、官方 RETURNDATA grouped state fixture 子集和官方 LOG4/OOG receipt grouped state fixture 子集。
2. 不再把继续堆官方 fixture 子集作为默认下一步。只有 v2/v3 黑盒差分暴露具体语义缺口时，才按缺口补对应官方子集。
3. 已完成 v2a：`eth_getBlockByNumber/eth_getBlockByHash` 的 `transactionsRoot/receiptsRoot` 不再返回 `null`，geth parity report 新增 `observableProjection`，默认 11 个样本 `mismatchCount=0`。
4. 已完成 v2b：真实 geth fullTx block fixture 差分已接入，raw tx RLP 进入 canonical block projection，当前 `number/gasUsed/logsBloom/transactionsRoot/receiptsRoot/stateRoot` 全部 match，`knownGapCount=0`。
5. 已开始 v3：真实 RLPx handshake/Status + 入站 `Transactions` -> native pending tx raw RLP gate 通过；出站 `Transactions` broadcast gate 通过；pooled tx hash/request/response gate 通过；block header/body/receipts sync gate 通过；最小 reorg 回池 gate 通过；`NewBlock` / `NewBlockHashes` gates 通过；`NewBlock` / `BlockBodies` transaction trie root validation、`Receipts` completeness/count/root validation、native receipt snapshot、本地 `GetReceipts` replay、empty/no-withdrawal stateRoot continuity validation、snap/1 AccountRange、AccountRange -> StorageRanges/ByteCodes follow-up/native cache/codeHash check、node direct RLPx sync 入口、geth DNS discovery ENR 候选池扩容和 snap/1 service sidecars gate 通过，仍不把 SUPERVM 产品口径改成完整 geth 全节点。
6. 如果 v3 或真实 block replay 暴露具体交易类型/root 差异，再补对应最小真实 fixture，不回到开放式 smoke 堆叠。

## 回归命令

协议可观察等价 v1 聚合 gate：

```powershell
cargo test -p novovm-adapter-novovm evm_protocol_observable_equivalence_execution_gate_v1 -- --nocapture
cargo test -p novovm-node evm_protocol_observable_equivalence_geth_rpc_fixture_gate_v1 -- --nocapture
cargo test -p novovm-adapter-evm-plugin evm_protocol_observable_equivalence_plugin_receipt_bal_gate_v1 -- --nocapture
cargo test -p novovm-node evm_protocol_observable_equivalence_geth_rpc_blackbox_projection_gate_v2 -- --nocapture
cargo test -p novovm-node evm_protocol_observable_equivalence_geth_real_block_diff_gate_v2b -- --nocapture
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
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_snap_service_sidecars_gate_v3 -- --nocapture
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_new_block_gate_v3 -- --nocapture
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_new_block_hashes_gate_v3 -- --nocapture
$env:NOVOVM_NODE_MODE='eth_rlpx_sync'; $env:NOVOVM_ETH_RLPX_TICKS='8'; cargo run -p novovm-node --bin novovm-node
```

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
cargo test -p novovm-evm-gateway raw_tx_gateway_write_surface_smoke_v1 -- --nocapture
cargo test -p novovm-evm-gateway raw_tx_gateway_txpool_error_surface_smoke_v1 -- --nocapture
cargo test -p novovm-adapter-evm-plugin txpool_replacement_and_reject_surface_smoke_v1 -- --nocapture
cargo test -p novovm-adapter-evm-plugin fee_settlement_ingress_surface_smoke_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
cargo test -p novovm-adapter-evm-core translate_type1_fields_extracts_access_list_intrinsic_counts -- --nocapture
cargo test -p novovm-adapter-evm-core access_list_warm_storage_read_reduces_execution_gas_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core access_list_warm_account_access_reduces_execution_gas_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core sload_sequence_reuses_warm_storage_key_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core sload_sequence_respects_access_list_initial_warm_set_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core sload_sequence_keeps_address_and_slot_in_access_key_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core sstore_clear_refund_matches_eip3529_schedule_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core eip3529_refund_cap_limits_refunded_gas_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core sstore_transition_clean_slots_match_eip3529_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core sstore_transition_dirty_slots_match_eip3529_m0 -- --nocapture
cargo test -p novovm-adapter-novovm execute_raw_type1_access_list_emits_declared_storage_reads_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_execution_spec_access_list_warm_storage_smoke_v1 -- --nocapture
cargo test -p novovm-adapter-novovm execute_success_call_debits_refunded_sstore_gas_used_v1 -- --nocapture
cargo test -p novovm-evm-gateway eth_send_transaction_infers_type1_from_access_list -- --nocapture
cargo test -p novovm-adapter-novovm evm_execution_spec_create_call_failure_state_invariants_v1 -- --nocapture
cargo test -p novovm-adapter-novovm typed_type2_semantics_reject_intrinsic_gas_too_low_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_equivalence_baseline_matrix_receipt_revert_gas_v1 -- --nocapture
```

Official geth address fixture subset gate：

```powershell
cargo test -p novovm-adapter-evm-core derive_create_contract_address_matches_geth_vectors_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core derive_create2_contract_address_matches_geth_vectors_m0 -- --nocapture
```

Official state fixture subset gate：

```powershell
cargo test -p novovm-adapter-novovm official_state_fixture_eip1559_sender_balance_fee_debit_v1 -- --nocapture
cargo test -p novovm-adapter-novovm official_state_fixture_sload_warm_cold_fee_debit_v1 -- --nocapture
cargo test -p novovm-adapter-novovm official_state_fixture_sstore_refund_cap_fee_debit_v1 -- --nocapture
cargo test -p novovm-adapter-novovm official_state_fixture_failure_account_fee_debit_v1 -- --nocapture
cargo test -p novovm-adapter-novovm official_state_fixture_zero_calls_revert_no_commit_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm official_state_fixture_selfdestruct_zero_value_account_preservation_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm official_state_fixture_staticcall_state_change_no_commit_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm official_state_fixture_staticcall_oog_no_commit_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm official_state_fixture_create_account_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm official_state_fixture_staticcall_precompile_return_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm official_state_fixture_precompile_failure_oog_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm official_state_fixture_call_output_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm official_state_fixture_call_high_value_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm official_state_fixture_call_depth_oog_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm official_state_fixture_delegatecall_callcode_context_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm official_state_fixture_log_receipt_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm official_state_fixture_log4_oog_receipt_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm official_state_fixture_return_data_grouped_projection_v1 -- --nocapture
```

Execution-spec/fork-rule smoke gate：

```powershell
cargo test -p novovm-adapter-evm-core access_list_warm_storage_read_reduces_execution_gas_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core access_list_warm_account_access_reduces_execution_gas_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core sload_sequence_reuses_warm_storage_key_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core sload_sequence_respects_access_list_initial_warm_set_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core sload_sequence_keeps_address_and_slot_in_access_key_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core sstore_clear_refund_matches_eip3529_schedule_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core eip3529_refund_cap_limits_refunded_gas_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core sstore_transition_clean_slots_match_eip3529_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core sstore_transition_dirty_slots_match_eip3529_m0 -- --nocapture
cargo test -p novovm-adapter-novovm evm_execution_spec_fork_rule_smoke_matrix_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_execution_spec_access_list_warm_storage_smoke_v1 -- --nocapture
cargo test -p novovm-adapter-novovm execute_success_call_debits_refunded_sstore_gas_used_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_execution_spec_create_call_failure_state_invariants_v1 -- --nocapture
```

eth/71 BAL wire smoke gate：

```powershell
cargo test -p novovm-network eth71_bal_wire_roundtrip_and_negotiation_gate_v1 -- --nocapture
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_bal_response_gate_v3 -- --nocapture
```

Gateway JSON-RPC 产品面 smoke：

```powershell
cargo test -p novovm-evm-gateway json_rpc_parity_surface_smoke_block_tx_filter_call_estimate_v1 -- --nocapture
```

Gateway JSON-RPC indexed block/tx/receipt smoke：

```powershell
cargo test -p novovm-evm-gateway json_rpc_indexed_block_tx_receipt_uncle_surface_smoke_v1 -- --nocapture
```

Gateway JSON-RPC pending/runtime smoke：

```powershell
cargo test -p novovm-evm-gateway json_rpc_pending_runtime_surface_smoke_v1 -- --nocapture
```

Gateway JSON-RPC store recovery smoke：

```powershell
cargo test -p novovm-evm-gateway json_rpc_store_recovery_surface_smoke_v1 -- --nocapture
```
