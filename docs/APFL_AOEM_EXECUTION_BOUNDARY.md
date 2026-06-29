# APFL / AOEM Execution Boundary

Date: 2026-06-30

Status: `ARCHITECTURE BOUNDARY / HARD RULE`

This document fixes the execution boundary between APFL and AOEM.

## Core Rule

NOVOVM must not introduce a second execution kernel.

```text
AOEM is the single physical execution kernel.
APFL is the structure and execution graph language above AOEM.
```

APFL may generate, rewrite, and optimize execution graphs.

APFL must not replace AOEM.

## Layer Model

### Layer 1: NOVORUDP Transport

Responsibilities:

```text
bytes
packets
batch transport
zero-copy network flow
DATA / REPAIR / ACK
```

NOVORUDP is the bit/byte transport layer.

### Layer 2: APFL Structure Layer

Responsibilities:

```text
transaction IR
invariant / generator / coeff / residual
batch structure
execution graph definition
graph rewrite rules
kernel selection policy
```

APFL is the model / structure / graph language layer.

### Layer 3: AOEM Runtime

Responsibilities:

```text
compute execution
tensor / graph runtime
state transition
GPU / CPU / Vulkan backend
ledger commit
receipt generation
```

AOEM is the deterministic physical execution layer.

## Correct Interpretation Of Self-Modifying Execution

The phrase:

```text
self-modifying execution kernel
```

is potentially misleading.

Correct meaning:

```text
APFL IR can generate or rewrite the AOEM execution graph.
```

Incorrect meaning:

```text
create AOEM v2
replace AOEM runtime
let runtime mutate itself without graph validation
```

Correct flow:

```text
APFL IR
  -> Execution Plan Generator
  -> AOEM Execution Graph
  -> AOEM Runtime
```

Incorrect flow:

```text
APFL IR
  -> AOEM v1
  -> AOEM v2
```

## Allowed APFL Capabilities

APFL may:

```text
generate execution graphs
rewrite graph structure
fuse graph nodes
select AOEM kernels
choose CPU/GPU/Vulkan backend policy
shape batches
plan invariant reuse
plan generator fusion
plan residual merge
```

APFL may not:

```text
execute outside AOEM
fork ledger semantics
create a second kernel state machine
change AOEM determinism guarantees
change receipt or replay semantics
```

## AOEM Role

AOEM is:

```text
the fixed deterministic execution machine
APFL's physical executor
the owner of state transition execution
the owner of backend dispatch
```

AOEM is not:

```text
deprecated by APFL
replaced by APFL
duplicated by APFL
```

## Future Work Positioning

Any task named like:

```text
APFL execution graph compiler
AOEM kernel selection engine
kernel fusion scheduler
self-modifying execution layer
```

must be interpreted as:

```text
APFL produces or optimizes an execution graph for AOEM to run.
```

It must not be interpreted as:

```text
add a second executor beside AOEM.
```

## Hard Guard

Every APFL-generated AOEM graph must have:

```text
deterministic graph id
versioned schema
semantic equivalence checks
ledger state root guard
receipt equivalence guard
rollback path
```

This boundary prevents NOVOVM from splitting into multiple incompatible execution kernels.
