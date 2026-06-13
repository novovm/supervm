# NOVOVM Native Execution Pipeline Lifecycle Signoff
_2026-06-14_

## 1. Signoff 结论

`NOVOVM Native Execution Pipeline Lifecycle Gate` 阶段性签收为 `PASS`。

本轮签收范围是 NOVOVM 原生交易从产品入口到 AOEM 执行、证明、落账、canonical 投影和广播输出的主线生命周期。该结论不声明最终生产主网全部完成，也不声明恶劣公网、跨机器长时 soak、重启恢复和 hostile network 全部封口。

当前冻结边界：

```text
product raw tx ingress
  -> pending queue
  -> AOEM batch
  -> proof / receipt
  -> dirty sharded atomic commit
  -> canonical projection
  -> broadcast
```

## 2. AOEM 执行边界

NOVOVM 主线只驱动生命周期，不在 Rust 主线自建执行并发调度器。

冻结规则：

- `execution_kernel = AOEM`
- `aoem_concurrency_owner = AOEM_runtime`
- Rust host 负责 ingress、pending runtime、tick budget、batch 封装、proof/commit 调用、canonical projection、broadcast 驱动。
- Rust host 不允许替代 AOEM 自建交易执行并发内核。
- 原生交易高频能力必须通过 AOEM 统一语义执行内核承接，而不是在 gateway、node RPC 或 store 层分叉实现。

## 3. 落账边界

`dirty sharded atomic commit` 是最终 canonical head 的确定性原子封口，不是老式串行逐笔执行模型。

正确语义：

```text
网络高频接入
  -> pending queue 批量准入
  -> AOEM 代数语义批执行 / proof / precommit
  -> deterministic dirty-set materialization
  -> sharded RocksDB atomic write batch
  -> receipt / state / semantic head 同批一致
```

落账规则：

- 同一 AOEM batch 的状态、receipt、receipt height index、module state、semantic head 必须原子提交。
- 不允许出现 receipt 已存在但 semantic head 未推进的半提交状态。
- 不允许出现 state 已更新但 receipt 缺失的半提交状态。
- `dirty sharded atomic commit` 可以是批量、分片、脏集提交，但必须保留确定性原子边界。

## 4. 产品入口边界

原生产品入口必须先入 pending runtime，再由 AOEM tick 生命周期产出 receipt/state。

冻结规则：

- 生产模式设置 `NOVOVM_NATIVE_SEND_RAW_TRANSACTION_PIPELINE_ONLY=true`。
- `nov_sendRawTransaction` 在产品模式下只负责接收、校验、索引和写入 pending。
- `nov_execute` 继承 pending-only 语义，不允许绕过 pending/AOEM tick 直接落账。
- receipt/state/canonical projection 只能由 `nov_runNativeExecutionTick` 或 native execution pipeline tick 产出。
- legacy immediate dispatch 仅保留为兼容/调试路径，不是产品高频 pipeline。

## 5. 已完成能力线

### 5.1 Pending 到 AOEM

- `8b7c064 feat(node): execute native pending txs through AOEM batch`
- `e736839 feat(node): add native AOEM execution tick`
- `33c2020 feat(node): add native execution pipeline mode`

完成内容：

- native pending tx 不再停留在队列。
- `nov_executePendingNativeTxBatch` 可将 pending tx 送入 AOEM batch。
- `nov_runNativeExecutionTick` 封装 pending drain、AOEM batch、proof/commit、receipt/state 输出。
- `NOVOVM_NODE_MODE=native_execution_pipeline` 可由真实 `novovm-node` 二进制驱动生命周期。

### 5.2 Canonical 与广播闭环

- `e393081 feat(node): broadcast native pipeline pending output`
- `16da665 feat(node): gate multitick native execution pipeline`
- `8c54b88 feat(network): broadcast native pending tx over transport`
- `ea26c53 feat(node): add dual node native pipeline gate`
- `be3cb2d feat(node): add native pipeline dual node soak report`
- `f4ba432 ci: add native pipeline dual node gate`

完成内容：

- AOEM 执行结果进入 canonical projection。
- pending output 可进入 broadcast candidates。
- UDP transport 支持 native pending tx 广播。
- dual-node gate 覆盖 sender ingress、UDP broadcast、receiver reentry、receiver AOEM tick、receiver canonical included。

### 5.3 Sustained / Fanout / 批宽门禁

- `3705d4b ci(node): add sustained native pipeline gate`
- `947d4d8 ci(node): add paced dual node native pipeline gate`
- `a8414d9 ci(node): extend native pipeline gate to fanout receivers`
- `7dd56ab ci(node): require product ingress in native pipeline gate`
- `a6fb86e ci(node): gate native pipeline batch width`
- `d90dd46 ci(node): gate dual-node receiver batch width`
- `32eb0bf ci(node): gate nonempty native pipeline proof commit`
- `354e4de ci(node): require sharded store in native pipeline gates`
- `493845d ci(node): gate native pipeline broadcast batch width`
- `86c812a ci(node): gate native pipeline proof and commit batch width`
- `21d1a0d ci(node): gate native pipeline ingress and admission batch width`

