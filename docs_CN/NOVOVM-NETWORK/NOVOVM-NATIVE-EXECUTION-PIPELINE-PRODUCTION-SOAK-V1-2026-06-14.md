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

## 5. 第二刀：RocksDB Recovery Gate

已新增独立 gate：

```text
cargo run -p novovm-node --bin supervm-native-pipeline-rocksdb-recovery-gate
```

该 gate 只验证恢复，不修改 lifecycle：

- 第一轮启动 `novovm-node`，使用 `NOVOVM_NATIVE_EXECUTION_STORE_BACKEND=rocksdb`。
- 交易仍走 pending-only product ingress 和 AOEM tick。
- dirty sharded atomic commit 写入 RocksDB。
- 子进程退出后，gate 重新打开同一路径 RocksDB。
- 验证 `semantic_head/current`。
- 验证 `semantic_head/by_height/{height}`。
- 验证 `snapshot_meta/current` 和 `snapshot_meta/{height}`。
- 验证 `receipt/{tx_hash}` materialized view。
- 验证 `receipt_by_height/{height}/{index}/{tx_hash}`。
- 验证 native execution materialized view 可重建。
- 第二轮同 store 无 ingress 重启 tick，验证 `duplicate_canonical_after_restart=0`。

报告：

```text
artifacts/native-pipeline/native-pipeline-rocksdb-recovery-report.json
schema = novovm-native-pipeline-rocksdb-recovery-report/v1
```

边界说明：

- 本刀签收 native execution RocksDB recovery。
- canonical body/head persistence 仍未签收；当前 canonical projection 仍是 network runtime state，不在 native execution RocksDB store 内。
- canonical body/head 持久化恢复需要下一刀单独补，不在本刀伪造 PASS。

## 6. 第三刀：Network Fault Injection Gate

已新增独立 gate：

```text
cargo run -p novovm-node --bin supervm-native-pipeline-network-fault-gate
```

该 gate 只验证网络扰动，不修改 lifecycle：

- 启动 receiver `novovm-node`，保持 `NOVOVM_NODE_MODE=native_execution_pipeline`。
- receiver 仍通过 UDP receive 进入 pending runtime。
- receiver 仍由 AOEM tick 产出 proof / receipt / dirty sharded commit / canonical included。
- gate 进程构造 native tx UDP `EvmNative::Transactions` 包。
- gate 进程注入 packet loss、duplicate、delay、reorder。
- 验证 duplicate packet 不造成重复 canonical included。
- 验证 reorder/delay 后 semantic head 仍可恢复。
- 验证 receipt index 与 canonical included unique tx 一致。
- 验证 queue pending / dropped / rejected 在预算内。

默认 smoke 配置：

```text
packet_loss_bps = 500
duplicate_bps = 10000
reorder_bps = 10000
delay_ms = 1
tx_count = 32
batch_budget = 8
max_unique_loss = 4
```

报告：

```text
artifacts/native-pipeline/native-pipeline-network-fault-injection-report.json
schema = novovm-native-pipeline-network-fault-injection-report/v1
```

本刀边界：

- 不声明 canonical body/head persistence。
- 不修改 pending-only 产品入口。
- 不绕过 AOEM tick。
- 不让 Rust host 成为执行并发调度器。
- 不新增账户/资产真相源。

## 7. 第四刀：Pending Queue Crash Recovery Gate

已新增独立 gate：

```text
cargo run -p novovm-node --bin supervm-native-pipeline-pending-crash-recovery-gate
```

该 gate 只验证 pending queue 崩溃语义，不修改 lifecycle：

- 当前 `network_runtime_native_pending` 是进程内 volatile runtime。
- crash 前只进入 pending、尚未 AOEM tick / dirty commit 的交易不保证恢复。
- 报告必须明确 `pending_policy = volatile`。
- 报告必须明确 `volatile_pending_not_recovered = true`。
- 已经 AOEM 执行、proof、dirty commit、canonical included 的交易通过 RocksDB native execution store 恢复。
- 重启后已 included 交易不得重复 AOEM 执行。
- 重启后不得生成重复 receipt。
- receipt/state 仍只能由 AOEM tick 生命周期产出。

