# NOVOVM APFL Full-Chain Roadmap

Date: 2026-06-30

Status: `ARCHITECTURE ROADMAP / DO NOT IMPLEMENT DIRECTLY`

This document converts the APFL transaction architecture discussion into an engineering roadmap for NOVOVM, NOVORUDP, APFL, and AOEM.

It is a knowledge and task-planning document. It does not authorize immediate code implementation.

## Final Shape

```text
User / DApp / RPC
  -> NOVORUDP Transport Layer
  -> APFL Transaction IR Codec Layer
  -> AOEM Execution Layer
  -> NOVO State / Ledger Layer
```

The long-term system should move from:

```text
raw transaction bytes
```

to:

```text
APFL transaction structures
```

Where transaction batches are represented as:

```text
shared invariants
+ generator references
+ coefficient vectors
+ sparse residuals
+ signature references or signature blocks
+ batch index / commitment
```

## Correct Layer Boundary

Physical transport still requires bytes.

Therefore:

```text
NOVORUDP transports encoded APFL payload bytes.
NOVORUDP does not interpret APFL IR.
NOVORUDP does not execute APFL formulas.
NOVORUDP does not know transaction semantics.
```

Correct statement:

```text
NOVORUDP = transport for opaque APFL-encoded payload bytes
```

Incorrect statement:

```text
NOVORUDP = APFL semantic execution protocol
```

This boundary prevents the old mixed-layer failure from returning.

## Layer Responsibilities

### Layer 0: NOVORUDP Transport

Responsibilities:

```text
DATA / REPAIR / ACK
packet ordering
delivery
dedupe
repair
pacing
flow control
transport finalization
```

Input and output:

```text
opaque bytes
```

### Layer 1: APFL Transaction IR Codec

Responsibilities:

```text
define APFLTransactionIR
encode IR batch into compact payload bytes
decode compact payload bytes into IR batch
deduplicate invariants
reuse generators
compress coefficient vectors
encode sparse residuals
carry signature references or signature blocks
prove canonical reconstruction
```

### Layer 2: AOEM Execution

Responsibilities:

```text
consume APFLTransactionIR or canonical reconstructed transactions
lookup invariants
execute generators
apply coefficients
apply residuals
verify signatures / commitments
perform state transition
emit digest / receipt
close ledger
```

## APFLTransactionIR v0 Sketch

Initial conceptual shape:

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

The exact Rust representation is not frozen by this document.

The first implementation should choose a narrow transaction family before attempting a universal IR.

## First Family

Start with:

```text
native_transfer_batch_v0
```

Reason:

```text
native transfer batches are regular enough to expose APFL benefits early.
```

Initial constraints:

```text
same chain
same transaction type
same asset
same fee policy
same signature scheme
same method / generator
limited receiver or account family
```

Out of scope for v0:

```text
general EVM contract calls
arbitrary ABI method batches
multi-asset batches
signature aggregation
session-key authorization changes
consensus semantics changes
ledger semantics changes
```

## Engineering Task Package

### Task 1: APFL Transaction IR Attribution

Goal:

```text
measure the current expanded transaction byte and time costs
```

Required report fields:

```text
legacy_bytes_per_tx
payload_bytes_per_payload_min
payload_bytes_per_payload_p50
payload_bytes_per_payload_p95
payload_bytes_per_payload_max
sender_batch_build_elapsed_ms
sender_business_encode_elapsed_ms
sender_payload_copy_elapsed_ms
sender_socket_send_elapsed_ms
receiver_business_decode_elapsed_ms
receiver_business_decode_per_tx_ns
receiver_aoem_execute_elapsed_ms
receiver_aoem_execute_per_tx_ns
receiver_ledger_close_elapsed_ms
```

No behavior change.

### Task 2: APFLTransactionIR v0 Definition

Goal:

```text
define a minimal IR for native transfer batch semantics
```

Requirements:

```text
explicit invariant id
explicit generator id
columnar per-transaction parameters
sparse residual support
signature material or signature reference
batch index
commitment / digest field
```

### Task 3: NativeTransferBatchV0 Codec

Goal:

```text
encode native transfer batches as APFL compact payloads
decode them deterministically
```

Requirements:

```text
batch header stores shared fields
per-transaction columns store deltas or compact values
decode is deterministic
canonical reconstruction is possible
```

### Task 4: Canonical Reconstruction Guard

Goal:

```text
prove APFL compact batches reconstruct canonical transaction semantics
```

Required checks:

```text
canonical_reconstruction_count
canonical_reconstruction_error_count
canonical_tx_hash_match_count
canonical_tx_hash_mismatch_count
signature_verify_count
signature_verify_error_count
```

Hard rule:

```text
hash / signature / receipt / ledger replay semantics must remain explicit and testable
```

### Task 5: AOEM APFL Execution Adapter

Goal:

```text
let AOEM consume APFLTransactionIR or reconstructed canonical transaction objects
```

Initial mode:

```text
safe adapter mode may reconstruct canonical tx first
```

Later mode:

```text
AOEM-native structured execution may execute IR directly if semantic equivalence is proven
```

### Task 6: Batch Benchmark

Measure:

```text
legacy_bytes_per_tx
apfl_bytes_per_tx
savings_ratio_bps
transport_payloads_per_sec
business_transactions_per_sec
ledger_transactions_per_sec
sender encode/copy/socket cost
receiver decode/AOEM/ledger cost
```

Initial success:

```text
APFL batch correctness passes
bytes_per_tx decreases materially below 247 B/tx
ledger semantics remain unchanged
```

## Expected Byte Target

Current expanded baseline:

```text
~247 B / tx
```

First APFL target:

```text
80-120 B / tx
```

More aggressive future target:

```text
50-80 B / tx
```

Sub-50 B/tx likely requires security model changes such as:

```text
signature aggregation
session keys
batch authorization
account-family authorization
```

Those are not part of APFL Transaction IR v0.

## Core Strategic Rule

The goal is not only to optimize blockchain throughput.

The goal is to define NOVOVM execution objects as algebraic structures:

```text
APFL describes what the computation is.
NOVORUDP transports the compact structural encoding.
AOEM executes the structure.
```

This same design philosophy should apply to:

```text
blockchain transaction batches
AI parameter / model execution
future AOEM GPU execution kernels
```

## Next Decision

Do not start by building a full universal APFL transaction codec.

The next safe implementation stage is:

```text
NOVORUDP Batch Payload Size / Serialization Attribution
```

Then:

```text
APFLTransactionIR v0
NativeTransferBatchV0
Canonical Reconstruction Guard
AOEM APFL Execution Adapter
```
