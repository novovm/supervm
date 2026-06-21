# NOVOVM AOEM Runtime Ownership Phase 3: tx_ingress Shadow Path

Date: 2026-06-21

## Status

```text
Phase 1: AOEM state / persistence surface baseline
Phase 2: NOVOVM_AOEM_NATIVE_TX_BATCH_V1 ABI / result / proof shape
Phase 3: tx_ingress AOEM-owned native tx batch shadow path
```

Phase 3 is a shadow integration gate. It does not cut production over from the legacy host execution store.

## Boundary

Signed baseline preserved:

```text
NovoRUDP cross-machine 30min / 14400
legacy host transitional receipt/canonical path
```

Phase 3 changes:

```text
tx_ingress builds NOVOVM_AOEM_NATIVE_TX_BATCH_V1 when enabled
tx_ingress calls the AOEM-owned native tx batch shadow executor shape path
tx_ingress records receipt / state delta / canonical proof / durable ledger close proof shape
tx_ingress writes diagnostics-only report fields
```

Phase 3 does not:

```text
delete legacy host native store
write AOEM shadow result into production canonical
change NovoRUDP transport
resume 2h sustained gate
declare production-ready cutover
```

## Runtime Gate

Enable shadow path:

```text
NOVOVM_AOEM_NATIVE_TX_BATCH_SHADOW=1
```

When disabled, tx_ingress continues through the legacy transitional path only.

## Ownership Rule

SUPERVM/NOVOVM host remains responsible for:

```text
network ingress
pending queue
batch construction
AOEM ABI call
proof/result shape verification
reporting and consensus handoff
```

AOEM-owned path is the production target for:

```text
execution
state transition
receipt proof
canonical inclusion proof
durable ledger close proof
snapshot metadata
persistence ownership
```

The legacy host store remains:

```text
runtime_ownership = legacy_host_transitional
production_target = false
replacement_target = aoem_runtime_owned_state_persistence
```

## Report Fields

Phase 3 adds root-level diagnostics:

```text
aoem_native_tx_batch_shadow_enabled
aoem_native_tx_batch_v1_built
aoem_native_tx_batch_v1_tx_count
aoem_native_tx_batch_v1_input_commitment
aoem_native_tx_batch_shadow_result_ok
aoem_native_tx_batch_shadow_receipt_count
aoem_native_tx_batch_shadow_state_delta_root_present
aoem_native_tx_batch_shadow_canonical_proof_count
aoem_native_tx_batch_shadow_ledger_close_proof_count
aoem_native_tx_batch_shadow_snapshot_metadata_present
aoem_native_tx_batch_shadow_mismatch_reasons
aoem_native_tx_batch_shadow_writes_production_canonical
tx_ingress_legacy_host_transitional_used
tx_ingress_production_target
```

Expected shadow-mode result:

```text
aoem_native_tx_batch_shadow_enabled = true
aoem_native_tx_batch_v1_built = true
aoem_native_tx_batch_shadow_result_ok = true
aoem_native_tx_batch_shadow_receipt_count = input tx count
aoem_native_tx_batch_shadow_writes_production_canonical = false
tx_ingress_legacy_host_transitional_used = true
tx_ingress_production_target = legacy_host_transitional
```

## Signoff Tests

```text
tx_ingress_builds_native_tx_batch_v1_shadow_smoke
tx_ingress_shadow_path_does_not_disable_legacy_host_path
tx_ingress_shadow_result_shape_smoke
tx_ingress_shadow_receipt_count_matches_input_count
tx_ingress_shadow_does_not_write_production_canonical
tx_ingress_reports_legacy_host_transitional_not_production_target
```

## Next Phase

Phase 4 compares AOEM-owned shadow output against legacy host transitional output:

```text
receipt count / tx_hash
state delta root
canonical proof
durable ledger close proof
snapshot metadata
```

Only after Phase 4 should tx_ingress production cutover be considered.
