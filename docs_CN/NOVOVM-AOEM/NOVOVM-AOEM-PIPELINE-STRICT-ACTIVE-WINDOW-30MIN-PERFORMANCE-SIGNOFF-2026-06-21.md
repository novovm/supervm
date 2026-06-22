# NOVOVM AOEM-owned Pipeline Strict Active-window 30min Performance Signoff

## Status

```text
pipeline AOEM-owned strict active-window 30min / 14400 performance: PASS
Commit under test: 389f747
Transport: NovoRUDP
Pipeline: AOEM Runtime Worker
```

This signoff covers the real cross-machine A/B pipeline run after the active
performance-window definition was fixed.

The performance window is:

```text
first_tx_seen -> final_close
```

It is not:

```text
receiver process start -> final_close
```

Receiver startup time before the first transaction arrives is tracked as
diagnostic wait time, not as receiver active execution performance.

## Signed Scope

- Pipeline AOEM-owned 30min / 14400 correctness.
- Strict active-window 30min performance.
- NovoRUDP transport.
- AOEM Runtime Worker production callsite.
- Network Receiver does not call production `tx_ingress`.
- AOEM-owned execution/proof/persistence path.
- A/B final PASS.
- `fallback = false`.
- `receiver_done_ack` observed by A.
- `missing = 0`.

## B Receiver Evidence

```text
accepted = true
receiver_exit_phase = completed
canonical_unique_included_total = 14400
ledger_completed_count = 14400
ledger_durable_missing_count = 0
queue_pending_last = 0
```

AOEM-owned pipeline:

```text
tx_ingress_real_callsite = aoem_runtime_worker
tx_ingress_called_by_network_receiver = false
tx_ingress_called_by_aoem_runtime_worker = true
fallback = false
```

Active performance window:

```text
performance_window_start_source = first_tx_seen
performance_window_elapsed_ms = 1630657
active_close_tx_count = 14400
active_close_tps_x1000 = 8830
strict_30min_performance_pass = true
total_elapsed_exceeded_due_to_pre_first_tx_wait = true
```

Human-readable active window:

```text
active_window ~= 27.18 minutes
active_close_tps ~= 8.83 TPS
```

ACK:

```text
receiver_done = true
receiver_ack_send_ok_count = 1
```

## A Sender Evidence

```text
accepted = true
sender_completed = true
sender_hard_timeout_reached = false
absolute_sender_timeout_reached = false
sender_repair_no_progress_timeout_reached = false
fail_reason = null
send_failed_count = 0
send_would_block_count = 0
send_retry_count = 0
transport_profile = novorudp
tail_repair_success = true
tail_repair_completion_reason = receiver_done_ack
latest_ack_missing_count = 0
latest_ack_receiver_done = true
receiver_final_done = true
receiver_final_missing_count = 0
final_missing_count = 0
repair_send_failed_count = 0
```

## Interpretation

The previous correctness regression proved that the pipeline AOEM-owned path can
close 14400 transactions correctly.

This signoff adds the strict active-window performance result:

```text
14400 tx closed within the first_tx_seen -> final_close window.
```

The total receiver process elapsed may exceed 30 minutes when B is started early
and waits for A to begin sending. That startup wait is not counted as receiver
active execution performance.

## Not Signed

- Total process elapsed 30min wall-clock from receiver start.
- 2h sustained gate.
- Fault profile.
- Multi-receiver.
- Higher TPS targets: 100 / 1000 / 10000.
- Final production cutover.

## Final Result

```text
pipeline AOEM-owned strict active-window 30min / 14400 performance: PASS
```
