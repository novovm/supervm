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
- CREATE/CALL failure state invariant smoke: pass, covers failed CALL no value/storage/log commit and failed CREATE no contract account/code/storage/BAL contract entry
- execution-spec/fork-rule smoke matrix: pass, covers intrinsic gas, access-list gas, Amsterdam calldata/access-list floor, precompile set, create/call/revert, storage write and rebuilt logs
- eth/71 BAL wire smoke: pass, covers GetBlockAccessLists/BlockAccessLists payload encode/decode, frame roundtrip, malformed BAL rejection, and safe negotiation fallback to eth/70 when remote advertises eth/71

这证明 `NOVOVM_ETH_SEND_RAW_TX(_FILE)` 可以作为 Novo mainline EVM host 的真实输入源，执行后产出 canonical batch 和完整 BAL hash。当前覆盖 signed legacy/type1/type2/type3 transfer smoke，以及 type1/type2 call/deploy、type3 call smoke；type3 仍是显式开关能力，不能外推到全部 fork rule / gas / blob sidecar 语义。

失败路径方面，当前已经证明 raw signed transaction 在解码和签名恢复后，会进入 Novo 统一账户控制面并被 nonce gate 拒绝；typed gas 语义和 contract failure/revert artifact 仍是 adapter 层样本门禁，不声明覆盖全部 geth txpool / execution failure 行为。

fork-rule 方面，当前只有最小 smoke matrix：覆盖 EVM core gas/precompile 规则和 adapter create/call/revert 执行结果，不等价于 Ethereum execution-spec 全量 fixture。

CREATE/CALL failure 方面，当前已证明 failed CALL 不提交 value transfer、target storage write、event logs；failed CREATE 即使 artifact 携带 contract_address/runtime_code，也不会创建 contract account/code/storage，也不会产出 contract BAL entry。

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
- 覆盖余额不足时拒绝执行且不推进 nonce
- 覆盖 sender post-balance 写入 observed BAL balance change
- 覆盖 type1 access-list intrinsic gas extras
- 覆盖 raw type1 access-list address/storage key 贯通到 TxIR
- 覆盖 raw type1 access-list declared storage read 写入 observed BAL
- 覆盖 access-list warm storage read、SLOAD accessed_storage_keys 顺序语义和 adapter observed BAL
- 覆盖 EIP-3529 SSTORE clear refund `4800`、refund cap `1/5`、SSTORE clean/dirty transition gas/refund delta 和 adapter post-refund gas fee debit
- 覆盖 contract call storage write 的 observed BAL
- 覆盖 contract deploy code/storage/balance observed BAL
- 覆盖 native adapter smoke 显式 funded sender
- 覆盖 EVM plugin 全包回归，确认 adapter 扣费语义未破坏 plugin apply/metadata 主线

这证明 adapter 在拿到 sender account pre-state 时，会执行生产级 value/fee debit 和余额不足拒绝；没有 sender account pre-state 的 plugin smoke 仍保持控制面执行，不伪造余额。当前已贯通 raw / gateway access-list entries 到 TxIR，并能把 declared storage read 写入 observed BAL；已补最小 warm/cold 成本、SLOAD accessed_storage_keys 顺序语义、EIP-3529 refund、SSTORE transition 和 BAL 执行观测门禁，但仍未跑 Ethereum execution-spec 官方 warm/cold/refund fixture，因此不声明完整 EVM gas/refund 等价。

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
- core/adapter/plugin full package tests pass

这证明 Novo EVM 插件当前不是只记录 access-list intrinsic gas；它已经具备最小 warm/cold 成本模型、SLOAD accessed_storage_keys 顺序语义和执行观测门禁。该门禁仍不是 opcode 级 geth EVM，也不是 Ethereum execution-spec 官方 fixture 全量通过。

### 16. Execution-spec SSTORE refund / transition smoke

依据本机最新 `D:\WEB3_AI\go-ethereum` 规则，当前 London/EIP-3529 后 `SstoreClearsScheduleRefundEIP3529 = 5000 - 2100 + 1900 = 4800`，refund cap 为 `gas_used / 5`。

命令：

