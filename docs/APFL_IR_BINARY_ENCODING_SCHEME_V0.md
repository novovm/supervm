# APFL IR Binary Encoding Scheme v0

Date: 2026-06-30

Status: `BINARY CODEC DESIGN / DO NOT IMPLEMENT DIRECTLY`

This document defines the next engineering target for turning APFL transaction structures into compact binary payloads that can be transported by NOVORUDP and executed by AOEM.

It is a design document, not an implementation change.

## Purpose

The missing layer is:

```text
APFLTransactionIR -> compact bytes -> NOVORUDP -> APFLTransactionIR
```

Current batch payload v0 still transmits fully expanded transaction wires:

```text
~247 B / tx
```

Binary Encoding Scheme v0 should target:

```text
80-120 B / tx
```

Future optimized targets:

```text
< 50 B / tx
```

## Hard Requirements

The binary codec must be:

```text
lossless
deterministic
batch-decodable
canonical-reconstructable
AOEM-executable
```

It must preserve:

```text
canonical tx hash semantics
signature verification semantics
ledger transition semantics
receipt semantics
deterministic replay
```

It must not change:

```text
NOVORUDP transport frame ABI
NOVORUDP DATA / REPAIR / ACK semantics
AOEM ledger semantics
consensus semantics
```

## Layer Boundary

NOVORUDP only transports encoded bytes.

```text
NOVORUDP TransportFrameV0
  DATA payload = APFL binary packet bytes
```

NOVORUDP does not parse:

```text
invariant_id
generator_id
coeff_vector
residual_sparse
signature_ref
transaction semantics
```

The APFL binary codec lives in the business payload / IR layer.

## APFLTransactionIR v0

Conceptual IR:

```text
APFLTransactionIR {
  invariant_id
  generator_id
  coeff_vector
  residual_sparse
  signature_ref
  batch_index
}
```

The binary codec should support this shape without freezing the final Rust struct yet.

## Binary Packet Layout v0

Recommended high-level packet:

```text
APFLBatchPacketV0 {
  magic
  version
  batch_header
  invariant_refs
  generator_refs
  coeff_block
  residual_block
  signature_block
  commitment
}
```

### Batch Header

Shared fields are sent once per batch:

```text
INVARIANT_TABLE_REF    2-4 bytes
GENERATOR_TABLE_REF    2-4 bytes
BATCH_SIZE             1-2 bytes
TX_FAMILY_ID           1-2 bytes
FLAGS                  varint or u16
```

Purpose:

```text
identify shared invariant bank
identify shared generator bank
identify transaction family
define batch width
select optional encoding modes
```

### Per-Transaction Compact Encoding

Each transaction row should be represented through compact columns:

```text
INVARIANT_IDX          varint
GENERATOR_IDX          varint
COEFF_ENCODED          compressed integer vector
RESIDUAL_SPARSE        sparse index/value stream
SIG_REF_OR_SIG_BYTES   32-64 bytes initially
BATCH_INDEX            implicit or delta encoded
```

Prefer columnar blocks over row-wise repeated structs when possible:

```text
invariant_idx[]
generator_idx[]
coeff_matrix[]
residual_stream[]
signature_block[]
```

Reason:

```text
columnar layout improves delta encoding, dictionary reuse, SIMD/vectorization, and cache locality
```

## Coefficient Encoding

Coefficient vectors should use integer-friendly encodings:

```text
delta encoding
zigzag varint
bit packing
shared coefficient dictionary
run-length encoding for repeated values
```

Recommended order for v0:

```text
1. delta encode
2. zigzag encode signed deltas
3. varint encode
4. add dictionary/bitpack later only if measurement proves value
```

## Residual Sparse Encoding

Residuals should be sorted and encoded as:

```text
(index_delta, value_delta)
```

Recommended techniques:

```text
sorted index delta
zigzag varint value
run-length encoding for contiguous spans
bit packing after v0 if needed
```

## Signature Handling

v0 should keep signature semantics conservative.

Allowed:

```text
raw signature bytes per transaction
signature reference if the current security model already supports it
```

Not in v0:

```text
signature aggregation
session-key authorization changes
batch authorization semantics
```

Reason:

```text
signature optimization changes the security model and should be a later signed stage
```

## Commitment

Each APFL batch packet should carry enough commitment material to prove deterministic reconstruction.

Candidate report and validation fields:

```text
apfl_batch_commitment
apfl_canonical_tx_hash_root
apfl_reconstruction_digest
```

The exact hash tree or digest scheme is not frozen by this document.

## Codec Tasks

### Task 1: Single IR Encoder

Target API:

```text
encode_apfl_ir_v0(ir) -> bytes
```

Goal:

```text
encode one APFLTransactionIR losslessly
```

### Task 2: Single IR Decoder

Target API:

```text
decode_apfl_ir_v0(bytes) -> ir
```

Goal:

```text
decode one APFLTransactionIR deterministically
```

### Task 3: Batch Encoder

Target API:

```text
encode_apfl_batch_v0(ir_batch) -> APFLBatchPacketV0 bytes
```

Responsibilities:

```text
deduplicate invariants
reuse generators
compress coefficient columns
compress sparse residuals
build signature block
build batch commitment
```

### Task 4: Batch Decoder

Target API:

```text
decode_apfl_batch_v0(packet_bytes) -> ir_batch
```

Requirements:

```text
100% deterministic
lossless for v0-supported fields
stable error reporting
canonical reconstruction support
```

### Task 5: Canonical Reconstruction Guard

Target checks:

```text
reconstruct_canonical_tx(ir) -> canonical_tx
canonical_tx_hash == legacy_tx_hash
signature verification passes
receipt semantics unchanged
ledger transition unchanged
```

## Benchmark Fields

Codec benchmark must report:

```text
legacy_bytes_per_tx
apfl_binary_bytes_per_tx
apfl_binary_bytes_total
apfl_binary_savings_ratio_bps
apfl_encode_elapsed_ms
apfl_decode_elapsed_ms
apfl_encode_per_tx_ns
apfl_decode_per_tx_ns
canonical_reconstruction_elapsed_ms
canonical_reconstruction_per_tx_ns
canonical_tx_hash_match_count
canonical_tx_hash_mismatch_count
```

Transport benchmark should continue to report:

```text
transport_payloads_delivered
receiver_transport_final_missing_count
receiver_transport_repair_received_count
receiver_transport_duplicate_received_count
transport_bytes_per_sec
business_transactions_per_sec
ledger_transactions_per_sec
```

## Success Criteria v0

First success condition:

```text
APFL binary batch decodes deterministically
canonical reconstruction passes
tx hash/signature/ledger/receipt semantics are preserved
bytes_per_tx materially decreases below 247 B/tx
```

Target acceptance:

```text
apfl_binary_bytes_per_tx <= 120 B / tx
canonical_tx_hash_mismatch_count = 0
signature_verify_error_count = 0
ledger mismatch = 0
```

## Strategic Meaning

This stage defines the minimum expression of a transaction.

It is not just network optimization.

It moves NOVOVM from:

```text
serialized transaction transfer
```

to:

```text
algebraic transaction expression transfer
```

The long-term execution path becomes:

```text
APFL IR -> AOEM execution
```

instead of:

```text
raw tx bytes -> decode -> AOEM execution
```

## Non-Goals

Do not implement these in binary codec v0:

```text
universal EVM APFL codec
signature aggregation
session-key authorization
multi-asset generalization
multi-method ABI compression
GPU execution kernel changes
NOVORUDP transport semantic changes
```

Those are later stages after `native_transfer_batch_v0` is signed.
