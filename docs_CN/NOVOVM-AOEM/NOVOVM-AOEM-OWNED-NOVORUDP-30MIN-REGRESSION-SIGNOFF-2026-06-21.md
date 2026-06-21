# NOVOVM AOEM-owned NovoRUDP 30min Regression Signoff

Date: 2026-06-21

Status: PASS

Baseline:

```text
AOEM-owned NovoRUDP 30min / 14400 regression: PASS
Validation commit: 797711f
Transport: NovoRUDP
Plain UDP profile: removed
Execution owner: AOEM-owned production candidate path
```

## Signed Scope

This signoff covers the real cross-machine 30min / 14400 regression after the AOEM runtime ownership migration.

Signed:

```text
- Single sender / single receiver
- Cross-machine NovoRUDP transport
- 30min sustained profile
- 14400 tx expected and closed
- AOEM-owned tx_ingress production candidate path selected
- No legacy host transitional fallback
- Receipt proof count = 14400
- Canonical proof count = 14400
- Durable ledger close proof count = 14400
- Receiver durable ledger missing = 0
- Sender received receiver_done_ack
- Sender final missing = 0
```

## B Receiver Evidence

```text
accepted = true
received_unique = 14400
canonical_unique_included = 14400
ledger_completed_count = 14400
ledger_durable_missing_count = 0
final_missing_sequence_count = 0
queue_pending_last = 0

transport_profile = novorudp
novorudp_enabled = true

tx_ingress_selected_path = aoem_runtime_owned_state_persistence
tx_ingress_production_target = aoem_runtime_owned_state_persistence
aoem_owned_single_path_enforced = true
aoem_owned_regression_signable = true
legacy_host_transitional_fallback_used = false

aoem_native_tx_batch_production_receipt_count = 14400
aoem_native_tx_batch_production_canonical_proof_count = 14400
aoem_native_tx_batch_production_ledger_close_proof_count = 14400

receiver_done = true
receiver_ack_send_ok_count > 0
receiver_ack_send_error_count = 0
receiver_ack_missing_target_count = 0
```

## A Sender Evidence

```text
accepted = true
sender_completed = true
sender_hard_timeout_reached = false
fail_reason = null

primary_sent_count = 14400
send_failed_count = 0
primary_ack_received_count > 0
latest_ack_epoch > 0
latest_ack_receiver_done = true
latest_ack_missing_count = 0
receiver_final_done = true
receiver_final_missing_count = 0
tail_repair_completion_reason = receiver_done_ack
```

## Meaning

This is not the legacy host-store 30min baseline.

```text
Previous signed baseline:
NovoRUDP + legacy host execution/canonical/ledger close

This signed baseline:
NovoRUDP reliable object transport
+ AOEM-owned production candidate execution/proof/persistence close
```

The receiver hot path selected AOEM-owned runtime ownership for tx_ingress and did not use the legacy host transitional fallback as the production truth source.

## Not Signed

This signoff does not cover:

```text
- AOEM-owned 2h sustained gate
- fault profile
- combined 2h + fault profile
- multi-receiver fanout
- relay / NAT / anti-censorship
- Network Receiver / AOEM Runtime Worker separation
- production-ready final cutover
```

## Next Gate

The next recommended gate is:

```text
AOEM-owned 2h sustained gate
```

The next architectural refactor remains:

```text
Receiver Network Path / AOEM Runtime Worker Separation
```

That refactor should keep the established boundary:

```text
NovoRUDP = reliable object transport
AOEM Runtime = execution / persistence / receipt / canonical proof / ledger close proof
NOVOVM Host = orchestration / verification / consensus handoff / report
```
