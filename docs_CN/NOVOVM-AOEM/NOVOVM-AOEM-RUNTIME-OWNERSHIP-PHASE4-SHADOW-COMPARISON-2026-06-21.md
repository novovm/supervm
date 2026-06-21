# NOVOVM AOEM Runtime Ownership Phase 4: Shadow Output Comparison

Date: 2026-06-21

## Status

```text
Phase 1: AOEM state / persistence surface
Phase 2: NOVOVM_AOEM_NATIVE_TX_BATCH_V1 ABI
Phase 3: tx_ingress AOEM-owned shadow path
Phase 4: AOEM shadow output vs legacy host transitional output comparison
```

Phase 4 is a comparison gate. It is not a production cutover gate.

## Goal

For the same tx batch input, tx_ingress keeps both outputs:

```text
legacy host transitional output
AOEM-owned native tx batch shadow output
```

Phase 4 verifies that both paths are aligned or that any divergence is explicitly reported.

## Comparison Scope

The comparison report covers:

```text
input tx count
legacy receipt count
AOEM shadow receipt count
receipt count match
per-tx tx_hash match count
per-tx tx_hash mismatch count
state_delta_root presence
canonical proof count
durable ledger close proof count
snapshot metadata presence
mismatch reasons
```

The AOEM shadow path must not write production canonical:

```text
aoem_shadow_compare_writes_production_canonical = false
```

## Report Fields

```text
aoem_shadow_compare_enabled
aoem_shadow_compare_result_ok
aoem_shadow_compare_input_tx_count
aoem_shadow_compare_legacy_receipt_count
aoem_shadow_compare_aoem_receipt_count
aoem_shadow_compare_receipt_count_match
aoem_shadow_compare_tx_hash_match_count
aoem_shadow_compare_tx_hash_mismatch_count
aoem_shadow_compare_state_delta_root_present
aoem_shadow_compare_canonical_proof_count
aoem_shadow_compare_ledger_close_proof_count
aoem_shadow_compare_snapshot_metadata_present
aoem_shadow_compare_writes_production_canonical
aoem_shadow_compare_mismatch_reasons
```

Expected successful comparison:

```text
aoem_shadow_compare_enabled = true
aoem_shadow_compare_result_ok = true
aoem_shadow_compare_input_tx_count = input batch size
aoem_shadow_compare_legacy_receipt_count = input batch size
aoem_shadow_compare_aoem_receipt_count = input batch size
aoem_shadow_compare_receipt_count_match = true
aoem_shadow_compare_tx_hash_mismatch_count = 0
aoem_shadow_compare_state_delta_root_present = true
aoem_shadow_compare_canonical_proof_count > 0
aoem_shadow_compare_ledger_close_proof_count > 0
aoem_shadow_compare_snapshot_metadata_present = true
aoem_shadow_compare_writes_production_canonical = false
aoem_shadow_compare_mismatch_reasons = []
```

## Boundary

Phase 4 keeps these boundaries:

```text
do not delete legacy host native store
do not cut production canonical ownership
do not resume 2h sustained gate
do not change NovoRUDP transport
do not move transaction semantics back into NovoRUDP
do not declare production-ready cutover
```

Legacy host path remains:

```text
runtime_ownership = legacy_host_transitional
production_target = false
replacement_target = aoem_runtime_owned_state_persistence
```

AOEM-owned path remains the future production target.

## Transport Boundary

NovoRUDP remains reliable object transport:

```text
object delivery
ACK range
missing range
repair range
delivery_complete
```

AOEM/NOVOVM own application completion:

```text
receipt
canonical proof
durable ledger close proof
snapshot metadata
application finality
```

Transport ACK and application ACK remain separate.

## Signoff Tests

```text
tx_ingress_shadow_compare_receipt_count_match_smoke
tx_ingress_shadow_compare_tx_hash_shape_smoke
tx_ingress_shadow_compare_records_mismatch_reasons
tx_ingress_shadow_compare_does_not_write_production_canonical
tx_ingress_shadow_compare_keeps_legacy_host_transitional_path
tx_ingress_shadow_compare_report_fields_smoke
```

## Next Phase

Phase 5 is production cutover design:

```text
AOEM-owned path takes over tx_ingress production receipt / canonical / ledger close ownership
legacy host path becomes fallback / regression
30min NovoRUDP signed baseline is rerun as migration regression
2h sustained gate resumes only after AOEM-owned production path is active
```
