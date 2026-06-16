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

## 12. 第八刀：Cross-machine 5min Sustained Diagnostics

状态：

```text
Production Soak v1 / Cross-machine 5min Sustained Diagnostics: PASS
code baseline: 1e16291
workspace: clean
```

实测拓扑：

```text
machine A sender:   192.168.71.118
machine B receiver: 192.168.71.117:39001
transport: UDP LAN
profile: 5min sustained diagnostics
tx_count: 2400
duration_seconds: 300
tx_per_round: 8
round_interval_ms: 1000
```

本轮使用 diagnostics watchdog：

```text
NOVOVM_NATIVE_PIPELINE_PROGRESS_WATCHDOG_ENABLED = 1
NOVOVM_NATIVE_PIPELINE_PROGRESS_SAMPLE_INTERVAL_MS = 5000
NOVOVM_NATIVE_PIPELINE_PROGRESS_STALL_WINDOWS = 24
NOVOVM_NATIVE_PIPELINE_MEMORY_SAMPLE_ENABLED = 1
```

B receiver 最终验收：

```text
receiver_accepted = true
diagnostics_accepted = true
received_unique = 2400
canonical_unique_included = 2400
aoem_executed_total = 2400
ledger_lines = 2400
queue_pending_last = 0
duplicate_canonical_included = 0
duplicate_receipt = 0
semantic_head_monotonic = true
receipt_index_consistent = true
receiver exited cleanly = true
udp_39001_residual_process = false
```

诊断末尾样本：

```text
stable_progress_total = 2400
aoem_executed_total = 2400
semantic_ledger_mirror.line_count = 2400
semantic_ledger_mirror.bytes = 2241397
queue_pending_last = 0
queue_dropped_total = 0
queue_rejected_total = 0
ticks = 440
ticks_per_sec_x1000 = 693
diagnostics_sample_count = 124
```

本轮修正并固定的 sustained 主口径：

```text
primary:
  AOEM executed total
  RocksDB receipt_count / receipt index
  semantic ledger sequence / ledger lines
  queue_pending_last

observability-only:
  received_unique_total
  included_canonical_total
```

`received_unique_total` 和 `included_canonical_total` 来自 runtime pending 的当前保留视图，会受到 retention / cleanup 影响；它们继续作为观察项保留，但不再作为 sustained 主签收口径。长跑最终签收必须以 AOEM、RocksDB receipt/index、semantic ledger、queue drain 这些稳定累计事实为准。

本签收边界：

- 只签收 cross-machine 5min sustained diagnostics。
- 不签收 30min sustained。
- 不签收 2h / overnight soak。
- 不签收 hostile network。
- 不签收 canonical body/head recovery。
- 不改变 frozen lifecycle。
- 不改变 pending-only 产品入口。
- 不绕过 AOEM tick 生成 receipt/state。
- 不引入第二账本或第二资产真相源。

下一阶段：

```text
Production Soak v1 / Cross-machine 30min Sustained Diagnostics

tx_count = 14400
duration_seconds = 1800
tx_per_round = 8
round_interval_ms = 1000
```

30min 签收继续使用稳定累计口径：

```text
aoem_executed_total >= 14400
receipt_count >= 14400
semantic_sequence >= 14400
ledger_lines >= 14400
queue_pending_last = 0
duplicate_receipt = 0
duplicate_canonical_included = 0
semantic_head_monotonic = true
receipt_index_consistent = true
receiver exited cleanly = true
diagnostics_accepted = true
```

## 12. 第八刀：Cross-machine Sustained Soak Gate

新增 sustained 入口：

```text
cargo run -p novovm-node --bin supervm-native-pipeline-cross-machine-sustained-soak
```

目标：

- 两台真实机器持续运行。
- sender 按多轮持续发送唯一 native tx。
- receiver 继续走 frozen lifecycle：`UDP receive -> pending queue -> AOEM batch -> proof -> dirty commit -> canonical included`。
- 第一版只做 clean sustained，不叠加 hostile network。
- 不声明 canonical body/head persistence。

30min profile 建议参数：

```text
duration_seconds = 1800
tx_count = 14400
tx_per_round = 8
round_interval_ms = 1000
```

机器 B receiver：

```powershell
$env:NOVOVM_NATIVE_PIPELINE_ROLE="receiver"
$env:NOVOVM_NATIVE_PIPELINE_LISTEN_ADDR="0.0.0.0:39001"
$env:NOVOVM_NATIVE_PIPELINE_TX_COUNT="14400"
$env:NOVOVM_NATIVE_PIPELINE_MAX_TICKS="24000"
$env:NOVOVM_NATIVE_PIPELINE_SUSTAINED_DURATION_SECONDS="1800"
$env:NOVOVM_NATIVE_PIPELINE_SUSTAINED_TX_PER_ROUND="8"
$env:NOVOVM_NATIVE_PIPELINE_SUSTAINED_ROUND_INTERVAL_MS="1000"
$env:NOVOVM_NATIVE_PIPELINE_REPORT_PATH="artifacts/native-pipeline/receiver-cross-machine-sustained-30m-report.json"
cargo run -p novovm-node --bin supervm-native-pipeline-cross-machine-sustained-soak
```

机器 A sender：