默认场景：

```text
crash_before_aoem_tick:
  pending_submitted_count = 16
  pending_policy = volatile
  pending_lost_count = 16

crash_after_partial_commit:
  submitted = 32
  canonical_before_restart = 8
  pending_lost_count = 24
  duplicate_canonical_after_restart = 0
  duplicate_receipt_after_restart = 0
```

报告：

```text
artifacts/native-pipeline/native-pipeline-pending-crash-recovery-report.json
schema = novovm-native-pipeline-pending-crash-recovery-report/v1
```

本刀边界：

- 不实现 persistent pending queue。
- 不伪装 crash 前 pending 可恢复。
- 不声明 canonical body/head persistence。
- 不修改 pending-only 产品入口。
- 不绕过 AOEM tick。
- 不新增账户/资产真相源。

## 8. 第五刀：Remote Reentry Dedup Gate

已新增独立 gate：

```text
cargo run -p novovm-node --bin supervm-native-pipeline-remote-reentry-dedup-gate
```

该 gate 只验证远端重复重入去重，不修改 lifecycle：

- 同一批 native tx 多轮从 UDP transport 进入 receiver。
- receiver 首次运行必须只 canonical include 唯一 tx。
- duplicate packet / duplicate broadcast 不得生成重复 receipt。
- receiver 重启后，同一批已 receipted tx 再次进入 remote pending。
- pending tick 在 AOEM batch 选择前读取 native execution store receipt index。
- 已存在 receipt 的 tx 只丢弃 volatile pending，不再进入 AOEM batch。
- 重启后重复 reentry 不得推进 semantic head，不得新增 dirty commit。
- `canonical_body_head_recovery = not_claimed_by_this_gate`。

默认门禁指标：

```text
tx_count = 16
duplicate_rounds = 3
duplicate_received > 0
canonical_unique_included = 16
duplicate_canonical_included = 0
duplicate_receipt = 0
duplicate_dirty_commit = 0
semantic_head_extra_advance = 0
duplicate_canonical_after_restart = 0
duplicate_receipt_after_restart = 0
```

报告：

```text
artifacts/native-pipeline/native-pipeline-remote-reentry-dedup-report.json
schema = novovm-native-pipeline-remote-reentry-dedup-report/v1
```

本刀边界：

- 不实现 persistent pending queue。
- 不声明 canonical body/head persistence。
- 不修改 pending-only 产品入口。
- 不绕过 AOEM tick。
- 不新增账户/资产真相源。

## 9. 未完成项

以下仍待后续刀完成：

- Persistent canonical body/head recovery gate。
- Long-run production profile：真实 30min / 2h / overnight 报告归档。

## 10. 第六刀：Cross-machine UDP Soak

新增双角色工具：

```text
cargo run -p novovm-node --bin supervm-native-pipeline-cross-machine-udp-soak
```

机器 B receiver：

```powershell
$env:NOVOVM_NATIVE_PIPELINE_ROLE="receiver"
$env:NOVOVM_NATIVE_PIPELINE_LISTEN_ADDR="0.0.0.0:39001"
$env:NOVOVM_NATIVE_PIPELINE_MAX_TICKS="3600"
$env:NOVOVM_NATIVE_PIPELINE_REPORT_PATH="artifacts/native-pipeline/receiver-cross-machine-report.json"
cargo run -p novovm-node --bin supervm-native-pipeline-cross-machine-udp-soak
```

机器 A sender：

```powershell
$env:NOVOVM_NATIVE_PIPELINE_ROLE="sender"
$env:NOVOVM_NATIVE_PIPELINE_RECEIVER_ADDR="<machine-b-lan-ip>:39001"
$env:NOVOVM_NATIVE_PIPELINE_REPORT_PATH="artifacts/native-pipeline/sender-cross-machine-report.json"
cargo run -p novovm-node --bin supervm-native-pipeline-cross-machine-udp-soak
```

