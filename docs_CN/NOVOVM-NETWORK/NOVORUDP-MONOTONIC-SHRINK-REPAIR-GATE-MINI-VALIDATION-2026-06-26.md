# NovoRUDP Monotonic-Shrink Repair Gate Mini Validation

Date: 2026-06-26

## Validation Result

```text
NOVORUDP High-Overlap Monotonic-Shrink Retry Gate: MINI EFFECTIVE / PASS
commit = eb1945b
run_id = real-ab-mini-20260626-eb1945b-monoshrink-20260626-020428
```

This is an efficiency validation on top of the existing real A/B mini correctness baseline. It does not reopen or replace the signed mini correctness result:

```text
babf18b = mini correctness signed baseline
2787e49 = missing digest identity delta attribution baseline
eb1945b = high-overlap monotonic-shrink repair retry gate
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

The mini run closed without residual missing sequences. This means the shrink gate did not block necessary repair coverage.

Receiver side:

```text
B accepted = true
```

The run remains valid cross-machine evidence for this mini efficiency check only. It does not sign 30min sustained, weak-net, resume, or multi-receiver behavior.

## Baseline Diagnosis

The previous `2787e49` digest-delta run showed that the remaining duplicate source was not non-semantic digest instability:

```text
repair_digest_changed_after_coverage_count = 1428
repair_digest_changed_same_missing_count = 0
repair_digest_changed_same_sequence_set_count = 0
repair_digest_changed_real_missing_delta_count = 1428
repair_digest_changed_added_sequence_count = 0
repair_digest_changed_removed_sequence_count = 472
repair_digest_changed_overlap_ratio_bps = 8750
repair_digest_changed_high_overlap_count = 747
```

Interpretation:

```text
The missing set was really changing.
The change was monotonic shrink: no added sequences, only removed sequences.
The ACK stream was converging, but each shrink was treated as a new retry opportunity.
```

## Monotonic-Shrink Gate Evidence

Validated `eb1945b` result:

```text
repair_monotonic_shrink_detected_count = 827
repair_monotonic_shrink_suppressed_count = 827
repair_monotonic_shrink_controlled_retry_count = 0
repair_monotonic_shrink_added_sequence_count = 0
repair_monotonic_shrink_removed_sequence_count = 466
repair_monotonic_shrink_overlap_bps = 10000
repair_monotonic_shrink_timeout_escape_count = 0
```

The gate detected high-overlap monotonic-shrink snapshots and suppressed immediate broad retry. No escape retry was required in this mini sample.

## Efficiency Delta

Baseline from `2787e49`:

```text
repair_sent_count = 1908
repair_duplicate_waste_ratio_bps = 7484
repair_duplicate_same_snapshot_next_round_count = 1428
repair_sent_after_unique_coverage_complete_count = 1428
```

Validated `eb1945b`:

```text
repair_sent_count = 1307
repair_duplicate_waste_ratio_bps = 6327
repair_duplicate_same_snapshot_next_round_count = 827
repair_sent_after_unique_coverage_complete_count = 827
```

Repair sends dropped by about 31.50% relative to `2787e49`:

```text
1 - 1307 / 1908 ~= 31.50%
```

## Current Interpretation

`eb1945b` validates the previous attribution:

```text
The remaining repair duplicate source was high-overlap monotonic-shrink missing snapshots being treated as new retry rounds.
```

The fix suppresses this retry class without correctness regression:

```text
latest_ack_missing_count = 0
receiver_done_ack_received = true
controlled_retry_count = 0
```

This means the sender did not need an escape retry to close the mini path, and the suppression did not create a coverage gap.

## Explicit Non-Scope

This validation does not sign:

```text
30min sustained / 14400
2h sustained
weak-net fault profile
resume checkpoint
multi-receiver / load balancing
production final transport cutover
```

## Next Gate

Recommended next validation:

```text
small sustained / 30min pre-gate after eb1945b
```

Carry these fields forward:

```text
repair_monotonic_shrink_detected_count
repair_monotonic_shrink_suppressed_count
repair_monotonic_shrink_controlled_retry_count
repair_sent_count
repair_duplicate_waste_ratio_bps
repair_duplicate_same_snapshot_next_round_count
repair_sent_after_unique_coverage_complete_count
latest_ack_missing_count
receiver_done_ack_received
```