```powershell
$env:NOVOVM_NATIVE_PIPELINE_ROLE="sender"
$env:NOVOVM_NATIVE_PIPELINE_RECEIVER_ADDR="<machine-b-lan-ip>:39001"
$env:NOVOVM_NATIVE_PIPELINE_TX_COUNT="14400"
$env:NOVOVM_NATIVE_PIPELINE_SUSTAINED_DURATION_SECONDS="1800"
$env:NOVOVM_NATIVE_PIPELINE_SUSTAINED_TX_PER_ROUND="8"
$env:NOVOVM_NATIVE_PIPELINE_SUSTAINED_ROUND_INTERVAL_MS="1000"
$env:NOVOVM_NATIVE_PIPELINE_REPORT_PATH="artifacts/native-pipeline/sender-cross-machine-sustained-30m-report.json"
cargo run -p novovm-node --bin supervm-native-pipeline-cross-machine-sustained-soak
```

report 关键字段：

```text
duration_seconds
tx_submitted_total
received_unique
aoem_executed_total
canonical_unique_included
queue_pending_last
queue_dropped_last
queue_rejected_last
duplicate_canonical_included
duplicate_receipt
semantic_head_monotonic
receipt_index_consistent
max_network_received_per_tick
max_queue_admitted_per_tick
max_aoem_batch_executed_per_tick
max_proof_items_per_tick
max_commit_items_per_tick
max_broadcast_tx_per_tick
```

receiver 验收：

```text
received_unique == expected_tx_total
canonical_unique_included == expected_tx_total
queue_pending_last == 0
duplicate_canonical_included == 0
duplicate_receipt == 0
semantic_head_monotonic == true
receipt_index_consistent == true
aoem_concurrency_owner == AOEM_runtime
host_concurrency_policy == host_drives_lifecycle_only_no_rust_execution_scheduler
```

本刀边界：

- 只签收持续运行稳定性。
- 不签收 hostile network。
- 不签收 canonical body/head recovery。
- 不改变 frozen lifecycle。
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

## 13. Cross-machine 30min Sustained Soak 当前状态：FAIL / Memory Retention Fix

状态：

```text
Production Soak v1 / Cross-machine 30min Sustained Soak: FAIL
fail_reason: process_working_set_exceeded
elapsed: ~972s / ~16.2min
working_set: 8,595,804,160 bytes
max_working_set: 8,589,934,592 bytes
```

失败前稳定累计口径：

```text
received_unique_total = 5024
aoem_executed_total = 3448
stable_progress_total = 3456
ledger_lines = 3456
queue_pending_last = 3992
queue_dropped_total = 0
queue_rejected_total = 0
```

结论：

- 执行链路仍在推进，失败不是 AOEM 生命周期断裂。
- 失败原因是 receiver 长跑内存保留与 pending backlog 增长。
- 30min sustained 不能签收，必须先修 receiver memory retention。

本轮修复边界：

- `dropped` pending tx 必须释放 payload，避免 already-receipted/drop 路径保留大 payload。
- pending snapshot 只克隆最终 limit 内条目，避免每 tick 对全量 pending state 做不必要克隆。
- cross-machine sustained 默认 AOEM batch budget 提升到 32，仍由 AOEM_runtime 执行，不引入 Rust host 执行调度器。
- diagnostics report 增加 bounded 样本保留指标和 working set 增量指标：
  - `diagnostics_samples_retained`
  - `diagnostics_samples_dropped`
  - `first_working_set_bytes`
  - `last_working_set_bytes`
  - `working_set_delta_total_bytes`
  - sample 内 `working_set_delta_per_minute`

下一轮验证顺序：

```text
1. 本地 deterministic tests
2. cross-machine 5min / 2400 tx
3. cross-machine 10min diagnostics
4. cross-machine 30min / 14400 tx
```

签收边界：

- 本节只记录 30min FAIL 和 memory retention fix。
- 不签收 30min sustained。
- 不签收 2h / overnight。
- 不签收 hostile network。
- 不签收 canonical body/head recovery。

### 13.1 Cross-machine 5min Sustained after `ef41432`

状态：

```text
Production Soak v1 / Cross-machine 5min Sustained after ef41432: PASS
code baseline: ef41432
profile: 2400 tx / 300s / 8 tx per round / 1000ms round interval
```

稳定累计口径：

```text
accepted = true
received_unique = 2400
canonical_unique_included = 2400
aoem_executed_total = 2400
receipt_count = 2400
ledger_lines = 2400
queue_pending_last = 0
duplicate_canonical_included = 0
duplicate_receipt = 0
semantic_head_monotonic = true
receipt_index_consistent = true
recovery_ok = true
```

内存诊断：

```text
diagnostics_accepted = true
fail_reason = None
diagnostics_samples_retained = 70
diagnostics_samples_dropped = 0
first_working_set_bytes = 11,231,232
last_working_set_bytes = 1,653,182,464
working_set_delta_total_bytes = 1,641,951,232
```

结论：

- 5min 功能闭环 PASS。
- 5min 内存平台化未证明。
- `ef41432` 修复了 payload retention 和 snapshot clone 放大后的短程运行稳定性，但 working set 仍从约 11MB 增长到约 1.65GB。
- 下一步必须先跑 10min diagnostics，不得直接跳 30min。

下一轮 10min diagnostics 参数：

