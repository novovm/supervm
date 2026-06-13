# NOVOVM Native Execution Pipeline Production Soak v1
_2026-06-14_

## 1. 阶段边界

本阶段基于 `native-execution-pipeline-lifecycle-v1` 冻结边界推进，只做 soak、恢复、故障注入和报告门禁，不扩展 pipeline 功能。

冻结规则：

- 不修改 `product raw tx ingress -> pending queue -> AOEM batch -> proof/receipt -> dirty sharded atomic commit -> canonical projection -> broadcast` 主结构。
- 不改变 `AOEM_runtime` 执行并发 owner。
- 不让 Rust host 自建执行调度器。
- 不新增账户/资产真相源。
- 不把 `nov_sendRawTransaction` / `nov_execute` 改回即时执行。
- 不绕过 AOEM tick 生成 receipt/state。

## 2. 第一刀：Production Soak Report Skeleton

已新增独立 gate：

```text
cargo run -p novovm-node --bin supervm-native-pipeline-production-soak
```

该 gate 只作为验证包装层：

- 启动现有 `novovm-node`。
- 设置 `NOVOVM_NODE_MODE=native_execution_pipeline`。
- 设置 `NOVOVM_NATIVE_SEND_RAW_TRANSACTION_PIPELINE_ONLY=true`。
- 使用现有 native pipeline summary。
- 验证 AOEM owner、host policy、dirty sharded RocksDB commit、queue drain、dropped/rejected 预算。
- 写出 `novovm-native-pipeline-production-soak-report/v1`。

默认报告：

```text
artifacts/native-pipeline/native-pipeline-production-soak-30min.json
artifacts/native-pipeline/native-pipeline-production-soak-30min-summary.json
```

## 3. Profile

支持 profile：

```text
30min      -> 默认 1800 seconds
2h         -> 默认 7200 seconds
overnight  -> 默认 28800 seconds
```

本地和 CI smoke 可用短时覆盖：

```powershell
$env:NOVOVM_NATIVE_PIPELINE_PRODUCTION_SOAK_PROFILE="30min"
$env:NOVOVM_NATIVE_PIPELINE_PRODUCTION_SOAK_DURATION_SECONDS="2"
$env:NOVOVM_NATIVE_PIPELINE_PRODUCTION_SOAK_TX_COUNT="64"
$env:NOVOVM_NATIVE_PIPELINE_PRODUCTION_SOAK_BATCH_BUDGET="16"
$env:NOVOVM_NATIVE_PIPELINE_PRODUCTION_SOAK_REPORT_PATH="artifacts/native-pipeline/native-pipeline-production-soak-30min.json"
cargo run -p novovm-node --bin supervm-native-pipeline-production-soak
```

正式 30min/2h/overnight 只拉长 duration 和 tx_count，不改变报告 schema。

## 4. 验收指标

当前 report skeleton 固定以下检查：

- `execution_kernel = AOEM`
- `aoem_concurrency_owner = AOEM_runtime`
- `host_concurrency_policy = host_drives_lifecycle_only_no_rust_execution_scheduler`
- `ingress_submitted_total >= tx_count`
- `product_ingress_submitted_total >= tx_count`
- `aoem_executed_total >= tx_count`
- `included_canonical_total >= tx_count`
- `max_product_ingress_submitted_per_tick >= batch_budget`
- `max_queue_admitted_per_tick >= batch_budget`
- `max_aoem_batch_executed_per_tick >= batch_budget`
- `max_proof_items_per_tick >= batch_budget`
- `max_commit_items_per_tick >= batch_budget`
- `max_broadcast_tx_per_tick >= batch_budget`
- `queue_pending_last = 0`
- `queue_dropped_last <= budget`
- `queue_rejected_last <= budget`
- `native_store_commit_model` contains `dirty_sharded_atomic_batch`
- `native_store_rocksdb_enabled = true`
- `native_store_transactional_commit = true`

## 5. 未完成项

本文件只签收 Production Soak v1 的第一刀 report skeleton。以下仍待后续刀完成：

- RocksDB recovery gate：dirty store、semantic head、receipt index、canonical body/head 重启恢复。
- Pending queue crash recovery gate。
- Remote reentry dedup gate。
- Network fault injection：packet loss、duplicate、delay、reorder。
- Cross-machine UDP soak：sender host / receiver host / env-config peer address。
- Long-run production profile：真实 30min / 2h / overnight 报告归档。
