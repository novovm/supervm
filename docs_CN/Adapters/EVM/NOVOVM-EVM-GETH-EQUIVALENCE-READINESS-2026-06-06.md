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

## Readiness 矩阵

| 能力域 | 当前状态 | 证据 | 产品口径 |
| --- | --- | --- | --- |
| Novo mainline EVM host 执行闭环 | Pass | `submitted_total=16 processed_total=16 success_total=16 writes_total=16` | 可作为 Novo 主网控制 EVM 插件能力线 |
| Canonical store + BAL payload | Pass | strict scan `problems=0 complete_with_hash=1` | transfer smoke 可用 |
| contract call BAL 完整性 | Pass | adapter + plugin metadata tests pass, hash present | 成功 contract call 样本可声明 BAL 完整 |
| contract deploy BAL 完整性 | Pass | adapter + plugin metadata tests pass, hash present；CREATE/CREATE2 official geth address fixture subset pass；CREATE2 artifact collision smoke pass | 成功 contract deploy 样本可声明 BAL 完整，fallback contract address 使用 geth CREATE 规则；CREATE2 地址公式和 artifact collision 已有门禁 |
| geth ethapi receipt/log parity | Pass | 默认 fixture `sampleCount=11 totalMismatchCount=0` | 样本级兼容可声明 |
| 最新 go-ethereum ethapi export parity | Pass | external fixture `sampleCount=11 totalMismatchCount=0` | 对当前本机 geth ethapi 测试数据无 mismatch |
| typed tx failure / revert / fee edge parity | Pass | parity sections `typedTxFailure.mismatchCount=0` | 样本级可声明 |
| reorg canonical/noncanonical log view | Pass | parity sections `logs.mismatchCount=0` | 样本级可声明 |
| eth/71 BAL 相关 wire 能力 | Partial | BAL payload/canonical/scanner pass；eth/71 BAL wire encode/decode/frame + safe negotiation gate pass；未证明完整 eth/71 peer sync | 可声明 eth/71 BAL wire smoke；不能声明完整 eth/71 等价 |
| Ethereum fork rules / gas accounting / precompiles | Partial | execution-spec/fork-rule smoke matrix pass；adapter balance/fee/access-storage smoke pass；access-list entries 贯通 smoke pass；access-list warm/cold 成本、SLOAD sequence 和 BAL smoke pass；SLOAD warm/cold fee debit smoke pass；EIP-3529 SSTORE refund/cap/transition smoke pass；adapter SSTORE refund cap fee debit smoke pass；CREATE/CREATE2 official geth address fixture subset pass；official EIP-1559 sender balance state fixture subset pass；official SLOAD warm/cold state fixture subset pass；official SSTORE refund cap state fixture subset pass；CREATE/CALL failure invariant smoke pass；CREATE existing-account collision smoke pass；CREATE2 artifact collision smoke pass；account balance value/fee invariant smoke pass；EIP-1559 effectiveGasPrice settlement smoke pass；未跑 Ethereum execution-spec state fixture 全量 | 可声明样本级 fork-rule、gas/refund/SLOAD sequence/SSTORE transition、SLOAD warm/cold fee debit、SSTORE refund cap fee debit、CREATE/CREATE2 geth address derivation official fixture subset、EIP-1559 sender balance official state fixture subset、SLOAD warm/cold official state fixture subset、SSTORE refund cap official state fixture subset、CREATE/CALL failure invariants、CREATE/CREATE2 existing-account collision invariant、account balance value/fee invariants、EIP-1559 effectiveGasPrice settlement、tracked-account fee/value debit、access-list read-set/warm-cold smoke/BAL gate；不能声明 EVM 语义全等价 |
| raw Ethereum transaction ingestion/execution | Partial | signed legacy/type1/type2/type3 transfer + typed call/deploy smoke pass；raw nonce gap reject pass；gateway raw write surface pass；gateway txpool error surface pass；plugin txpool replacement/reject pass；plugin fee settlement pass；adapter tracked-account value/fee debit pass；adapter account balance value/fee invariant pass；adapter effectiveGasPrice fee debit pass；access-list entries 贯通 pass；BAL strict scan pass | 可声明 raw transfer/call/deploy smoke 可执行，gateway 写入/拒绝面、plugin txpool/fee settlement、adapter tracked-account debit、account balance invariant、effectiveGasPrice settlement、access-list read-set 有 gate；不能声明 raw tx 全等价 |
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

1. 已接入官方 geth address fixture 子集、官方 EIP-1559 sender balance state fixture 子集、官方 SLOAD warm/cold state fixture 子集和官方 SSTORE refund cap grouped state fixture 子集。
2. 如要继续提高执行语义置信度，下一步接 failure/account 或 CREATE2 execution/collision 官方 state fixture 子集；基于现在已贯通的 `TxIR.evm_access_list`、SLOAD sequence/fee debit smoke、EIP-3529 SSTORE transition/cap fee debit smoke、CREATE/CREATE2 address derivation smoke、CREATE/CALL failure invariant smoke、CREATE/CREATE2 collision invariant smoke、account balance value/fee invariant smoke 和 effectiveGasPrice settlement smoke 接官方 fixture；不要再做包装层。
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