```text
TX_COUNT = 4800
DURATION = 600s
TX_PER_ROUND = 8
ROUND_INTERVAL_MS = 1000
```

10min 重点观察：

- `working_set_delta_per_minute` 是否下降。
- `working_set` 是否趋于平台。
- `queue_pending_last` 是否最终清空。
- `diagnostics_samples_retained / diagnostics_samples_dropped` 是否受控。
- `semantic_ledger_mirror_bytes` 是否只作为磁盘 mirror 增长，而非全量内存驻留。

### 13.2 Cross-machine 10min Sustained after `ef41432`

状态：

```text
Production Soak v1 / Cross-machine 10min Sustained after ef41432: FUNCTIONAL PASS
Production Soak v1 / Cross-machine 10min Memory Plateau: FAIL / NOT PROVEN
code baseline: ef41432
profile: 4800 tx / 600s / 8 tx per round / 1000ms round interval
```

稳定累计口径：

```text
accepted = true
received_unique = 4800
canonical_unique_included = 4800
aoem_executed_total = 4800
receipt_count = 4800
semantic_sequence = 4800
queue_pending_last = 0
duplicate_canonical_included = 0
duplicate_receipt = 0
semantic_head_monotonic = true
receipt_index_consistent = true
recovery_ok = true
```

runtime retention 观察项：

```text
receiver_summary.included_canonical_total = 4128
receiver_summary.ingress_total_last = 4120
```

说明：

- `included_canonical_total / ingress_total_last` 属于 runtime retention 当前视图，会受 cleanup / retention 影响。
- sustained 主签收口径继续以 AOEM executed、RocksDB receipt/index、semantic ledger sequence 和 `queue_pending_last` 为准。

内存诊断：

```text
diagnostics_accepted = true
fail_reason = None
diagnostics_samples_retained = 144
diagnostics_samples_dropped = 0
first_working_set_bytes = 11,235,328
last_working_set_bytes = 3,191,017,472
working_set_delta_total_bytes = 3,179,782,144

~3min  = ~913MB
~6min  = ~1.71GB
~9min  = ~2.48GB
~10min = ~3.19GB
```

结论：

- 10min 功能闭环 PASS。
- 10min 内存平台化 FAIL / NOT PROVEN。
- working set 基本呈线性增长，不能进入 30min / 14400 tx sustained。
- 下一刀必须收敛 sustained receiver/report/probe/runtime-view 的内存保留，不修改 frozen lifecycle，不修改 AOEM owner，不修改 UDP 行为。

### 13.3 Sustained Report / Probe Memory Retention Fix

目标：

```text
Production Soak v1 / Sustained Report-Probe Memory Retention Fix
scope: report / recovery probe / runtime-view retention only
```

修复边界：

- final report 不得保留全量 `tx_hash`、receipt key、included list。
- recovery probe 不得把全量 receipt key 列表写入 report；默认只输出 count、digest、少量 sample。
- receiver summary report 只输出 aggregate 字段，不输出全量 tick detail / tx detail。
- sender report 不输出全量 `sent_by_hash` map，改为 count、digest、sample。
- sustained diagnostics 继续保留 bounded samples，不允许嵌入全量 pending / receipt / tx list。

新增/固定诊断字段：

```text
report_tx_hash_list_len
report_receipt_key_list_len
recovery_probe_materialized_key_count
receipt_hash_digest
receipt_hash_samples
receipt_hashes_omitted
receipt_index_missing_count
```

下一轮验证顺序：

```text
1. 本地 sustained 256 tx
2. 本地 sustained 2400 tx
3. Cross-machine 5min / 2400 tx
4. Cross-machine 10min / 4800 tx，观察 working set slope
5. 只有 10min 后半程内存增长趋缓，才允许进入 30min / 14400 tx
```

签收边界：

- 本节只记录 report/probe retention 修复。
- 不签收 30min sustained。
- 不签收 memory plateau，必须由下一轮 10min 数据证明。
- 不签收 hostile network、2h / overnight、canonical body/head recovery。

### 13.4 Cross-machine Sustained Tail Repair Gate

触发背景：

```text
Production Soak v1 / Cross-machine 5min Sustained after ac66259: FAIL
expected = 2400
received_unique = 2399
canonical_unique_included = 2399
aoem_executed_total = 2399
ledger_lines = 2399
queue_pending_last = 0
receiver_state = waiting_for_sender
```

结论：

- AOEM、dirty commit、receipt index 和 semantic ledger 对已收到交易处理正确。
- 失败点不是执行链路，而是裸 UDP 一次发送存在交付缺口。
- sustained gate 不能建立在 `UDP send once == reliable delivery` 的假设上。

本轮修复：

- cross-machine sustained sender 增加 tail repair 阶段。
- 正常发送结束后，sender 按稳定 tx hash / sequence 重新发送全量 fixture tx 若干轮。
- receiver 继续依赖既有 remote reentry dedup / receipt dedup，确保重复补发不重复 AOEM execution、不重复 receipt、不重复 dirty commit。
- receiver diagnostics 不再在 `pending_count = 0` 且等待 sender 后续轮次时误判 `canonical_progress_stall`。
- receiver 增加最大等待窗口：`duration + tail_repair_budget + 60s`，超过预算仍未达标则明确 FAIL，不无限等待。
- receiver 最大等待窗口只在 `pending_count = 0` 且仍缺交易时触发；如果 tail repair 已把包交付到 receiver 且本地还有 pending，需要继续 drain，由 stall watchdog 判断是否真正卡死。

