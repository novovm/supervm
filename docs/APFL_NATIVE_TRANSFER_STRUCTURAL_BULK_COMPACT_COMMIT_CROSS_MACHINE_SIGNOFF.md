# APFL NativeTransfer Structural Bulk Compact Commit Cross-Machine Signoff

Date: 2026-07-01

Status: `PASS / SIGNED`

Baseline:

```text
SUPERVM HEAD = 4f45526
AOEM HEAD = 34a3cb44
Route = aoem_execute_ops_wire_v1 / opcode 114
Payload mode = native_transfer_apfl_v0
Bulk size = 128 payloads / route
Transport pacing = 128 / 1ms
```

## Scope

This signoff covers the compact commit optimization for the already signed
APFL native transfer structural bulk opcode 114 path.

The optimization preserves the existing route:

```text
NOVORUDP
  -> APFL compact payload bytes
  -> SUPERVM bulk handoff
  -> aoem_execute_ops_wire_v1 / opcode 114
  -> AOEM structural native transfer hot plans
  -> compact AOEM state commit
  -> AOEM state surfaces
  -> OCCC delta contract surfaces
```

The signed boundary remains unchanged:

```text
SUPERVM does not execute APFL native transfer semantics locally.
SUPERVM does not depend on AOEM Rust crates.
SUPERVM enters AOEM through aoem-bindings / aoem_ffi.dll.
AOEM owns APFL structural execution and hot plan routing.
NOVORUDP transport ABI remains unchanged.
```

## Optimization

The previous structural bulk route was correct, but each transaction still
created multiple hot plan state writes.

Previous hot plan write shape:

```text
tx_count = 614400
writes_per_tx = 4
aoem_hot_plan_total_writes = 2457600
```

Compact commit write shape:

```text
tx_count = 614400
bulk_route_count = 38
aoem_hot_plan_total_writes = 614438
```

Interpretation:

```text
Per-tx balance deltas remain explicit.
Per-tx nonce / receipt / canonical hash detail is compacted into bulk commitment evidence.
Deterministic digest and correctness checks remain intact.
Hot plan write count is reduced by about 75%.
```

## Local Smoke

Run:

```text
local-apfl-compact-commit-4800x128-20260701-143015
```

Result:

```text
transport_payloads_delivered = 4800
receiver_transport_final_missing_count = 0

aoem_apfl_bulk_route_count = 38
aoem_apfl_state_surface_read_count = 190
aoem_apfl_canonical_materialization_count = 0
aoem_apfl_hot_plan_executed = true
aoem_apfl_hot_plan_count = 76
aoem_apfl_hot_plan_total_writes = 614438

business_transactions_decoded_count = 614400
aoem_transactions_executed_total = 614400
ledger_transactions_completed_count = 614400

canonical_tx_hash_mismatch_count = 0
signature_verify_error_count = 0
aoem_apfl_occc_delta_contract_present_count = 38

business_transactions_per_sec ~= 1002283
aoem_execute_elapsed_ms ~= 3238
aoem_apfl_hot_plan_execute_elapsed_ms ~= 1299
```

Classification:

```text
LOCAL SMOKE PASS
Local 1M-class TPS evidence only.
Million TPS is not claimed from local smoke.
```

## Clean Cross-Machine Run

Run:

```text
real-ab-apfl-compact-commit-4800x128-20260701-4f45526-144348
```

Sender:

```text
accepted = true
payload_mode = native_transfer_apfl_v0
txs_per_payload = 128
transport_payloads_sent = 4800
business_transactions_sent_count = 614400
sender_transport_repair_sent_count = 32
sender_transport_missing_final_count = 0

sender_elapsed_ms = 6746
sender_data_frames_per_sec = 711
sender_data_bytes_per_sec = 2977053
sender_effective_business_tx_per_sec = 122953
sender_batch_build_elapsed_ms = 1568
sender_apfl_encode_elapsed_ms = 1568
sender_socket_send_elapsed_ms = 4108
```

Receiver:

```text
transport_payloads_delivered = 4800
receiver_transport_final_missing_count = 0
receiver_transport_repair_received_count = 17
receiver_transport_duplicate_received_count = 0

aoem_apfl_bulk_route_count = 38
aoem_apfl_state_surface_read_count = 190
aoem_apfl_canonical_materialization_count = 0
aoem_apfl_canonical_materialization_elapsed_ms = 0
aoem_apfl_hot_plan_executed = true
aoem_apfl_hot_plan_count = 76
aoem_apfl_hot_plan_total_writes = 614438
aoem_apfl_occc_delta_contract_present_count = 38

business_transactions_decoded_count = 614400
aoem_transactions_executed_total = 614400
ledger_transactions_completed_count = 614400

canonical_tx_hash_mismatch_count = 0
signature_verify_error_count = 0
```

Timing:

```text
business_transactions_per_sec = 120352
receiver_transport_delivery_elapsed_ms = 5105
receiver_aoem_execute_elapsed_ms = 3052
aoem_apfl_opcode_114_execute_elapsed_ms = 2973
aoem_apfl_hot_plan_execute_elapsed_ms = 1247
aoem_apfl_structural_native_transfer_execute_elapsed_ms = 1356
aoem_apfl_canonical_hash_parity_elapsed_ms = 726
aoem_apfl_occc_delta_contract_generation_elapsed_ms = 1583
aoem_apfl_state_read_elapsed_ms = 0
```

Classification:

```text
PASS / SIGNED
```

## Signed Result

```text
CORRECTNESS PASS
STRUCTURAL BULK ROUTE PRESERVED
CANONICAL MATERIALIZATION HOT PATH REMOVED
AOEM HOT PLAN PATH VALIDATED
COMPACT COMMIT HOT PLAN WRITES VALIDATED
SUPERVM LOCAL EXECUTION FALLBACK = false
MILLION TPS NOT CLAIMED
```

The signed correctness conditions held at 614400 tx scale:

```text
business_transactions_decoded_count = 614400
aoem_transactions_executed_total = 614400
ledger_transactions_completed_count = 614400
canonical_tx_hash_mismatch_count = 0
signature_verify_error_count = 0
occc_delta_contract_present_count = 38
```

The repair counts observed in the cross-machine run are not correctness
blockers:

```text
final_missing = 0
duplicate = 0
ledger completed = 614400
```

## Performance Interpretation

This signoff does not claim cross-machine 1M TPS.

The compact commit optimization reduced the AOEM execution segment:

```text
previous clean structural bulk aoem_execute_elapsed_ms ~= 7400
compact commit clean aoem_execute_elapsed_ms = 3052
```

The hot plan write reduction was directly observed:

```text
previous structural bulk hot plan writes = 2457600
compact commit hot plan writes = 614438
write reduction ~= 75%
```

The cross-machine throughput remained bounded by outer runtime and transport
factors:

```text
business_transactions_per_sec = 120352
receiver_transport_delivery_elapsed_ms = 5105
sender_socket_send_elapsed_ms = 4108
```

## Next Work

The AOEM compact commit path is signed. The next performance work should not
reopen the APFL/AOEM/SUPERVM ownership boundary.

Next attribution should focus on:

```text
cross-machine transport delivery elapsed
sender socket send elapsed
receiver scheduling and lifecycle overhead
AOEM opcode 114 structural execution residual cost
OCCC delta contract generation elapsed
hash parity elapsed
release vs debug/runtime build behavior
```

Performance work must preserve:

```text
NOVORUDP opaque transport bytes
SUPERVM handoff/report role
AOEM opcode 114 semantic ownership
compact commit deterministic evidence
canonical hash/signature correctness
OCCC delta contract visibility
```
