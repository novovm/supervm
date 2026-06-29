# NOVOVM APFL Structure-Centric Paradigm

Date: 2026-06-30

Status: `ARCHITECTURE KNOWLEDGE / DO NOT IMPLEMENT DIRECTLY`

This document teaches NOVOVM the APFL-oriented design philosophy that should guide future NOVOVM, NOVORUDP, AOEM, and AI-runtime work.

It is not a code change request.

## Core Thesis

NOVOVM should move from:

```text
wire-centric programming
```

to:

```text
structure-centric programming
```

Meaning:

```text
Do not treat a transaction, tensor, model parameter, or execution object as only raw serialized bytes.
Treat it as an algebraic structure with shared invariants, formulas, deltas, and residuals.
```

APFL is the design philosophy for this shift.

## Important Boundary

Bytes cannot disappear.

Physical transport still requires:

```text
bytes
packets
buffers
serialization
checksums
auth
network framing
```

The goal is not:

```text
no bytes
```

The goal is:

```text
bytes become the compact encoding of an algebraic structure,
not the repeated full materialization of raw objects.
```

## Three-Layer Model

### Layer 0: NOVORUDP Physical Transport

Responsibilities:

```text
packet framing
DATA / REPAIR / ACK
sequence
delivery
dedupe
repair
pacing
transport finalization
```

Rules:

```text
NOVORUDP only sees opaque bytes.
NOVORUDP must not know EVM, AOEM, ledger, APFL formulas, or transaction semantics.
```

### Layer 1: APFL Codec / IR Layer

Responsibilities:

```text
define algebraic object representation
encode structure into compact bytes
decode compact bytes into APFL IR or canonical materialized objects
track invariant banks, formula banks, deltas, residuals, and commitments
```

This is where transaction size and semantic density are improved.

### Layer 2: AOEM Execution Layer

Responsibilities:

```text
execute APFL IR or reconstructed canonical objects
verify signatures / commitments
perform state transition
write ledger
generate receipts
support deterministic replay
```

AOEM should eventually execute structured APFL objects directly where semantics allow it.

## APFL Object Pattern

Traditional model:

```text
object_1 full bytes
object_2 full bytes
object_3 full bytes
...
```

APFL model:

```text
InvariantBank
FormulaBank
APFLProgram
DeltaStream
ResidualStream
Commitment
```

Generic APFL object:

```text
APFLObject:
  invariant_ref
  formula_ref
  coefficient_vector
  sparse_residual
  commitment
```

For transactions:

```text
APFLTransactionIR:
  invariant_id
  generator_id
  coeff_vector
  residual_sparse
  signature_ref
  batch_index
```

For AI parameters:

```text
APFLParameterIR:
  invariant_id
  formula_id
  coefficient_block
  residual_block
  precision_policy
```

The two domains differ in validation and execution rules, but they share the same APFL principle:

```text
shared structure + parameters + residuals
```

## NOVOVM Transaction Direction

Current batch payload v0 proved correctness but exposed size cost:

```text
txs_per_payload = 128
bytes_per_tx ~= 247 B
payload ~= 31.6 KB
```

This means the current payload is still:

```text
128 fully expanded transaction wires
```

The next transaction model should move toward:

```text
1 batch template
+ shared invariants
+ per-transaction parameter columns
+ residual streams
+ signatures or signature references
+ commitment
```

First narrow family:

```text
native_transfer_batch_v0
```

The first family should optimize one regular case before attempting generic EVM calls.

## Semantic Safety Rules

APFL encoding may change representation, but not execution semantics.

Required:

```text
canonical transaction reconstruction remains possible
transaction hash rules remain explicit
signature verification remains valid
ledger state transition remains deterministic
receipt semantics remain unchanged
replay remains possible
```

If AOEM later executes APFL IR directly without materializing full legacy tx bytes, it must still be semantically equivalent to canonical execution.

## What Future Agents Must Not Do

Do not:

```text
put APFL formulas inside NOVORUDP transport state
make transport ACK depend on APFL decode or AOEM execution
remove bytes from the physical network layer
claim APFL removes the need for serialization
optimize by breaking canonical hash / receipt / replay semantics
reintroduce business-aware repair into NOVORUDP
blindly increase txs_per_payload while each tx remains fully expanded
```

## Correct Next Development Sequence

Before implementing a compact APFL transaction codec:

```text
1. Measure payload byte composition.
2. Measure sender encode / copy / socket send cost.
3. Measure receiver decode / AOEM / ledger cost.
4. Identify repeated fields and true residuals.
5. Define a narrow APFLTransactionIR family.
6. Prove canonical reconstruction.
7. Prove hash / signature / receipt equivalence.
8. Then run NOVORUDP batch transport with compact APFL payload bytes.
```

## Paradigm Summary

```text
NOVORUDP transports bytes.
APFL defines what those bytes mean structurally.
AOEM executes the structure.
```

The long-term NOVOVM direction is:

```text
wire-centric raw object transfer
  -> structure-centric APFL object transfer
  -> AOEM-native structured execution
```

This applies to both:

```text
blockchain transaction execution
AI parameter / model execution
```

The shared principle is:

```text
do not repeat materialized data when invariant structure plus small parameters can generate it.
```