尾部修复后新增观察：

```text
Production Soak v1 / Cross-machine 5min after tail repair: FAIL
expected = 2400
received_unique = 2400
canonical_unique_included = 2312
aoem_executed_total = 2312
ledger_lines = 2312
queue_pending_last = 88
fail_reason = receiver_expected_tx_timeout
```

结论：

- tail repair 已解决裸 UDP 少包问题，receiver 收到 2400/2400。
- 新失败点是 repair 尾部进入 receiver 后，pending drain grace 不足，receiver 在仍有 `pending=88` 时被 timeout 杀掉。
- 修复口径是不把 `pending_count > 0` 的本地 drain 阶段当成缺包 timeout；如果 drain 真卡住，应由 `canonical_progress_stall` 判断。

新增 sender 参数：

```text
NOVOVM_NATIVE_PIPELINE_TAIL_REPAIR_ENABLED=1
NOVOVM_NATIVE_PIPELINE_TAIL_REPAIR_ROUNDS=3
NOVOVM_NATIVE_PIPELINE_TAIL_REPAIR_INTERVAL_MS=1000
```

新增/更新报告字段：

```text
tail_repair.enabled
tail_repair.rounds_configured
tail_repair.interval_ms
tail_repair.repair_rounds_used
tail_repair.initial_sent_total
tail_repair.repair_sent_total
tail_repair.repair_scheduled_total
tail_repair.tail_repair_success
waiting_for_sender
max_elapsed_ms
```

签收边界：

- 本节只修 sustained gate 网络交付尾部缺口。
- 不修改 frozen lifecycle。
- 不改变 AOEM_runtime 并发 owner。
- 不绕过 AOEM tick 生成 receipt/state。
- 不把 canonical body/head recovery 混入本 gate。
- 不签收 5min / 10min sustained，必须由下一轮 A/B 实测重新证明。

### 13.5 Cross-machine 5min Sustained after Tail Repair Signoff

状态：

```text
Production Soak v1 / Cross-machine 5min Sustained after Tail Repair: PASS
Memory Plateau: NOT SIGNED
code baseline: 556e361
profile: 2400 tx / 300s / 8 tx per round / 1000ms round interval
tail_repair_rounds = 3
tail_repair_interval_ms = 1000
```

B receiver final report：

```text
accepted = true
tx_count = 2400
received_unique = 2400
canonical_unique_included = 2400
aoem_executed_total = 2400
receipt_count = 2400
semantic_sequence = 2400
queue_pending_last = 0
duplicate_canonical_included = 0
duplicate_receipt = 0
semantic_head_monotonic = true
receipt_index_consistent = true
recovery_ok = true
violations = none
receiver_clean_exit = true
```

Diagnostics：

```text
diagnostics_accepted = true
fail_reason = None
diagnostics_samples_retained = 75
diagnostics_samples_dropped = 0
first_working_set_bytes = 11,251,712
last_working_set_bytes = 1,705,390,080
working_set_delta_total_bytes = 1,694,138,368
ledger_lines = 2400
ledger_bytes = 2,241,349
```

结论：

- cross-machine 5min sustained 功能闭环 PASS。
- tail repair 解决裸 UDP 缺包；pending drain 修复解决 repair 后 pending 未清空就 timeout。
- 本签收只覆盖 5min 功能闭环。
- 内存仍从约 11MB 增长到约 1.7GB，memory plateau 不签。

下一步：

```text
Cross-machine 10min / 4800 tx
目的：继续验证功能闭环，同时观察 working set slope 是否趋缓。
```

签收边界：

- 不签收 30min sustained。
- 不签收 2h / overnight。
- 不签收 hostile network。
- 不签收 canonical body/head recovery。
- 不签收 memory plateau。

### 13.6 Cross-machine 10min Sustained after Tail Repair Signoff

状态：

```text
Production Soak v1 / Cross-machine 10min Sustained after Tail Repair: FUNCTIONAL PASS
Memory Plateau: FAIL / NOT SIGNED
profile: 4800 tx / 600s / 8 tx per round / 1000ms round interval
tail_repair_rounds = 3
tail_repair_interval_ms = 1000
```

B receiver final report：

```text
accepted = true
tx_count = 4800
received_unique = 4800
canonical_unique_included = 4800
aoem_executed_total = 4800
receipt_count = 4800
semantic_sequence = 4800
queue_pending_last = 0
duplicate_canonical_included = 0
duplicate_receipt = 0
semantic_head_monotonic = true
receipt_index_consistent = true
recovery_ok = true
violations = none
receiver_clean_exit = true
```

Diagnostics：

```text
diagnostics_accepted = true
fail_reason = None
diagnostics_samples_retained = 145
diagnostics_samples_dropped = 0
first_working_set_bytes = 11,243,520
last_working_set_bytes = 3,206,868,992
working_set_delta_total_bytes = 3,195,625,472
ledger_lines = 4800
ledger_bytes = 4,484,117
```

内存采样趋势：

