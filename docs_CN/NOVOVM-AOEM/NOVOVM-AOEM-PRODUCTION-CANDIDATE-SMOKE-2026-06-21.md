# NOVOVM AOEM-owned Production Candidate Smoke

Date: 2026-06-21

## Status

```text
Phase 5: AOEM-owned tx_ingress production candidate gate is available
Smoke: verify gate-on owner, proof shape, no fallback, no legacy production double-write
```

This smoke must run before any 30min regression or 2h sustained restart.

## Runtime Mode

Required:

```text
NOVOVM_AOEM_NATIVE_TX_BATCH_PRODUCTION_CANDIDATE=1
```

Optional diagnostics:

```text
NOVOVM_AOEM_NATIVE_TX_BATCH_SHADOW=1
```

## Smoke Assertions

The smoke passes only when:

```text
aoem_native_tx_batch_production_candidate_enabled = true
aoem_native_tx_batch_production_candidate_result_ok = true
aoem_native_tx_batch_production_owner = aoem_runtime_owned_state_persistence
tx_ingress_production_target = aoem_runtime_owned_state_persistence
aoem_native_tx_batch_production_receipt_count = input tx count
aoem_native_tx_batch_production_canonical_proof_count = input tx count
aoem_native_tx_batch_production_ledger_close_proof_count = input tx count
aoem_native_tx_batch_production_state_delta_root_present = true
aoem_native_tx_batch_production_snapshot_metadata_present = true
aoem_native_tx_batch_production_fallback_used = false
aoem_native_tx_batch_production_mismatch_reasons = []
aoem_native_tx_batch_production_double_write_legacy_canonical = false
```

Legacy host path may still execute as fallback / regression / comparison support during this phase, but it must not be marked as the production target:

```text
native_store_commit.production_target = false
native_store_commit.runtime_ownership = legacy_host_transitional
```

## Fail-Closed Conditions

The production candidate must fail and use fallback if any proof/result shape is incomplete:

```text
receipt_count != input tx count
canonical proof missing
durable ledger close proof missing
state_delta_root missing
snapshot_metadata missing
```

The reason must appear in:

```text
aoem_native_tx_batch_production_mismatch_reasons
```

## Non-Goals

This smoke does not:

```text
run 2h sustained
declare final production cutover
delete legacy host store
change NovoRUDP transport
move transaction semantics into NovoRUDP
```

## Signoff Tests

```text
tx_ingress_aoem_production_candidate_smoke_success
tx_ingress_aoem_production_candidate_no_fallback_on_complete_result
tx_ingress_aoem_production_candidate_owner_report_smoke
tx_ingress_aoem_production_candidate_no_legacy_double_write_smoke
tx_ingress_aoem_production_candidate_fail_closed_on_missing_proof
```

## Next Order

After smoke passes:

```text
1. small AOEM-owned tx_ingress production candidate smoke
2. 30min NovoRUDP migration regression
3. 2h sustained gate restart
4. legacy host path default downgrade only after repeated clean gates
```
