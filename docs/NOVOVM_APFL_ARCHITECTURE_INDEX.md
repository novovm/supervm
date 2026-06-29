# NOVOVM APFL Architecture Index

Date: 2026-06-30

Status: `ARCHITECTURE INDEX`

This index orders the APFL / NOVOVM / NOVORUDP / AOEM architecture documents by implementation dependency.

It distinguishes:

```text
signed baseline
near-term implementation
future architecture
```

## Signed Baseline

These documents describe already signed NOVORUDP transport work:

```text
NOVORUDP_BOUNDARY.md
NOVORUDP_TRANSPORT_BOUNDARY_RESET_V0_SIGNOFF.md
NOVORUDP_LEGACY_MIXED_REPAIR_REMOVAL_SIGNOFF.md
NOVORUDP_DATA_PACING_SIGNOFF.md
NOVORUDP_FIXED_PACING_PERFORMANCE_BASELINE.md
```

Current signed baseline:

```text
NOVORUDP TransportFrameV0 is production sustained signed.
Legacy mixed repair path is removed from production.
Recommended fixed pacing tier is 128 / 1ms.
Batch payload correctness is signed, but expanded batch throughput is not final.
```

## Current Implementation Target

The current implementation target is narrow and testable:

```text
native_transfer_batch_v0
```

Confirmed constraints:

```text
only native_transfer_batch_v0
no generic EVM contract call codec
no APFL transform for external EVM plugin / passthrough transactions
keep per-transaction original signatures
canonical hash uses current NOVOVM native tx hash rule
receiver reconstructs canonical tx before existing AOEM execution
NOVORUDP transport ABI does not change
```

First acceptance:

```text
bytes_per_tx <= 120 B
canonical_tx_hash_match_count = tx_count
signature_verify_error_count = 0
ledger completed = tx_count
transport final_missing = 0
```

Current v0 implementation:

```text
Commit: 08ad5d8
Mode: native_transfer_apfl_v0
Transport ABI: unchanged NovoRudpTransportFrameV0 opaque payload bytes
Receiver path: APFL compact payload -> canonical native tx reconstruction -> existing AOEM adapter
```

Local smoke result:

```text
64 payload x 128 tx = 8192 tx
transport delivered = 64
final_missing = 0
business decoded = 8192
ledger tx completed = 8192
apfl_bytes_per_tx = 32
legacy_bytes_per_tx = 233
apfl savings = 8599 bps
canonical hash match = 8192
canonical mismatch = 0
signature errors = 0
```

This confirms the first APFL density rule:

```text
use one NOVORUDP payload as an APFL algebraic batch object,
not as a container of fully expanded transaction wires.
```

Primary documents:

```text
NOVORUDP_TRANSACTION_APFL_CODEC_NOTES.md
APFL_IR_BINARY_ENCODING_SCHEME_V0.md
```

## Implementation Order

### 1. Attribution Before Codec

Document:

```text
NOVORUDP_TRANSACTION_APFL_CODEC_NOTES.md
```

Task:

```text
Measure expanded batch payload size and serialization cost.
```

Purpose:

```text
Identify whether sender encode, copy, socket send, receiver decode, AOEM, or ledger dominates.
```

### 2. APFL Binary Codec v0

Document:

```text
APFL_IR_BINARY_ENCODING_SCHEME_V0.md
```

Task:

```text
APFLTransactionIR -> compact bytes -> APFLTransactionIR
```

Purpose:

```text
Reduce bytes_per_tx while preserving canonical reconstruction.
```

### 3. Native Transfer Algebraic Batch v0

Documents:

```text
NOVORUDP_TRANSACTION_APFL_CODEC_NOTES.md
APFL_IR_BINARY_ENCODING_SCHEME_V0.md
```

Task:

```text
Encode a native transfer batch as shared header + per-tx columns.
Decode and reconstruct canonical native tx.
```

Purpose:

```text
Prove APFL transaction representation can reduce bytes without changing execution semantics.
```

### 4. Zero-Copy SIMD Wire Format

Document:

```text
APFL_ZERO_COPY_SIMD_WIRE_FORMAT_V0.md
```

Task:

```text
Move from object encoding to columnar batch views and borrowed decode slices.
```

Purpose:

```text
Reduce copy/allocation and prepare SIMD/AOEM batch execution.
```

### 5. Batch Invariant Reuse

Document:

```text
APFL_BATCH_INVARIANT_REUSE_SIMD_FUSION_V0.md
```

Task:

```text
Deduplicate invariants and generators within one batch.
```

Purpose:

```text
Reduce repeated compute inside a batch.
```

### 6. Cross-Batch Cache

Document:

```text
APFL_CROSS_BATCH_INVARIANT_CACHE_EXECUTION_GRAPH_FUSION_V0.md
```

Task:

```text
Reuse invariants/generators across batches.
```

Purpose:

```text
Reduce system-level repeated compute.
```

### 7. Execution Graph Compiler

Documents:

```text
APFL_AOEM_EXECUTION_BOUNDARY.md
APFL_EXECUTION_GRAPH_COMPILER_V0.md
```

Task:

```text
APFL IR -> AOEM execution graph.
```

Purpose:

```text
Compile structure into a deterministic AOEM execution plan.
```

### 8. AOEM Kernel Fusion

Document:

```text
APFL_AOEM_KERNEL_FUSION_SCHEDULER_V0.md
```

Task:

```text
Fuse AOEM kernels and reduce memory round trips.
```

Purpose:

```text
Optimize physical execution cost without creating AOEM v2.
```

### 9. Fused Runtime

Document:

```text
APFL_AOEM_FUSED_RUNTIME_EXECUTION_ENGINE_V0.md
```

Task:

```text
Coordinate AOEM schedule, memory, dispatch, and commit.
```

Purpose:

```text
Turn fused kernels into a continuous AOEM execution pipeline.
```

## Future Architecture

These documents are future-facing and must not be treated as implemented:

```text
APFL_ADAPTIVE_TRANSACTION_PATTERN_LEARNING_V0.md
APFL_AUTONOMOUS_STRUCTURE_EVOLUTION_COMPILER_V0.md
NOVOVM_APFL_FULL_CHAIN_ROADMAP.md
NOVOVM_APFL_STRUCTURE_CENTRIC_PARADIGM.md
NOVOVM_FUSED_COMPUTE_OPERATING_SYSTEM_V0.md
```

Hard rule:

```text
Future architecture documents are not signed runtime behavior.
They are design direction only until implemented and explicitly signed.
```

## Global Boundary

```text
NOVORUDP transports opaque bytes.
APFL defines the structure encoded in those bytes.
AOEM executes the structure.
```

External EVM boundary:

```text
APFL is only for NOVOVM-native / AOEM-executed transaction structures.
External Ethereum-compatible plugin traffic must remain standard EVM wire / RPC payload.
External EVM nodes cannot decode APFL IR.
```

Do not:

```text
make NOVORUDP interpret APFL or transactions
APFL-transform external EVM plugin passthrough traffic
create AOEM v2
change canonical tx hash semantics
change signature semantics
change ledger / receipt semantics
skip deterministic replay guards
```
