# NovoRUDP Production Full Async Mini Signoff

Date: 2026-06-25

## Signoff Result

```text
production full async mini after 0844078 longwait-203111: PASS / SIGNED
A sender: PASS
B receiver: PASS
Total A/B signoff: PASS
cross-machine evidence: SIGNED
0844078 mini regression: PASS
```

Validated run:

```text
run_id = real-ab-mini-20260625-0844078-longwait-20260625-203111
session_id = real-ab-mini-20260625-0844078-longwait-20260625-203111
commit = 0844078
transport_protocol = novorudp
plain_udp = false
udp_socket_underlay = true
```

This signoff covers the real cross-machine mini path only. It does not sign 30min sustained, 2h soak, weak-net fault profile, resume checkpoint, or multi-receiver/load-balancing behavior.

## Validated Scope

```text
single sender / single receiver
real A/B cross-machine
production profile
full async runtime
NovoRUDP transport
signed endpoint record enabled
source pin / ACK target contract enabled
receiver_done_ack closed
AOEM proof / receipt / ledger close reached for 480 tx
```

Validated sender evidence:

```text
primary_sent_count = 480
primary_send_completed = true
endpoint_record_sent_count = 1
receiver_done_ack_received = true
sender_ack_receiver_done_seen = true
latest_ack_missing_count = 0
production_low_latency_signoff = true
violations = []
accepted = true
signed = true
```

Validated receiver evidence:

```text
top.accepted = true
receiver_summary.accepted = true
received_unique_count = 480
canonical_unique_included = 480
receipt_count = 480
ledger_completed_count = 480
ledger_durable_missing_count = 0
ledger_final_missing_count = 0
stable_progress_total = 480
aoem_executed_total = 480
queue_pending_last = 0
receiver_done = true
final_report_written = true
receiver_ack_send_ok_count = 1
receiver_done_ack_fast_path_send_ok_count = 8
```

## Invalidated Prior Run

The prior run below is explicitly invalidated as a signoff artifact:

```text
run_id = real-ab-mini-20260625-0844078-20260625-201801
result = INVALID
reason = operational wait-window too short
```

It must not be treated as a network/protocol failure. The receiver no-progress wait window was too short for manual cross-machine startup, so A could miss the valid receiver window.

Invalidated conclusions:

```text
not evidence of broken A->B data plane
not evidence of broken A ACK bind
not evidence of auth/key mismatch
not evidence of endpoint/source-pin failure
```

## Child Exit Interpretation

For the valid `longwait-203111` run, `child_exit_code = 1` is not a signoff failure because the wrapper completed the live summary and wrote the final report:

```text
fail_reason = completed_live_summary
final_report_written = true
stable_progress_total = 480
aoem_executed_total = 480
queue_pending_last = 0
```

## Follow-Up Item

Repair duplicate waste remains high and is carried as an efficiency follow-up, not a mini correctness blocker:

```text
repair_duplicate_waste_ratio_bps ~= 9907
follow_up = Full Async Repair Duplicate Waste Attribution
```

Initial attribution candidates:

```text
repair pump using stale missing snapshot
all-missing ACK repair continuation not converging quickly
receiver_done stop / grace stop not fast enough
sender-side de-duplication over already ACKed ranges insufficient
```

## Explicitly Not Signed

```text
production full async 30min sustained
2h soak
weak-net fault profile
resume checkpoint
multi-receiver / load balancing
repair duplicate efficiency
```

