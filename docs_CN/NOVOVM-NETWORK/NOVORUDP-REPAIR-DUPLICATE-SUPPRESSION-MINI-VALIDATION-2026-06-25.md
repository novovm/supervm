# NovoRUDP Repair Duplicate Suppression Mini Validation

Date: 2026-06-25

## Validation Result

```text
Full Async Repair Pacing / Duplicate Suppression: MINI EFFECTIVE / PASS
commit = 5f2cd6e
run_id = real-ab-mini-20260625-5f2cd6e-repairpace-20260625-215613
```

This is an efficiency validation on top of the existing mini correctness baseline. It does not reopen or replace the signed correctness baseline:

```text
babf18b = mini correctness signed baseline
4fb16e4 = repair duplicate attribution-only baseline
5f2cd6e = repair pacing / duplicate suppression behavior baseline
```

## Correctness Non-Regression

Sender evidence:

```text
accepted = true
signed = true
fail_reason = null
receiver_done_ack_received = true
latest_ack_missing_count = 0
violations = []
repair_stop_reason = receiver_done_ack
```

Receiver evidence:

```text
B accepted = true
B ledger_completed_count = 480
B ledger_durable_missing_count = 0
```

## Efficiency Delta

Baseline from `4fb16e4` attribution run:

```text
repair_sent_count = 50555
repair_sent_unique_count = 480
repair_duplicate_sequence_count = 50075
repair_duplicate_waste_ratio_bps = 9905
primary_reason = duplicate_retry_or_packet_copies
```

Validated `5f2cd6e` result:

```text
repair_sent_count = 1713
repair_sent_unique_count = 480
repair_duplicate_sequence_count = 1233
repair_duplicate_waste_ratio_bps = 7197
```

The repair send count dropped by about 96.61%:

```text
1 - 1713 / 50555 ~= 96.61%
```

## Suppression Evidence

```text
repair_same_snapshot_retry_cap = 1
repair_same_snapshot_retry_count = 1
repair_suppressed_inflight_count = 1400730
repair_suppressed_cooldown_count = 1400730
repair_ack_refresh_wait_count = 4756
repair_retry_cooldown_ms = 250
```

Excluded causes remained excluded:

```text
repair_snapshot_stale = false
repair_sent_from_stale_snapshot_count = 0
repair_after_receiver_done_count = 0
repair_after_missing_zero_count = 0
repair_selected_already_acked_count = 0
```

## Current Interpretation

`5f2cd6e` successfully reduced duplicate repair burst volume without breaking the real A/B mini close path.

Remaining duplicate waste is no longer explained by stale snapshots, receiver_done leakage, missing-zero leakage, or already-ACKed range selection. The remaining 1233 duplicate sequence copies should be attributed next inside valid same-snapshot retry behavior.

## Next Attribution Target

```text
NOVORUDP Repair Same-Snapshot Duplicate Copy Attribution
```

Focus:

```text
same-round duplicate selection
same-snapshot next-round duplicate selection
cross-path duplicate between repair pump and post-primary continuation
in-flight race before ACK convergence
sequence-key / chunk-key de-duplication granularity
```

Suggested fields:

```text
repair_duplicate_same_round_count
repair_duplicate_same_snapshot_next_round_count
repair_duplicate_cross_path_count
repair_duplicate_post_primary_count
repair_duplicate_repair_pump_count
repair_duplicate_inflight_race_count
repair_sequence_last_sent_age_ms_p50
repair_sequence_last_sent_age_ms_p90
repair_sequence_last_sent_age_ms_p99
repair_unique_coverage_completed_at_sent_count
repair_sent_after_unique_coverage_complete_count
```

## Explicitly Not Signed Here

```text
production full async 30min sustained
2h soak
weak-net fault profile
resume checkpoint
multi-receiver / load balancing
final repair waste optimization
```