```powershell
cargo test -p novovm-adapter-evm-core sstore_clear_refund_matches_eip3529_schedule_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core eip3529_refund_cap_limits_refunded_gas_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core sstore_transition_clean_slots_match_eip3529_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core sstore_transition_dirty_slots_match_eip3529_m0 -- --nocapture
cargo test -p novovm-adapter-novovm execute_success_call_debits_refunded_sstore_gas_used_v1 -- --nocapture
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
- core/adapter/plugin full package tests pass

这证明当前产品面已经能处理 post-refund gas fee settlement，并把 SSTORE clear refund、refund cap、clean/dirty transition 的关键数值锁进 core gate。它仍不是 opcode 级 SSTORE 执行器全量实现，后续若要声明完整等价，需要接 Ethereum execution-spec 官方 SSTORE fixture。

### 17. Execution-spec CREATE/CALL failure invariants smoke

命令：

```powershell
cargo test -p novovm-adapter-novovm evm_execution_spec_create_call_failure_state_invariants_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_equivalence_baseline_matrix_receipt_revert_gas_v1 -- --nocapture
```

结果：

- pass
- failed CALL 不提交 value transfer
- failed CALL 不写 target storage，且不产生 event logs / log bloom
- failed CALL 的 target BAL account entry 不包含 storage changes
- failed CREATE 即使 artifact 携带 `contract_address` / `runtime_code` / `runtime_code_hash`，resolved artifact 也会清空 contract/runtime 字段
- failed CREATE 不创建 contract account，不写 deploy/runtime storage，不产出 contract BAL account entry
- failure classification 仍落到 sender 侧 metadata，不伪造成合约状态变更

这证明当前 adapter 产品面已经把 CREATE/CALL 失败路径的状态不变式锁住。它仍不是官方 execution-spec 全量 fixture；后续要声明等价，需要把这些 invariant 接入官方 failure fixture 子集。

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
| Ethereum fork rules / gas accounting / precompiles | Partial | execution-spec/fork-rule smoke matrix pass；adapter balance/fee/access-storage smoke pass；access-list entries 贯通 smoke pass；access-list warm/cold 成本、SLOAD sequence 和 BAL smoke pass；EIP-3529 SSTORE refund/cap/transition smoke pass；CREATE/CALL failure invariant smoke pass；未跑 Ethereum execution-spec 全量 fixture | 可声明样本级 fork-rule、gas/refund/SLOAD sequence/SSTORE transition、CREATE/CALL failure invariants、tracked-account fee/value debit、access-list read-set/warm-cold smoke/BAL gate；不能声明 EVM 语义全等价 |
| raw Ethereum transaction ingestion/execution | Partial | signed legacy/type1/type2/type3 transfer + typed call/deploy smoke pass；raw nonce gap reject pass；gateway raw write surface pass；gateway txpool error surface pass；plugin txpool replacement/reject pass；plugin fee settlement pass；adapter tracked-account value/fee debit pass；access-list entries 贯通 pass；BAL strict scan pass | 可声明 raw transfer/call/deploy smoke 可执行，gateway 写入/拒绝面、plugin txpool/fee settlement、adapter tracked-account debit、access-list read-set 有 gate；不能声明 raw tx 全等价 |
| JSON-RPC full-node surface | Partial | mainline query receipt/log 样本 pass；gateway block/tx/filter/call/estimateGas smoke pass；indexed block/tx/receipt/uncle smoke pass；pending/runtime smoke pass；store recovery smoke pass；未覆盖 tracing/debug/admin 和全 geth RPC 行为 | 可声明 gateway JSON-RPC 产品面样本可用；不能声明 geth RPC 等价 |
| devp2p/RLPx peer sync / block import | Partial | 有 gateway/network 代码和 canary，但未作为本矩阵通过项 | 不能声明以太坊全节点 |

## 当前产品判定

可以声明：

`SUPERVM 当前具备 Novo 主网可控 EVM 插件执行能力，能产出 canonical EVM block metadata，并对 BAL payload 进行严格扫描；对 geth ethapi receipt/log/typed-failure 样本具备 parity。`

不能声明：

`SUPERVM 是 geth 等价实现。`

`SUPERVM 是完整以太坊全节点。`

`SUPERVM 已完整支持 eth/71 P2P 同步和全部 BAL wire 行为。`

## 下一步门禁顺序

1. 如要继续提高执行语义置信度，接入 Ethereum execution-spec 官方 fixture 子集，优先选账户余额、CREATE/CALL failure edge、SSTORE refund cap edge 和 SLOAD warm/cold edge 样本。
2. 如要完整验证 access-list warm/cold/refund/failure 语义，基于现在已贯通的 `TxIR.evm_access_list`、SLOAD sequence smoke、EIP-3529 SSTORE transition smoke 和 CREATE/CALL failure invariant smoke 接官方 fixture；不要再做包装层。
3. 如继续扩展 JSON-RPC parity，可补更多 batch/mixed-param edge case；tracing/debug/admin 仍不作为 Novo EVM 插件主线优先项。
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
