# NOVOVM AOEM Native Transaction Batch ABI v1

Date: 2026-06-21

## Status

```text
Phase: 2 baseline
ABI: NOVOVM_AOEM_NATIVE_TX_BATCH_V1
Goal: algebraic semantic transaction batch shape
Production-ready: no
tx_ingress full migration: not yet
```

## Purpose

This ABI moves NOVOVM transaction execution toward AOEM runtime ownership.

The design replaces this production target:

```text
host raw bytes replay
  -> host native execution store materialization
  -> host RocksDB commit
```

with this target:

```text
canonical algebraic semantic tx batch
  -> AOEM execution IR
  -> AOEM state delta / receipt / canonical proof
  -> AOEM-owned persistence close
```

## Layer Boundary

```text
Network:
  reliable transport of canonical encoded semantic frames

Consensus / business protocol:
  validation of ordering, signatures, commitments, and proof terms

AOEM:
  high-concurrency execution, state transition, receipt/canonical proof,
  snapshot/readback, RocksDB-backed persistence
```

Network must not own business consensus. Consensus must not own AOEM scheduling or persistence. AOEM must own runtime state mutation.

## Algebraic Semantic Data Plane

The source of truth is no longer arbitrary raw bytes.

The source of truth is:

```text
operator + parameter payload/tensor + deterministic commitment
```

Bytes remain only as deterministic carriers at physical boundaries:

```text
NovoRUDP packet
FFI buffer
RocksDB key/value
snapshot/export
```

## Batch Input Schema

Schema:

```text
novovm-aoem-native-tx-batch/v1
```

Fields:

```text
batch_id
chain_id
height_hint
tx_count
tx_sequence_start
tx_sequence_end
canonical_input_commitment
algebraic_semantic_ir_version
tx_items
expected_output_commitment
```

The current IR version is:

```text
novovm-algebraic-semantic-ir/v1
```

## Transaction Item Shape

Each item contains:

```text
sequence
tx_hash
sender_identity
signer_identity
nonce
intent_type
semantic_operator
parameter_payload
canonical_rebuild_commitment
```

`semantic_operator` is the AOEM-executable algebraic operation, for example:

```text
TransferV1
StoragePutV1
ProgramDeployV1
VoteV1
SwapV1
```

`parameter_payload` is the structured parameter tensor/object for the operator.

## Commitments

`canonical_rebuild_commitment` proves that the structured item can rebuild its canonical transaction semantics.

`canonical_input_commitment` commits to the ordered batch item sequence.

`expected_output_commitment` commits to the expected result envelope shape before execution.

These commitments are not encryption. They are deterministic verification anchors.

## Batch Result Schema

Schema:

```text
novovm-aoem-native-tx-batch-result/v1
```

Fields:

```text
batch_result_id
batch_id
per_tx_receipts
state_delta_root
canonical_inclusion_proof
receipt_root
durable_ledger_close_proof
snapshot_metadata
```

## Host Ownership Rule

SUPERVM host may construct the batch input and verify result commitments.

SUPERVM host must not be the production owner of:

```text
state mutation
receipt production
canonical proof production
RocksDB persistence
durable execution close
```

The current host-native execution store is explicitly transitional:

```text
runtime_ownership = legacy_host_transitional
production_target = false
replacement_target = aoem_runtime_owned_state_persistence
```

## Phase 2 Acceptance

Phase 2 accepts only the ABI and proof/result shape baseline:

```text
schema shape exists
canonical input commitment exists
per-tx receipt shape exists
state delta root shape exists
host store not owner smoke exists
capability contract exposes native_tx_batch_v1
```

Phase 2 does not claim:

```text
full tx_ingress migration
2h sustained pass
production-ready transport
legacy store removal
```

## Next Phase

Phase 3 must introduce a dual-track tx ingress path:

```text
legacy_host_transitional path
aoem_native_tx_batch_v1 path
```

The AOEM-owned path becomes the target default after local and cross-machine regression gates pass.
