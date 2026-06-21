# NOVOVM AOEM Runtime Ownership Phase 5: Production Candidate Cutover

Date: 2026-06-21

## Status

```text
Phase 1: AOEM state / persistence surface
Phase 2: NOVOVM_AOEM_NATIVE_TX_BATCH_V1 ABI
Phase 3: tx_ingress AOEM-owned shadow path
Phase 4: AOEM shadow output vs legacy host transitional output comparison
Phase 5: AOEM-owned tx_ingress production candidate gate
```

Phase 5 is a production candidate gate. It is not a default production cutover.

## Runtime Gate

```text
NOVOVM_AOEM_NATIVE_TX_BATCH_PRODUCTION_CANDIDATE=1
```

Default behavior remains unchanged:

```text
tx_ingress_production_target = legacy_host_transitional
```

When the gate is enabled and AOEM-owned result/proof shape is complete:

```text
tx_ingress_production_target = aoem_runtime_owned_state_persistence
```

## Goal

Phase 5 gives the AOEM-owned tx batch path the right to become the production candidate output source for:

```text
per-tx receipt
canonical inclusion proof
durable ledger close proof
state delta root
snapshot metadata
```

The legacy host native store remains available as:

```text
fallback
comparison
regression baseline
```

It is not the final production target.

## Safety Rules

Production candidate success is forbidden unless all of these are true:

```text
AOEM-owned result exists
receipt_count == input_tx_count
canonical_proof_count == input_tx_count
durable_ledger_close_proof_count == input_tx_count
state_delta_root is present
snapshot_metadata is present
```

If any rule fails:

```text
aoem_native_tx_batch_production_candidate_result_ok = false
aoem_native_tx_batch_production_fallback_used = true
tx_ingress_production_target = legacy_host_transitional_fallback
```

The failure reason must be explicit in:

```text
aoem_native_tx_batch_production_mismatch_reasons
```

## Report Fields

```text
aoem_native_tx_batch_production_candidate_enabled
aoem_native_tx_batch_production_candidate_result_ok
aoem_native_tx_batch_production_owner
aoem_native_tx_batch_production_receipt_count
aoem_native_tx_batch_production_canonical_proof_count
aoem_native_tx_batch_production_ledger_close_proof_count
aoem_native_tx_batch_production_state_delta_root_present
aoem_native_tx_batch_production_snapshot_metadata_present
aoem_native_tx_batch_production_fallback_used
aoem_native_tx_batch_production_mismatch_reasons
aoem_native_tx_batch_production_double_write_legacy_canonical
tx_ingress_production_target
tx_ingress_legacy_host_transitional_used
```

Expected enabled healthy result:

```text
aoem_native_tx_batch_production_candidate_enabled = true
aoem_native_tx_batch_production_candidate_result_ok = true
aoem_native_tx_batch_production_owner = aoem_runtime_owned_state_persistence
aoem_native_tx_batch_production_receipt_count = input tx count
aoem_native_tx_batch_production_canonical_proof_count = input tx count
aoem_native_tx_batch_production_ledger_close_proof_count = input tx count
aoem_native_tx_batch_production_state_delta_root_present = true
aoem_native_tx_batch_production_snapshot_metadata_present = true
aoem_native_tx_batch_production_fallback_used = false
aoem_native_tx_batch_production_mismatch_reasons = []
aoem_native_tx_batch_production_double_write_legacy_canonical = false
tx_ingress_production_target = aoem_runtime_owned_state_persistence
```

## Boundaries

Phase 5 does not:

```text
delete legacy host native store
enable AOEM-owned production path by default
resume 2h sustained gate
change NovoRUDP transport
move transaction semantics back into NovoRUDP
declare production-ready final cutover
```

NovoRUDP remains reliable object transport. AOEM owns semantic execution, proof, and persistence.

## Signoff Tests

```text
tx_ingress_aoem_production_candidate_gate_off_keeps_legacy_path
tx_ingress_aoem_production_candidate_gate_on_uses_aoem_owner
tx_ingress_aoem_production_candidate_receipt_count_required
tx_ingress_aoem_production_candidate_canonical_proof_required
tx_ingress_aoem_production_candidate_ledger_close_proof_required
tx_ingress_aoem_production_candidate_state_delta_root_required
tx_ingress_aoem_production_candidate_does_not_double_write_legacy_canonical
tx_ingress_aoem_production_candidate_fallback_reports_reason
tx_ingress_legacy_host_remains_fallback_not_production_target
```

## Next Steps

After Phase 5:

```text
1. AOEM-owned production candidate unit/smoke
2. small tx_ingress production candidate smoke
3. 30min NovoRUDP regression
4. 2h sustained gate restart
```

Permanent replacement should only happen after candidate gate proves that AOEM-owned output is stable and legacy host fallback is no longer needed for safety.
