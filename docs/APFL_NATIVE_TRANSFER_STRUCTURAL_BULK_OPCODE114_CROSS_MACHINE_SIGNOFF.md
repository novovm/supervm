# APFL NativeTransfer Structural Bulk Opcode114 Cross-Machine Signoff

Date: 2026-07-01

Status: `PASS / SIGNED`

Baseline:

```text
SUPERVM HEAD = 16f1ba7
AOEM route = aoem_execute_ops_wire_v1 / opcode 114
AOEM output = aoem_state_read_v1
Payload mode = native_transfer_apfl_v0
Bulk size = 128 payloads / route
Transport pacing = 128 / 1ms
```

## Scope

This signoff covers the structural bulk APFL native transfer path:

```text
NOVORUDP
  -> APFL compact payload bytes
  -> SUPERVM bulk handoff
  -> aoem_execute_ops_wire_v1 / opcode 114
  -> AOEM structural native transfer hot plans
  -> AOEM state surfaces
  -> OCCC delta contract surfaces
```

The signed path preserves the boundary:

```text
SUPERVM does not execute APFL native transfer semantics locally.
SUPERVM does not depend on AOEM Rust crates.
SUPERVM enters AOEM through aoem-bindings / aoem_ffi.dll.
AOEM owns APFL structural execution and hot plan routing.
NOVORUDP transport ABI remains unchanged.
```

## Previous Baseline

The previous opcode 114 production route proved correctness but used a
per-payload transaction shape:

```text
payload_count = 4800
route_success = 4800
state_read_count = 24000
canonical materialization remained part of the effective hot path
```

That baseline is superseded for performance work by this structural bulk path.

## Structural Bulk Validation

The structural bulk route changes the execution shape:

```text
payload_count = 4800
bulk_size = 128
route_success = 38
state_read_count = 190
aoem_hot_plan_count = 76
canonical_materialization_count = 0
```

Interpretation:

```text
4800 payloads are no longer submitted as 4800 AOEM route calls.
24000 per-payload state reads are reduced to 190 bulk state reads.
Canonical full transaction materialization is removed from the hot execution path.
AOEM hot plans execute the APFL native transfer structural route.
```

## Local Smoke

Run:

```text
local structural bulk 4800 x 128
```

Result:

```text
transport_payloads_delivered = 4800
receiver_transport_final_missing_count = 0

aoem_apfl_bulk_route_count = 38
aoem_apfl_state_read_count = 190
aoem_apfl_canonical_materialization_count = 0
aoem_apfl_hot_plan_executed = true
aoem_apfl_hot_plan_count = 76

business_transactions_decoded_count = 614400
aoem_transactions_executed_total = 614400
ledger_transactions_completed_count = 614400

canonical_tx_hash_mismatch_count = 0
signature_verify_error_count = 0
business_transactions_per_sec ~= 1008866
```

Classification:

```text
LOCAL SMOKE PASS
Local near-1M TPS evidence only.
Million TPS is not claimed from local smoke.
```

## Cross-Machine Preflight

Run:

```text
real-ab-apfl-structural-bulk-4800x128-20260701-e5cab37-20260701-132150
```

Result:

```text
transport_payloads_delivered = 4800
receiver_transport_final_missing_count = 0
receiver_transport_repair_received_count = 7
receiver_transport_duplicate_received_count = 0

aoem_apfl_bulk_route_count = 38
aoem_apfl_state_read_count = 190
aoem_apfl_canonical_materialization_count = 0
aoem_apfl_hot_plan_executed = true
aoem_apfl_hot_plan_count = 76
aoem_apfl_occc_delta_contract_present_count = 38

business_transactions_decoded_count = 614400
aoem_transactions_executed_total = 614400
ledger_transactions_completed_count = 614400

canonical_tx_hash_mismatch_count = 0
signature_verify_error_count = 0

receiver_transport_delivery_elapsed_ms = 5014
aoem_execute_elapsed_ms = 7344
business_transactions_per_sec = 122536
```

Classification:

```text
DIRTY / RUNTIME PREFLIGHT PASS
Correctness and structural bulk route validated.
Not used as final clean signoff.
```

## Clean Cross-Machine Runs

First clean run:

```text
real-ab-apfl-structural-bulk-4800x128-20260701-16f1ba7-20260701-133036
```

Result:

```text
transport_payloads_delivered = 4800
receiver_transport_final_missing_count = 0
receiver_transport_repair_received_count = 1
receiver_transport_duplicate_received_count = 0

aoem_apfl_bulk_route_count = 38
aoem_apfl_state_read_count = 190
aoem_apfl_canonical_materialization_count = 0
aoem_apfl_hot_plan_executed = true
aoem_apfl_hot_plan_count = 76
aoem_apfl_occc_delta_contract_present_count = 38

business_transactions_decoded_count = 614400
aoem_transactions_executed_total = 614400
ledger_transactions_completed_count = 614400

canonical_tx_hash_mismatch_count = 0
signature_verify_error_count = 0

receiver_transport_delivery_elapsed_ms = 4103
aoem_execute_elapsed_ms = 7852
business_transactions_per_sec = 149744
```

Clean rerun:

```text
real-ab-apfl-structural-bulk-4800x128-20260701-16f1ba7-rerun-20260701-134102
```

Result:

```text
accepted = true
transport_payloads_delivered = 4800
receiver_transport_final_missing_count = 0
receiver_transport_repair_received_count = 3
receiver_transport_duplicate_received_count = 0

aoem_apfl_bulk_route_count = 38
aoem_apfl_state_read_count = 190
aoem_apfl_canonical_materialization_count = 0
aoem_apfl_hot_plan_executed = true
aoem_apfl_hot_plan_count = 76
aoem_apfl_occc_delta_contract_present_count = 38

business_transactions_decoded_count = 614400
aoem_transactions_executed_total = 614400
ledger_transactions_completed_count = 614400

canonical_tx_hash_mismatch_count = 0
signature_verify_error_count = 0

receiver_transport_delivery_elapsed_ms = 4565
aoem_execute_elapsed_ms = 7400
business_transactions_per_sec = 134589
```

Classification:

```text
PASS / SIGNED
```

## Signed Result

```text
CORRECTNESS PASS
STRUCTURAL BULK ROUTE PASS
CANONICAL MATERIALIZATION HOT PATH REMOVED
AOEM HOT PLAN PATH VALIDATED
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

The small repair counts observed in cross-machine runs are not correctness
blockers:

```text
final_missing = 0
duplicate = 0
ledger completed = 614400
```

## Throughput Interpretation

This signoff does not claim 1M TPS.

Observed throughput:

```text
local smoke ~= 1008866 TPS
clean cross-machine first run = 149744 TPS
clean cross-machine rerun = 134589 TPS
```

The clean cross-machine runs are the signed production-boundary evidence. The
local smoke is useful optimization evidence, but not a cross-machine throughput
claim.

## Next Work

The next performance work should not revisit APFL/SUPERVM ownership boundaries.
The structural bulk route is correct. Optimize attribution around:

```text
AOEM structural bulk hot plan execution elapsed
receiver execution scheduling
cross-machine transport delivery elapsed
sender socket send elapsed
AOEM state surface aggregation cost
```

Performance work must preserve:

```text
NOVORUDP opaque transport bytes
SUPERVM handoff/report role
AOEM opcode 114 semantic ownership
canonical hash/signature correctness
OCCC delta contract visibility
```