完成内容：

- sustained gate 覆盖高频本地生命周期。
- paced dual-node gate 覆盖真实双进程 UDP reentry。
- fanout gate 覆盖多 receiver 场景。
- CI/nightly 不只检查总量，也检查每段批宽，防止内部退化成逐笔路径。
- 门禁覆盖：
  - `max_product_ingress_submitted_per_tick`
  - `max_network_received_per_tick`
  - `max_queue_admitted_per_tick`
  - `max_aoem_batch_executed_per_tick`
  - `max_proof_items_per_tick`
  - `max_commit_items_per_tick`
  - `max_broadcast_tx_per_tick`
  - `nonempty_aoem_batch_ticks`
  - `nonempty_proof_ticks`
  - `nonempty_commit_ticks`

### 5.4 产品旁路封口

- `6dc8d57 feat(node): enforce pending-only native raw tx product mode`

完成内容：

- 新增 `NOVOVM_NATIVE_SEND_RAW_TRANSACTION_PIPELINE_ONLY=true`。
- `nov_sendRawTransaction` 产品模式只入 pending，不即时落账。
- `nov_execute` 传递 pending-only 标志。
- 新增锁定测试：pending-only raw tx 在 AOEM tick 前没有 receipt，AOEM tick 后产生 receipt。
- README 和 nightly gate 固定生产口径。

## 6. 验证摘要

本轮签收验证通过：

```text
cargo check -q -p novovm-node
cargo test -q -p novovm-node run_nov_send_raw_transaction --lib -- --test-threads=1
cargo test -q -p novovm-node native_execution_pipeline_ --bin novovm-node -- --test-threads=1
git diff --check
```

`git diff --check` 仅有仓库既有 CRLF warning，无空白错误。

### 6.1 Sustained gate

本地 sustained gate 通过：

```text
tx_count = 256
tick_budget = 32
execution_kernel = AOEM
aoem_concurrency_owner = AOEM_runtime
host_concurrency_policy = host_drives_lifecycle_only_no_rust_execution_scheduler
aoem_executed_total = 256
max_product_ingress_submitted_per_tick = 32
max_queue_admitted_per_tick = 32
max_aoem_batch_executed_per_tick = 32
max_proof_items_per_tick = 32
max_commit_items_per_tick = 32
max_broadcast_tx_per_tick = 32
nonempty_aoem_batch_ticks = 8
nonempty_proof_ticks = 8
nonempty_commit_ticks = 8
queue_pending_last = 0
included_canonical_total = 256
native_store_commit_model = dirty_sharded_atomic_batch_with_json_compat_snapshot
native_store_rocksdb_enabled = true
native_store_transactional_commit = true
```

### 6.2 Dual-node fanout gate

双节点 fanout gate 通过：

```text
sender product ingress = 16
sender max_product_ingress_submitted_per_tick = 4
sender max_aoem_batch_executed_per_tick = 4
sender max_proof_items_per_tick = 4
sender max_commit_items_per_tick = 4
sender max_broadcast_tx_per_tick = 8

receiver count = 2
receiver max_network_received_per_tick = 4
receiver max_queue_admitted_per_tick = 4
receiver max_aoem_batch_executed_per_tick = 4
receiver max_proof_items_per_tick = 4
receiver max_commit_items_per_tick = 4
receiver queue_pending_last = 0
receiver included_canonical_total = 16
```

## 7. 当前未完成项

以下内容不纳入本次 v1 签收，不应在产品叙事中提前宣称完成：

- 更长时间 native execution pipeline soak。
- 跨机器真实网络 soak。
- hostile network：丢包、乱序、重复包、延迟抖动、限速、断连。
- restart recovery：pipeline 中途重启后的 pending、receipt、canonical head 和 broadcast 恢复。
- production profile：正式硬件、磁盘、网络、tick budget、AOEM batch size 的压测报告。
- report artifact 归档和长期趋势统计。

## 8. 后续变更规则

- 不允许恢复 `nov_sendRawTransaction` / `nov_execute` 产品路径的即时落账。
- 不允许让 gateway、RPC 或 store 层重新拥有执行并发调度权。
- 不允许绕过 AOEM tick 直接生成产品 receipt/state。
- 不允许降低 ingress、admission、AOEM、proof、commit、broadcast 批宽门禁。
- 如需优化性能，应优先调整 AOEM batch、pipeline tick、network receive/broadcast budget 和 dirty commit keyspace，而不是在 Rust 主线重写执行调度器。

## 9. Tag

本签收建议固定 tag：

```text
native-execution-pipeline-lifecycle-v1 -> 6dc8d57
```