第一版边界：

- 不修改 lifecycle 主结构。
- 不扩展 pipeline 功能。
- 不做 fault injection。
- 不声明 canonical body/head persistence。
- 只验证两台真实机器 clean UDP 链路。

receiver 验收：

```text
received_unique == tx_count
canonical_unique_included == tx_count
duplicate_canonical_included == 0
queue_pending_last == 0
semantic_head_monotonic == true
receipt_index_consistent == true
aoem_concurrency_owner == AOEM_runtime
```

### 10.1 Cross-machine Clean UDP Soak Signoff

状态：

```text
Production Soak v1 / Cross-machine Clean UDP Soak: PASS
commit: f81d4c4
workspace: clean
```

实测拓扑：

```text
machine A sender:   192.168.71.118
machine B receiver: 192.168.71.117:39001
transport: UDP LAN
profile: clean network
tx_count: 32
```

实测链路：

```text
A sender
-> UDP LAN
-> B receiver
-> pending queue
-> AOEM batch
-> dirty sharded atomic commit
-> canonical included
-> receipt index
```

关键指标：

```text
received_unique = 32
aoem_executed_total = 32
canonical_unique_included = 32
queue_pending_last = 0
duplicate_canonical_included = 0
duplicate_receipt = 0
receipt_index_consistent = true
semantic_head_monotonic = true
recovery_ok = true
max_network_received_per_tick = 32
max_queue_admitted_per_tick = 8
```

skipped 诊断：

```text
skipped_missing_payload_total = 0
skipped_non_native_payload_total = 0
skipped_chain_mismatch_total = 0
```

结论：

- 两台真实机器之间的 clean UDP native payload 已经完成接收、AOEM 执行、落账、canonical included 和 receipt index 闭环。
- 本次不是本机双进程验证，而是真实 LAN sender/receiver 验证。
- skipped 统计确认本次没有 payload 缺失、非 native payload 或 chain mismatch。

本签收边界：

- 只签收 clean cross-machine UDP。
- 不签收 cross-machine packet loss / duplicate / delay / reorder。
- 不签收 canonical body/head recovery。
- 不修改 frozen lifecycle。
- 不改变 AOEM_runtime 作为执行并发 owner。
- 不允许 Rust host 自建执行调度器。

## 11. 第七刀：Cross-machine Fault UDP Soak

新增 fault 入口：

```text
cargo run -p novovm-node --bin supervm-native-pipeline-cross-machine-fault-udp-soak
```

实现边界：

- 不修改 frozen lifecycle。
- 不扩展 pipeline 功能。
- 只在 sender UDP 发包前注入 fault schedule。
- receiver 仍走 `UDP receive -> pending queue -> AOEM batch -> proof -> dirty commit -> canonical included`。
- 不声明 canonical body/head persistence。

默认 fault 参数：

```text
tx_count = 32
packet_loss_bps = 200
duplicate_bps = 3000
delay_ms = 20
reorder_bps = 1000
seed = 123
```

机器 B receiver：

```powershell
$env:NOVOVM_NATIVE_PIPELINE_ROLE="receiver"
$env:NOVOVM_NATIVE_PIPELINE_LISTEN_ADDR="0.0.0.0:39001"
$env:NOVOVM_NATIVE_PIPELINE_MAX_TICKS="3600"
$env:NOVOVM_NATIVE_PIPELINE_REPORT_PATH="artifacts/native-pipeline/receiver-cross-machine-fault-report.json"
cargo run -p novovm-node --bin supervm-native-pipeline-cross-machine-fault-udp-soak
```

机器 A sender：