```text
~0s    stable=0     working_set=10.7MB
~186s  stable=1112  working_set=848.0MB   delta=270.1MB/min
~373s  stable=2392  working_set=1613.8MB  delta=258.6MB/min
~560s  stable=3640  working_set=2359.8MB  delta=252.1MB/min
~742s  stable=4760  working_set=3058.3MB  delta=246.8MB/min
```

结论：

- cross-machine 10min sustained 功能闭环 PASS。
- tail repair + pending drain 已经满足 4800/4800 功能口径。
- 内存仍近似线性增长，working set slope 只轻微下降，没有平台化。
- 不能进入 30min / 14400 tx sustained。

下一步：

```text
Sustained Receiver Memory Retention Fix Round 2
目标：继续定位 pending/runtime/progress/ledger/report/probe 中仍然随 tx 数增长的内存保留源。
```

签收边界：

- 只签 10min 功能闭环。
- 不签 memory plateau。
- 不签 30min sustained。
- 不签 2h / overnight。
- 不签 hostile network。
- 不签 canonical body/head recovery。

### 13.7 Sustained Receiver Memory Retention Fix Round 2

状态：

```text
Production Soak v1 / Sustained Receiver Memory Retention Fix Round 2: DONE
Cross-machine 10min Sustained: FUNCTIONAL PASS remains valid
Memory Plateau: NOT SIGNED
30min sustained: still blocked until 10min memory slope improves
```

背景：

```text
4800 tx / 10min 后 receiver working set 从约 10.7MB 增长到约 3.06GB。
功能口径已经 4800/4800 PASS，但内存接近线性增长，不能进入 30min。
```

本轮修复：

```text
1. pipeline tick report 不再嵌入完整 tick_result。
   - raw_txs omitted
   - selected_tx_hashes omitted
   - canonical_projection omitted
   - receipt detail omitted
   - aggregate 仍保留 selected_count / skipped / dirty_set / backend_status

2. ingress_drive / broadcast_drive 中 tx_hashes 改为 count + omitted 标记。

3. egress broadcast_candidate_hashes 改为 count + omitted 标记。

4. native execution pipeline 模式启用专用 runtime retention budget：
   - pending_tx_canonical_retain_depth 默认 4
   - pending_tx_tombstone_retention_max 默认 256
   - runtime_pending_tx_snapshot_limit 默认 512
   - 只影响 native_execution_pipeline 模式
   - 不改变 Ethereum mainnet long-sync 默认预算
```

保持不变的边界：

```text
AOEM_runtime 仍是执行并发 owner。
Rust host 仍只驱动生命周期。
pending-only 产品入口不变。
AOEM tick / proof / dirty commit / canonical lifecycle 不变。
canonical body/head recovery 仍不由本 gate 签收。
```

下一轮验证顺序：

```text
1. Cross-machine 5min / 2400 tx
2. Cross-machine 10min / 4800 tx
3. 只有 10min 后半程 working_set slope 明显下降，才允许重新进入 30min / 14400 tx
```

本节不是 memory plateau 签收；它只是记录 Round 2 内存保留修复已经写入。

### 13.8 Sustained Receiver Pending Scan Retention Fix

状态：

```text
Production Soak v1 / Sustained Receiver Pending Scan Retention Fix: DONE
Cross-machine 5min function: remains PASS
Memory Plateau: NOT SIGNED
30min sustained: still blocked until 5min/10min memory slope improves
```

触发原因：

```text
上一轮 2400 tx 功能 PASS，但 receiver 出现异常扫描：
- skipped_ineligible_stage_total = 161244
- skipped_already_receipted_total = 631
- queue_dropped_last = 631

判断：report/probe 不再是主要问题；pending/runtime current view 中的 historical entries
仍被 AOEM admission 每 tick 反复扫描。
```

本轮修复：

```text
1. 新增 network runtime active pending snapshot：
   snapshot_network_runtime_native_active_pending_txs_v1

2. AOEM pending batch admission 改为只读取 active execution stages：
   - Seen
   - Pending
   - Propagated
   - ReorgedBackToPending

3. Dropped / Rejected / IncludedCanonical 不再进入 AOEM candidate scan。

4. duplicate remote/native reentry 如果命中 IncludedCanonical：
   - 不重新进入 Pending
   - 不重新保留 payload
   - payload map 中对应 entry 被移除

5. diagnostics / summary 增加 active/historical 区分：
   - queue_active_pending_last
   - queue_historical_pending_last
   - active_pending_count
   - historical_pending_count
   - current_view_received_retained
   - current_view_included_retained
   - current_view_dropped_retained
   - queue_dropped_last_active
   - queue_dropped_total_cumulative
```

保持不变的边界：

```text
AOEM_runtime 仍是执行并发 owner。
Rust host 仍只驱动生命周期。
pending-only 产品入口不变。
AOEM tick / proof / dirty commit / canonical lifecycle 不变。
不改变 Ethereum mainnet long-sync 默认 retention 语义。
canonical body/head recovery 仍不由本 gate 签收。
```

验证状态：

```text
cargo check -q -p novovm-node --bins: PASS
cargo check -q -p novovm-network: PASS
cargo test -q -p novovm-node native_execution_pipeline_ --bin novovm-node -- --test-threads=1: PASS
cargo test -q -p novovm-network --lib -- --test-threads=1: PASS
cargo test -q -p novovm-node --lib -- --test-threads=1: PASS
git diff --check: only CRLF warnings
```

