# NOVOVM AOEM-owned Pipeline 30min Active-window Baseline Freeze

## Frozen Baseline

```text
pipeline AOEM-owned 30min active-window performance baseline: FROZEN
Commit: 08bcf6b
Tag: pipeline-aoem-owned-30min-active-window-v1
Status: A/B final PASS
```

This document freezes the current stable AOEM-owned pipeline baseline before
starting longer sustained and fault-profile validation.

## Signed Evidence

```text
NovoRUDP transport: PASS
AOEM-owned production candidate path: PASS
Pipeline receiver path: PASS
Ready-queue active drain: PASS
A/B final: PASS
```

Execution ownership:

```text
tx_ingress_real_callsite = aoem_runtime_worker
network_receiver_calls_production_tx_ingress = false
tx_ingress_called_by_aoem_runtime_worker = true
fallback = false
```

30min active-window performance:

```text
performance_window_start_source = first_tx_seen
performance_window_elapsed_ms = 1630657
active_window ~= 27.18min
active_close_tx_count = 14400
active_close_tps_x1000 = 8830
active_close_tps ~= 8.83 TPS
strict_30min_performance_pass = true
```

A sender final:

```text
accepted = true
fail_reason = null
transport_profile = novorudp
tail_repair_success = true
tail_repair_completion_reason = receiver_done_ack
latest_ack_missing_count = 0
latest_ack_receiver_done = true
receiver_final_done = true
receiver_final_missing_count = 0
```

B receiver final:

```text
accepted = true
canonical_unique_included_total = 14400
ledger_completed_count = 14400
ledger_durable_missing_count = 0
queue_pending_last = 0
receiver_done = true
```

## Architecture Boundary

This baseline keeps the existing ownership split:

```text
NovoRUDP = reliable object transport
AOEM Runtime Worker = execution / persistence / proof close owner
NOVOVM Host = orchestration / verification / report / consensus handoff
```

Current `ready_queue_active_drain` is the stable 8 TPS / 30min baseline.

## Not Started In This Freeze

Full async high-frequency engine is not started in this phase.

The future high-frequency engine must be feature-gated:

```text
NOVOVM_AOEM_FULL_ASYNC_RUNTIME_ENGINE=1
```

It must not directly replace the current production candidate path without
separate smoke, mini, 30min, 2h, and fault-profile gates.

## Next Validation Order

```text
1. pipeline AOEM-owned 2h sustained
2. pipeline AOEM-owned fault profile
3. Full Async AOEM Runtime Engine v1 planning / feature-gated implementation
```

## Not Signed Yet

```text
2h sustained
fault profile
multi-receiver
higher TPS targets
final production cutover
full async high-frequency engine
```

## Final Freeze Statement

```text
08bcf6b is the stable AOEM-owned pipeline 30min active-window baseline.
Do not replace it with full async runtime work until 2h sustained and fault
profile validation are handled as separate gates.
```
