# NOVOVM AOEM-owned Pipeline 14400 Correctness Regression Signoff

Date: 2026-06-21

## Status

```text
pipeline AOEM-owned 14400 correctness regression: PASS
pipeline AOEM-owned strict 30min performance gate: NOT PASSED
Baseline commit: 96989ee
Transport: NovoRUDP
Receiver pipeline: aoem_runtime_worker_pipeline
Scheduler: ready_queue_active_drain
```

This signoff freezes correctness only. It does not sign the strict 30min wall-clock performance gate.

## Signed

```text
A final: PASS
B final: PASS
received_unique = 14400
canonical_unique_included = 14400
ledger_completed_count = 14400
ledger_durable_missing_count = 0
final_missing_sequence_count = 0
queue_pending_last = 0
latest_ack_missing_count = 0
receiver_done_ack = true
fallback = false
```

The receiver executed through the AOEM-owned pipeline path:

```text
receiver_pipeline_mode = aoem_runtime_worker_pipeline
tx_ingress_real_callsite = aoem_runtime_worker
network_receiver_calls_production_tx_ingress = false
tx_ingress_called_by_network_receiver = false
tx_ingress_called_by_aoem_runtime_worker = true
tx_ingress_selected_path = aoem_runtime_owned_state_persistence
tx_ingress_production_target = aoem_runtime_owned_state_persistence
aoem_owned_regression_signable = true
aoem_native_tx_batch_production_fallback_used = false
```

Proof and close counts completed:

```text
aoem_native_tx_batch_production_receipt_count = 14400
aoem_native_tx_batch_production_canonical_proof_count = 14400
aoem_native_tx_batch_production_ledger_close_proof_count = 14400
duplicate_receipt = 0
duplicate_canonical_included = 0
receipt_index_consistent = true
```

Ready-queue scheduler was enabled on the receiver:

```text
aoem_runtime_worker_scheduler = ready_queue_active_drain
aoem_runtime_worker_active_sleep_ms = 0
aoem_runtime_worker_idle_sleep_ms = 10
```

## Not Signed

```text
strict 30min wall-clock performance
2h sustained
fault profile
multi-receiver
final production cutover
higher TPS targets
```

The strict 30min performance gate is not signed because the run closed correctly but exceeded the 30min wall-clock target:

```text
receiver elapsed ~= 32.27 min
average receiver close TPS ~= 7.44
target = 14400 tx / 30 min ~= 8 TPS
```

## Boundary

This result proves the new pipeline can close 14400 transactions correctly across A/B using NovoRUDP and the AOEM-owned runtime worker path.

It does not prove the pipeline can always close 14400 transactions within 30 minutes. Wall-clock performance must be handled by a separate performance fix and regression.

## Next Work

Open a separate performance task:

```text
Pipeline AOEM Runtime Worker Throughput / 30min Wall-clock Performance Fix
```

The performance task must focus on the extra wall-clock time only and must not mix correctness, ACK, transport ownership, or AOEM ownership with throughput attribution.

Initial attribution fields:

```text
active_send_window_ms
receiver_total_elapsed_ms
finalization_tail_ms
tail_repair_wait_ms
sender_pacing_tps
aoem_runtime_worker_batch_size
aoem_runtime_worker_inflight_batch_count
object_assembler_flush_delay_ms
aoem_runtime_worker_submit_elapsed_ms
aoem_runtime_worker_result_drain_elapsed_ms
finality_report_worker_backpressure
diagnostics_report_write_elapsed_ms
pipeline_stage_lag
pipeline_backpressure_reason
```