```powershell
$env:NOVOVM_NATIVE_PIPELINE_ROLE="sender"
$env:NOVOVM_NATIVE_PIPELINE_RECEIVER_ADDR="<machine-b-lan-ip>:39001"
$env:NOVOVM_NATIVE_PIPELINE_TX_COUNT="32"
$env:NOVOVM_NATIVE_PIPELINE_FAULT_PACKET_LOSS_BPS="200"
$env:NOVOVM_NATIVE_PIPELINE_FAULT_DUPLICATE_BPS="3000"
$env:NOVOVM_NATIVE_PIPELINE_FAULT_DELAY_MS="20"
$env:NOVOVM_NATIVE_PIPELINE_FAULT_REORDER_BPS="1000"
$env:NOVOVM_NATIVE_PIPELINE_FAULT_SEED="123"
$env:NOVOVM_NATIVE_PIPELINE_REPORT_PATH="artifacts/native-pipeline/sender-cross-machine-fault-report.json"
cargo run -p novovm-node --bin supervm-native-pipeline-cross-machine-fault-udp-soak
```

sender report 必须包含：

```text
scheduled_packets
sent_packets
dropped_packets
duplicated_packets
delayed_packets
reordered_packets
sent_unique
```

receiver 验收：

```text
received_unique == tx_count
canonical_unique_included == tx_count
duplicate_canonical_included == 0
duplicate_receipt == 0
queue_pending_last == 0
semantic_head_monotonic == true
receipt_index_consistent == true
aoem_concurrency_owner == AOEM_runtime
host_concurrency_policy == host_drives_lifecycle_only_no_rust_execution_scheduler
```

本刀边界：

- 只签收轻量 cross-machine UDP fault。
- 不签收高丢包或长时间 hostile network。
- 不恢复 pending 持久化假设。
- 不把 canonical body/head recovery 混入本 gate。
- 不引入第二账本或第二资产真相源。

### 11.1 Cross-machine Fault UDP Soak Signoff

状态：

```text
Production Soak v1 / Cross-machine Fault UDP Soak: PASS
commit: c3c745a
workspace: clean
```

实测拓扑：

```text
machine A sender:   192.168.71.118
machine B receiver: 192.168.71.117:39001
transport: UDP LAN
profile: light packet fault
tx_count: 32
```

fault 参数：

```text
packet_loss_bps = 200
duplicate_bps = 3000
delay_ms = 20
reorder_bps = 1000
seed = 123
```

B receiver 实测指标：

```text
accepted = true
received_unique = 32
aoem_executed_total = 32
canonical_unique_included = 32
duplicate_canonical_included = 0
duplicate_receipt = 0
queue_pending_last = 0
semantic_head_monotonic = true
receipt_index_consistent = true
recovery_ok = true
max_network_received_per_tick = 34
max_queue_admitted_per_tick = 8
ticks = 1200
elapsed_ms = 177050
```

skipped 诊断：

```text
skipped_missing_payload_total = 0
skipped_non_native_payload_total = 0
skipped_chain_mismatch_total = 0
```

结论：

- 两台真实机器之间的轻量 UDP fault 链路已经完成 receiver 侧接收、AOEM 执行、落账、canonical included 和 receipt index 闭环。
- B 端 `max_network_received_per_tick = 34`，确认 receiver 侧观察到了超过 32 个 unique tx 的网络接收压力，且未产生重复 canonical included 或重复 receipt。
- skipped 统计确认本次没有 payload 缺失、非 native payload 或 chain mismatch。
- A 端 sender report 保存在机器 A 的 `artifacts/native-pipeline/sender-cross-machine-fault-report.json`，B 端签收只记录 receiver 侧最终账本一致性与网络接收结果。

本签收边界：

- 只签收轻量 cross-machine UDP fault。
- 不签收高丢包、长时间 hostile network 或 overnight soak。
- 不签收 canonical body/head recovery。
- 不恢复 pending 持久化假设。
- 不修改 frozen lifecycle。
- 不改变 AOEM_runtime 作为执行并发 owner。
- 不允许 Rust host 自建执行调度器。
