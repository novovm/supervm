# APFL NativeTransfer Opcode 114 Cross-Machine Signoff

Date: 2026-07-01

Status: `PASS / SIGNED`

Baseline:

```text
SUPERVM HEAD = 1dfc154
AOEM runtime = a36303dd FULLMAX bundle
AOEM entry = aoem_execute_ops_wire_v1
AOEM opcode = 114 / compute.apfl_native_transfer_v1
AOEM output = aoem_state_read_v1
Payload mode = native_transfer_apfl_v0
```

## Boundary

This signoff freezes the APFL native transfer execution boundary:

```text
NOVORUDP
  -> APFL compact payload bytes
  -> aoem_execute_ops_wire_v1
  -> opcode 114 / compute.apfl_native_transfer_v1
  -> AOEM state surfaces
  -> OCCC delta contract
```

SUPERVM responsibilities:

```text
transport payload handoff
AOEM dynamic library loading through aoem-bindings
opcode 114 wire construction
AOEM state surface reads
report aggregation
```

SUPERVM must not:

```text
implement APFL native transfer execution semantics
depend on AOEM Rust crates
fallback to host-local APFL execution
call a standalone APFL symbol as the main path
```

AOEM owns:

```text
APFL native_transfer_batch_v0 semantics
CPU reference operator
OCCC delta bridge
state surface materialization
future GPU/FULLMAX backend for the same semantic contract
```

## Local Smoke

Run:

```text
local-apfl-opcode114-480x128-20260701-1010
```

Result:

```text
transport_payloads_delivered = 480
business_transactions_decoded_count = 61440
aoem_transactions_executed_total = 61440
ledger_transactions_completed_count = 61440

aoem_apfl_wire_route_opcode = 114
aoem_apfl_wire_route_success_count = 480
aoem_apfl_wire_route_error_count = 0
aoem_apfl_wire_route_capability_missing = false
aoem_apfl_occc_delta_contract_present_count = 480

canonical_tx_hash_match_count = 61440
canonical_tx_hash_mismatch_count = 0
signature_verify_count = 61440
signature_verify_error_count = 0

apfl_binary_bytes_per_tx = 32
legacy_bytes_per_tx = 238
apfl_binary_savings_ratio_bps = 8626
```

Classification:

```text
PASS
```

## Cross-Machine Sample

Run:

```text
real-ab-apfl-opcode114-480x128-20260701-1dfc154-100821
```

Parameters:

```text
payload_count = 480
txs_per_payload = 128
total_tx = 61440
payload_mode = native_transfer_apfl_v0
execute_aoem = 1
pacing = 128 / 1ms
```

Transport result:

```text
transport_payloads_delivered = 480
receiver_transport_final_missing_count = 0
receiver_transport_repair_received_count = 0
receiver_transport_duplicate_received_count = 0
```

AOEM route result:

```text
aoem_apfl_wire_route_enabled = true
aoem_apfl_wire_route_opcode = 114
aoem_apfl_wire_route_attempt_count = 480
aoem_apfl_wire_route_success_count = 480
aoem_apfl_wire_route_error_count = 0
aoem_apfl_wire_route_capability_missing = false
aoem_apfl_occc_delta_contract_present_count = 480
```

Execution result:

```text
business_transactions_decoded_count = 61440
aoem_transactions_executed_total = 61440
ledger_transactions_completed_count = 61440

canonical_tx_hash_match_count = 61440
canonical_tx_hash_mismatch_count = 0
signature_verify_count = 61440
signature_verify_error_count = 0
```

Compression result:

```text
apfl_binary_bytes_per_tx = 32
legacy_bytes_per_tx = 238
apfl_binary_savings_ratio_bps = 8626
```

Classification:

```text
PASS / SIGNED
```

## Signed Conclusion

```text
APFL NativeTransferBatchV0 is now connected to the AOEM unified semantic entry.
Cross-machine opcode 114 execution is signed at 480 x 128.
OCCC delta contract surfaces are readable.
SUPERVM local APFL native transfer execution is not the signed path.
NOVORUDP transport ABI remains unchanged.
```

Previous APFL native transfer signoff covered compact payload correctness through
the earlier host adapter path. This document supersedes that execution boundary
for future APFL native transfer work: the main path is now AOEM unified dispatch
opcode 114.

## Next Work

Next scale step:

```text
payload_count = 4800
txs_per_payload = 128
total_tx = 614400
payload_mode = native_transfer_apfl_v0
execute_aoem = 1
pacing = 128 / 1ms
AOEM route = opcode 114
```

Acceptance:

```text
transport_payloads_delivered = 4800
final_missing = 0
aoem_apfl_wire_route_success_count = 4800
aoem_apfl_wire_route_capability_missing = false
business_transactions_decoded_count = 614400
aoem_transactions_executed_total = 614400
ledger_transactions_completed_count = 614400
canonical_tx_hash_mismatch_count = 0
signature_verify_error_count = 0
occc_delta_contract_present = 4800
```

