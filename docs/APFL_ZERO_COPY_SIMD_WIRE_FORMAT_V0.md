# APFL Zero-Copy SIMD Wire Format v0

Date: 2026-06-30

Status: `WIRE LAYOUT DESIGN / DO NOT IMPLEMENT DIRECTLY`

This document captures the next APFL transaction payload optimization layer: turning APFL IR from object-oriented encoding into a zero-copy, columnar, SIMD-friendly wire layout.

It is a design and task document. It does not authorize code implementation.

## Core Thesis

The next bottleneck after APFL IR design is:

```text
encode/decode CPU copy
Vec allocation
per-transaction serialization
intermediate objects
cache misses
```

The target transformation is:

```text
APFL IR as objects
  -> APFL IR as memory layout
```

This is not a NOVORUDP semantic change.

NOVORUDP still transports opaque bytes:

```text
NOVORUDP TransportFrameV0 DATA payload = APFL zero-copy wire bytes
```

The APFL codec layer owns the wire layout.

## Design Principles

Hard principles:

```text
no heap allocation per tx
no Vec clone per tx
no per-tx serialization loop as the hot path
no intermediate materialized object in the decode hot path
direct slice-based encoding
SIMD-friendly columnar layout
batch-first design
```

Acceptable v0 compromise:

```text
safe reference implementation may exist for correctness,
but the target hot path must be batch slice / view based.
```

## Batch Wire Layout v0

High-level packet layout:

```text
[BatchHeader]
[InvariantColumn]
[GeneratorColumn]
[CoeffMatrix]
[ResidualStream]
[SignatureBlock]
[Commitment]
```

### BatchHeader

Target size:

```text
16-32 bytes
```

Fields:

```text
magic
version
invariant_table_ref
generator_table_ref
batch_size
tx_family_id
flags
alignment_or_reserved
```

Purpose:

```text
identify APFL codec version
identify shared invariant table
identify shared generator table
define batch width
define transaction family
define optional column encodings
```

## Columnar Layout

Do not store:

```text
tx1, tx2, tx3, tx4, ...
```

Store:

```text
invariant_id[]:
  i1, i2, i3, i4, ...

generator_id[]:
  g1, g2, g3, g4, ...

coeff_matrix[]:
  c1, c2, c3, c4, ...

residual_stream:
  sparse residual blocks

signature_block:
  aligned signature or signature-ref region
```

Reason:

```text
columnar layout improves cache locality,
enables SIMD scan/compare/pack operations,
and avoids repeated row object construction.
```

## Wire View Types

Conceptual target:

```text
APFLWireBatchView<'a> {
  header: BatchHeaderView<'a>,
  invariant_column: &'a [u16],
  generator_column: &'a [u16],
  coeff_matrix: &'a [i32],
  residual_stream: &'a [u8],
  signature_block: &'a [u8],
}
```

The exact Rust layout is not frozen.

Rules:

```text
views borrow from the packet bytes
views do not allocate per tx
views do not clone columns
views expose slices for SIMD / vectorized processing
```

## Zero-Copy Encoder Target

Conceptual API:

```text
encode_batch_view(batch: &APFLWireBatchView) -> &[u8]
```

Requirement:

```text
the hot path maps prepared batch columns into one packet buffer
without per-transaction serialization
```

Practical v0 note:

```text
a builder may allocate one contiguous packet buffer,
but it must avoid per-tx heap allocation and per-tx clone.
```

## Decoder View Target

Conceptual API:

```text
decode_view(wire: &[u8]) -> APFLWireBatchView
```

Requirement:

```text
decode validates header and bounds,
then returns borrowed column slices.
```

Do not make the default decode path:

```text
wire bytes -> Vec<APFLTransactionIR> -> AOEM
```

Preferred path:

```text
wire bytes -> APFLWireBatchView -> AOEM batch view execution
```

## SIMD Optimization Points

### Column SIMD Scan

Use case:

```text
load invariant_id lanes
compare across batch
detect repeated invariant groups
prepare reuse plan
```

### Coefficient Vector Packing

Use case:

```text
int16 / int32 packed lanes
SIMD add / multiply / delta apply
```

### Residual Sparse Lane

Use case:

```text
index delta stream
SIMD-assisted decode or scan
batch residual apply
```

### Signature Batch View

Use case:

```text
aligned signature block
batch verify or GPU verify pipeline later
```

Signature aggregation is not part of v0.

## Performance Targets

Byte target progression:

```text
current expanded baseline: ~247 B / tx
APFL binary v0 target: 80-120 B / tx
zero-copy SIMD layout target: 50-80 B / tx
future structured reuse target: 30-60 B / tx
```

Execution target:

```text
reduce encode/decode allocation
reduce copy cost
reduce cache misses
prepare AOEM batch execution for SIMD/GPU
```

## Engineering Task Package

### Task 1: Wire Format Struct

Define:

```text
BatchHeader
APFLWireBatchView
APFLWireBatchBuilder
```

The builder may own one contiguous buffer.
The view must borrow slices from the buffer.

### Task 2: Zero-Copy Batch Decoder

Implement:

```text
decode_apfl_wire_batch_view_v0(wire: &[u8]) -> APFLWireBatchView
```

Requirements:

```text
bounds checked
deterministic
no per-tx allocation
stable error codes
```

### Task 3: Batch View Encoder

Implement:

```text
encode_apfl_wire_batch_v0(columns) -> Vec<u8>
```

v0 may allocate one contiguous output buffer.

Forbidden:

```text
per-tx heap allocation
per-tx Vec clone
row-wise repeated serialization in the hot path
```

### Task 4: SIMD Batch Processor

Implement a first safe abstraction:

```text
process_apfl_batch_columns_v0(view)
```

It may start scalar but must preserve columnar interfaces so SIMD can be added without changing the wire format.

### Task 5: Benchmark

Report:

```text
apfl_wire_bytes_per_tx
apfl_wire_encode_elapsed_ms
apfl_wire_decode_view_elapsed_ms
apfl_wire_decode_alloc_count
apfl_wire_copy_bytes_total
apfl_wire_cache_friendly_layout = true
```

If platform counters are available later:

```text
cache_miss_rate
memory_bandwidth_usage
simd_utilization
```

## Non-Goals

Do not:

```text
change NOVORUDP DATA / REPAIR / ACK semantics
make NOVORUDP inspect APFL columns
change ledger or receipt semantics
add signature aggregation
add GPU execution kernel changes
claim zero-copy if packet construction still clones per tx
```

## First Success Criteria

```text
decode_view returns valid borrowed column slices
canonical reconstruction still passes
no per-tx allocation in decode hot path
bytes_per_tx is lower than expanded batch baseline
AOEM adapter can consume the batch view or reconstruct canonical txs from it
```