下一轮验证顺序：

```text
1. Cross-machine 5min / 2400 tx
   目标：功能 PASS，skipped_ineligible_stage_total 不再出现 16 万级重复扫描。

2. Cross-machine 10min / 4800 tx
   目标：观察 working_set slope 是否明显下降。

3. 只有 10min 后半程 working_set slope 明显下降，才允许重新进入 30min / 14400 tx。
```

本节不是 memory plateau 签收；它只是记录 pending scan / cleanup / retention 修复已经写入。

### 13.9 Sustained Receiver Historical Retention Fix

状态：

```text
Production Soak v1 / Sustained Receiver Historical Retention Fix: DONE
Cross-machine 5min function: remains PASS from previous run
Memory Plateau: NOT SIGNED
30min sustained: still blocked until 5min/10min memory slope improves
```

触发原因：

```text
Pending active scan 已收干净：
- skipped_ineligible_stage_total = 0
- queue_dropped_last_active = 0

但 receiver runtime current view 仍保留 historical entries：
- historical_pending = 327
- included_retained = 160
- dropped_retained = 167

判断：AOEM admission 不再是问题；剩余问题在 historical current view /
tombstone / included+dropped retention。
```

本轮修复：

```text
1. 新增 network runtime historical pending compaction：
   compact_network_runtime_native_pending_tx_history_v1

2. native_execution_pipeline report 构建时执行 bounded historical compaction。

3. IncludedCanonical / IncludedNonCanonical / Dropped / Rejected historical entries：
   - 移出 runtime current view
   - 写入 bounded tombstone
   - 释放 payload map 中对应 payload

4. 新增 summary / diagnostics 字段：
   - historical_compacted_total
   - historical_payload_bytes_freed
   - tombstone_retained_count
   - tombstone_evicted_count
   - historical_pending_after_compaction
   - included_retained_after_compaction
   - dropped_retained_after_compaction
   - runtime_current_view_bytes_estimate

5. 新增回归测试：
   native_pending_history_compaction_evicts_final_state_and_payloads
```

本地 smoke 验证：

```text
profile: local sustained smoke
tx_count = 256
accepted = true
aoem_executed_total = 256
receipt_count = 256
queue_pending_last = 0
skipped_ineligible_stage_total = 0
historical_compacted_total = 253
historical_payload_bytes_freed = 58321
queue_historical_pending_last = 32
historical_pending_after_compaction = 32
runtime_current_view_bytes_estimate = 8192
duplicate_canonical_included = 0
duplicate_receipt = 0
```

保持不变的边界：

```text
AOEM_runtime 仍是执行并发 owner。
Rust host 仍只驱动生命周期。
pending-only 产品入口不变。
AOEM tick / proof / dirty commit / canonical lifecycle 不变。
不改变 Ethereum mainnet long-sync 默认 retention 语义。
canonical body/head recovery 仍不由本 gate 签收。
```

验证状态：

```text
cargo check -q -p novovm-node --bins: PASS
cargo check -q -p novovm-network: PASS
cargo test -q -p novovm-node native_execution_pipeline_ --bin novovm-node -- --test-threads=1: PASS
cargo test -q -p novovm-network --lib -- --test-threads=1: PASS
cargo test -q -p novovm-node --lib -- --test-threads=1: PASS
local sustained 256 tx smoke: PASS
git diff --check: pending
```

下一轮验证顺序：

```text
1. Cross-machine 5min / 2400 tx
   目标：功能 PASS，skipped_ineligible_stage_total 继续保持低值，
   queue_historical_pending_last 应被 bounded cap 控制。

2. Cross-machine 10min / 4800 tx
   目标：观察 working_set slope 是否明显下降。

3. 只有 10min 后半程 working_set slope 明显下降，才允许重新进入 30min / 14400 tx。
```

本节不是 memory plateau 签收；它只是记录 historical current view retention 修复已经写入。

### 13.10 Receiver RocksDB LOCK Conflict Fix and 5min Functional Re-run

状态：

```text
Receiver RocksDB LOCK Conflict Fix: PASS
Cross-machine 5min Sustained Functional: PASS
Commit: 374bdac
Memory Plateau: NOT SIGNED
10min / 30min sustained: NOT SIGNED
```

触发原因：

```text
上一轮 cross-machine 5min attribution run 失败时，receiver 子进程以 exit code 1
退出，外层 exit forensics 捕获到 stderr：

open nov native execution rocksdb failed
Failed to create lock file ... rocksdb/LOCK

判断：父进程 diagnostics 在 receiver child 运行期间打开同一个 RocksDB
路径做 memory probe，抢占了 child 正在使用的 RocksDB LOCK，导致 receiver
被诊断系统自身打死。
```

修复内容：

```text
1. receiver child 运行期间，父进程不再打开 child 持有的 RocksDB path。

2. live receiver diagnostics 使用 skipped probe 记录：
   rocksdb_probe_skipped_reason = live_receiver_child_holds_lock

3. child 退出后，final / recovery probe 才允许重新打开 RocksDB。

4. exit forensics 增加 stderr tail：
   child_stderr_tail

5. stderr 中出现 RocksDB LOCK 冲突时，fail_reason 细化为：
   rocksdb_lock_conflict

6. 不改变 frozen lifecycle，不改变 AOEM_runtime owner，不改变 pending-only 入口。
```

