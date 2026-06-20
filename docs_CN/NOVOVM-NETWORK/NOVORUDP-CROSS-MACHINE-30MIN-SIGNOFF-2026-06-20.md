# NovoRUDP Cross-Machine 30min Signoff

Date: 2026-06-20

## Signoff Result

```text
NovoRUDP Cross-machine 30min / 14400: PASS
Production Soak v1 NovoRUDP sustained gate: PASS
Receipt/Canonical Proof Writer durable ledger close: PASS
Full sender + receiver cross-machine signoff: PASS
Validation baseline: 4d8732b
```

This signoff moves NovoRUDP from active fault isolation into hardening and extended validation.

## Validated Scope

```text
single sender / single receiver
cross-machine
30min sustained
14400 tx
NovoRUDP enabled
receiver lifecycle closed
sender receiver_done_ack closed
receipt/canonical durable ledger close proven by tx_hash proof
```

Validated receiver result:

```text
accepted = true
received_unique = 14400
canonical_unique_included = 14400
aoem_executed_total = 14400
final_missing_sequence_count = 0
queue_pending_last = 0
ledger_completed_count = 14400
ledger_durable_missing_count = 0
duplicate_receipt = 0
duplicate_canonical_included = 0
receipt_index_consistent = true
```

Validated sender result:

```text
accepted = true
sender_completed = true
tail_repair_success = true
final_missing_count = 0
latest_ack_missing_count = 0
latest_ack_receiver_done = true
receiver_final_done = true
tail_repair_completion_reason = receiver_done_ack
send_failed_count = 0
repair_send_failed_count = 0
window_failed_count = 0
window_success_count = 45
```

## Closed Loop

```text
sender sustained send
=> receiver NovoRUDP receive/repair
=> AOEM execution
=> receipt/canonical included
=> durable ledger close
=> receiver_done_ack
=> sender final success
```

The critical completed invariant is:

```text
receipt/canonical tx_hash proof
=> lookup sequence
=> close durable missing
=> ledger_completed_count grows
=> ledger_durable_missing_count reaches 0
=> receiver_done_ack
```

## Explicitly Not Signed

```text
2h sustained
hostile/fault profile
2h + fault combined
multi-receiver fanout
relay/NAT/anti-censorship
production-ready final transport
```

## Follow-Up Gates

```text
1. NovoRUDP 2h sustained
2. NovoRUDP fault profile
3. NovoRUDP 2h + fault combined
4. NovoRUDP Reliable Transport State Machine v1 protocol hardening
```