验证结果：

```text
profile: cross-machine 5min sustained functional re-run
sender: 192.168.71.118
receiver: 192.168.71.117:39001
tx_count = 2400
duration_seconds = 300
tx_per_round = 8
round_interval_ms = 1000

accepted = true
received_unique = 2400
aoem_executed_total = 2400
receipt_count = 2400
semantic_sequence = 2400
queue_pending_last = 0
duplicate_canonical_included = 0
duplicate_receipt = 0
receipt_index_consistent = true
semantic_head_monotonic = true
receiver clean exit = true

child_exit_code = 0
fail_reason = normal_pass
stderr_tail = empty
final_report_written = true
diagnostics_report_written = true
```

保持不变的边界：

```text
AOEM_runtime 仍是执行并发 owner。
Rust host 仍只驱动生命周期。
pending-only 产品入口不变。
AOEM tick / proof / dirty commit / canonical lifecycle 不变。
canonical body/head recovery 仍不由本 gate 签收。
```

本节签收范围：

```text
签收：
- Receiver RocksDB LOCK Conflict Fix
- Cross-machine 5min sustained functional pass
- Receiver final report / diagnostics / exit forensics 正常写出

不签收：
- Memory plateau
- Cross-machine 10min sustained
- Cross-machine 30min sustained
- 2h / overnight sustained
- hostile network
- canonical body/head recovery
```

下一步：

```text
进入 Allocator / Native Heap Working Set Attribution。

目标：解释 remaining working set / unattributed working set 的来源，区分：
- Rust retained objects
- AOEM/native heap
- RocksDB native allocation / cache
- allocator fragmentation
- Windows working set not returned

在 memory plateau 未签收前，不进入 30min sustained 签收。
```

### 13.11 Allocator / Native Heap Working Set Attribution

状态：

```text
Allocator / Native Heap Working Set Attribution: READY
Commit: 67dc254
Workspace: clean
Memory Plateau: NOT SIGNED
```

目的：

```text
继续解释 cross-machine 5min / 2400 functional PASS 后 receiver
仍出现的高 private / working set。

本阶段不修改 frozen lifecycle，不改变 AOEM_runtime 并发 owner，
不改变 pending-only 产品入口，不把 memory plateau 伪签为 PASS。
```

新增归因字段：

```text
process_working_set_bytes
process_private_bytes
process_virtual_bytes
process_handle_count
process_thread_count
rust_estimated_retained_bytes
rocksdb_total_estimated_memory_bytes
native_heap_unattributed_bytes
working_set_bytes_per_1000_tx
private_bytes_per_1000_tx
native_heap_unattributed_bytes_per_1000_tx
allocator_fragmentation_suspected
working_set_not_returned_suspected
```

cross-machine 5min 归因结果：

```text
accepted = true
received_unique = 2400
aoem_executed_total = 2400
receipt_count = 2400
semantic_sequence = 2400
queue_pending_last = 0
child_exit_code = 0
fail_reason = normal_pass

peak_live_working_set ≈ 1.65GB
peak_live_private ≈ 1.69GB
peak_live_native_heap_unattributed ≈ 1.68GB
allocator_fragmentation_suspected = true
working_set_not_returned_suspected = false
```

判断：

```text
post-exit 0 sample 已排除。
pending/history retention 已收敛。
RocksDB live probe LOCK conflict 已排除。
working set not returned 不是主因，因为 private bytes 同步偏高。

剩余大头为 private/native heap unattributed。
```

### 13.12 Diagnostics Live Memory Summary Fix

状态：

```text
Diagnostics Live Memory Summary Fix: PASS
Commit: ec6a416
Workspace: clean
Memory Plateau: NOT SIGNED
```

修复内容：

```text
final diagnostics report 区分：
- last_sample_any
- last_live_child_sample
- peak_live_child_sample
- post_exit_sample

内存 summary 以 live child sample 为准，不允许 child 退出后的 0
覆盖运行中的真实峰值。
```

验证结果：

```text
memory_summary_source = live_peak
post_exit_sample_present = true/false
post_exit_working_set_zeroed 不再影响 live peak 判断

Cross-machine 5min / 2400 functional: PASS
Receiver clean exit: PASS
Diagnostics live summary: PASS
Memory Plateau: NOT SIGNED
```

边界：

```text
本节只签 diagnostics summary 可信度。
不签 memory plateau。
不进入 30min。
```

### 13.13 Native Heap Source Isolation

状态：

```text
Native Heap Source Isolation: READY
Commit: 747bbe8
Workspace: clean
Memory Plateau: NOT SIGNED
```

目的：

```text
把 private/native heap unattributed 继续拆到 AOEM / proof /
receipt / canonical / UDP / decode / JSON / Vec capacity 等阶段估算。
```

新增字段：

```text
summary_stage_estimated_bytes_total
summary_unknown_native_heap_source
summary_large_allocation_suspected_stage
summary_native_heap_source_isolation_confidence

aoem_runtime_estimated_bytes
aoem_batch_input_bytes
aoem_batch_output_bytes
proof_projection_bytes
receipt_projection_bytes
canonical_projection_bytes
udp_receive_buffer_bytes
decode_buffer_bytes
json_serialization_buffer_bytes
tick_vec_capacity_bytes
batch_vec_capacity_bytes
```

cross-machine 5min 结果：

```text
functional = PASS
peak_live_working_set_bytes = 1,641,803,776
peak_live_private_bytes = 1,682,796,544
peak_live_native_heap_unattributed_bytes = 1,673,638,539
summary_stage_estimated_bytes_total = 8,445,952
summary_unknown_native_heap_source = true
summary_large_allocation_suspected_stage = unknown_native_heap_source
summary_native_heap_source_isolation_confidence = low_unknown_dominates
allocator_fragmentation_suspected = true
working_set_not_returned_suspected = false
```

判断：

```text
显式阶段估算只能解释约 8.4MB。
约 1.67GB private/native heap 仍在诊断盲区。
不能签 memory plateau。
不能进入 30min sustained。
```

### 13.14 Native Heap Stage Toggle Bisect Gate

状态：

```text
Native Heap Stage Toggle Bisect Gate: READY / DIAGNOSTIC PASS
Commit: c72fbe4
Workspace: clean
Memory Plateau: NOT SIGNED
```

新增 gate：

```text
bin: supervm-native-pipeline-memory-bisect-gate
artifact: artifacts/native-pipeline/native-pipeline-memory-bisect-report.json
```

probe-only toggles：

```text
NOVOVM_NATIVE_PIPELINE_MEMORY_PROBE_DISABLE_PROOF_PROJECTION
NOVOVM_NATIVE_PIPELINE_MEMORY_PROBE_DISABLE_RECEIPT_PROJECTION
NOVOVM_NATIVE_PIPELINE_MEMORY_PROBE_DISABLE_CANONICAL_PROJECTION
NOVOVM_NATIVE_PIPELINE_MEMORY_PROBE_DISABLE_BROADCAST_REPORTING
NOVOVM_NATIVE_PIPELINE_MEMORY_PROBE_DISABLE_RECOVERY_PROBE
NOVOVM_NATIVE_PIPELINE_MEMORY_PROBE_DISABLE_DIAGNOSTICS_SAMPLES
NOVOVM_NATIVE_PIPELINE_MEMORY_PROBE_DISABLE_JSON_REPORT_SERIALIZATION
NOVOVM_NATIVE_PIPELINE_MEMORY_PROBE_DISABLE_SEMANTIC_LEDGER_MIRROR
NOVOVM_NATIVE_PIPELINE_MEMORY_PROBE_MINIMAL_AOEM_RESULT
NOVOVM_NATIVE_PIPELINE_MEMORY_PROBE_NO_RECEIPT_BODY_CACHE
```

边界：

```text
toggle 仅用于 memory attribution gate。
报告显式标记 toggle_applied_to_execution = false。
probe_only_not_functional toggle 不能用于签 production functional。
不修改 frozen lifecycle。
不改变 AOEM_runtime owner。
不改变 pending-only 产品入口。
不签 memory plateau。
```

本地 smoke：

```text
tx_count = 32
accepted = true
baseline_peak_private_bytes = 27,590,656
suspected_stage = allocator_or_external_native_heap_suspected
best_reduction_percent = 1
confidence = low_no_toggle_reduction_over_30_percent
```

结论：

```text
上层可开关阶段没有明显降低 private/native heap。

当前更可能方向：
1. Windows/MSVC allocator heap fragmentation / arena retention
2. RocksDB/native library 内部未被现有 probe 捕捉的 allocation
3. AOEM FFI / runtime 内部 native heap
4. 底层 buffer/arena 每 batch 分配后释放给 allocator，但 allocator 保留高水位
5. child process runtime cache / global cache 未被诊断覆盖

下一步进入 External Memory Profiler Capture。
```

### 13.15 External Memory Profiler Capture Guide

状态：

```text
External Memory Profiler Capture Guide: READY
Memory Plateau: NOT SIGNED
30min sustained: BLOCKED
```

目的：

```text
代码内估算和 stage toggle 已经无法解释约 1.6GB private/native heap。
下一步必须使用进程级外部工具确认内存大类和分配路径。
```

Artifact 目录：

```text
artifacts/native-pipeline/memory-profiler/
```

采样窗口：

```text
cross-machine 5min / 2400 receiver 运行中采样：
- 约 1000 tx
- 约 2000 tx
- 约 2400 tx / receiver 退出前
```

VMMap 判读规则：

```text
Heap / Private Data 大：
  优先查 allocator/native heap、AOEM FFI、Rust/native buffer arena。

Mapped File 大：
  优先查 RocksDB mmap、block cache、file mapping。

Stack 大：
  优先查 thread_count / 线程泄漏。

Handle/thread 增长：
  优先查 socket、file handle、child process、runtime resource leak。

Image / Shareable 大：
  通常不是本轮 private heap 主因，记录即可。
```

WPR/WPA 使用边界：

```text
VMMap 先确认大类。
如果大头在 Heap / Private Data，再用 WPR/WPA 抓 heap allocation stack。
不要把 allocator_or_external_native_heap_suspected 当最终结论。
```

保持不变的边界：

```text
不修改 frozen lifecycle。
不改变 AOEM_runtime owner。
不改变 pending-only 产品入口。
不签 memory plateau。
不进入 30min sustained。
不把 canonical body/head recovery 混入本轮。
```
