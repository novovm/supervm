# NOVOVM Fused Compute Operating System v0

Date: 2026-06-30

Status: `FINAL ARCHITECTURE DESIGN / DO NOT IMPLEMENT DIRECTLY`

This document captures the final NOVOVM system abstraction: APFL, AOEM, NOVORUDP, and fused runtime execution as a compute operating system.

It is a top-level architecture document. It does not authorize immediate implementation.

## Core Thesis

NOVOVM is moving from:

```text
blockchain VM + network transport
```

to:

```text
structure-centric compute operating system
```

In this model:

```text
APFL = instruction / structure language
AOEM = deterministic execution kernel
NOVORUDP = I/O transport subsystem
Fused Runtime = scheduler / memory / dispatch orchestration
State + Ledger = durable system state
```

## Boundary

Compute OS does not mean collapsing all layers into one undifferentiated runtime.

Correct interpretation:

```text
APFL, AOEM, NOVORUDP, state, and runtime are unified by an OS abstraction.
Each subsystem keeps its responsibility and boundary.
```

Incorrect interpretation:

```text
NOVORUDP executes APFL.
APFL replaces AOEM.
AOEM owns transport.
ledger semantics are changed by runtime optimization.
```

Hard rule:

```text
one AOEM execution kernel
one NOVORUDP transport layer
one APFL structure / instruction layer
explicit state and ledger semantics
```

## Final Stack

```text
APFL Instruction Set
  -> Execution Compiler
  -> Kernel Fusion Scheduler
  -> Fused Compute OS Scheduler
  -> AOEM Execution Runtime
  -> State / Ledger System
  -> NOVORUDP I/O Layer
```

More explicitly:

```text
User / DApp / RPC
  -> NOVORUDP I/O subsystem
  -> APFL encoded payload bytes
  -> APFL instruction / IR decode
  -> APFL execution graph compiler
  -> AOEM kernel fusion scheduler
  -> AOEM fused runtime execution engine
  -> state transition / ledger / receipts
```

## APFL Instruction Set

APFL is the compute OS instruction and structure language.

Conceptual instruction groups:

```text
invariant_op
generator_op
coeff_op
residual_op
state_op
commit_op
```

APFL instruction stream responsibilities:

```text
describe structured computation
reference invariant banks
reference generator banks
carry coefficient columns
carry sparse residuals
carry commitment material
preserve deterministic reconstruction
```

APFL must not:

```text
perform transport reliability
directly mutate ledger outside AOEM
replace AOEM kernels
```

## Compute OS Scheduler

Responsibilities:

```text
instruction scheduling
graph execution ordering
kernel fusion dispatch
AOEM execution binding
dependency management
fallback path selection
```

Conceptual API:

```text
schedule_instructions(instruction_stream) -> ExecutionPlan
```

Report fields:

```text
novovm_os_instruction_count
novovm_os_execution_plan_count
novovm_os_schedule_elapsed_ns
novovm_os_dependency_count
novovm_os_fallback_path_available
```

## Unified Memory + State Layer

The Compute OS must coordinate memory and state, but must not blur semantic ownership.

Memory-managed domains:

```text
invariant cache
generator cache
coefficient tensors
residual store
zero-copy packet buffers
AOEM intermediate buffers
state transition buffers
ledger commit buffers
```

Conceptual API:

```text
plan_compute_os_memory(execution_plan) -> ComputeOSMemoryMap
```

Report fields:

```text
novovm_os_memory_region_count
novovm_os_zero_copy_region_count
novovm_os_cache_region_count
novovm_os_state_region_count
novovm_os_memory_bytes_total
novovm_os_memory_reuse_ratio_bps
```

## Execution VM Layer

The Execution VM layer turns APFL instruction streams into AOEM runtime work.

Flow:

```text
APFL IR
  -> instruction stream
  -> execution VM planner
  -> AOEM runtime
```

Responsibilities:

```text
compile APFL instructions into AOEM-compatible execution plan
preserve deterministic execution
track graph / kernel / memory decisions
report execution cost
```

Conceptual API:

```text
execute_apfl_program(plan) -> AOEMRuntimeState
```

## NOVORUDP I/O Subsystem

NOVORUDP is the Compute OS I/O subsystem.

Responsibilities:

```text
batch packet I/O
APFL encoded payload transport
zero-copy network buffers
stream decode into APFL payload bytes
DATA / REPAIR / ACK reliability
```

NOVORUDP still transports opaque bytes.

It must not:

```text
inspect APFL instruction semantics
schedule AOEM kernels
depend on ledger close for transport accounting
```

Conceptual API:

```text
handle_novorudp_packets() -> APFLPayloadByteStream
```

## AOEM Runtime Kernel

AOEM is the Compute OS kernel.

Responsibilities:

```text
physical execution
CPU / GPU / Vulkan backend dispatch
state transition
receipt generation
ledger commit
deterministic replay
```

AOEM must remain:

```text
single authoritative execution kernel
deterministic
semantically guarded
fallback-capable
```

## Validation Guard

Compute OS optimizations must preserve:

```text
transaction hash semantics
signature verification semantics
state root
ledger entries
receipt outputs
deterministic replay
transport delivery independence
```

Required report fields:

```text
novovm_os_baseline_equivalence_checked
novovm_os_semantic_mismatch_count
novovm_os_state_root_match
novovm_os_receipt_mismatch_count
novovm_os_replay_match
novovm_os_transport_boundary_preserved
```

## Benchmark Metrics

Measure:

```text
instruction throughput
scheduling overhead
memory locality
I/O latency
AOEM execution cost
ledger commit cost
end-to-end transaction TPS
```

Suggested report fields:

```text
novovm_os_instruction_per_sec
novovm_os_transaction_per_sec
novovm_os_schedule_overhead_bps
novovm_os_memory_bandwidth_bytes_per_sec
novovm_os_io_latency_ns
novovm_os_aoem_execution_elapsed_ns
novovm_os_ledger_commit_elapsed_ns
```

## Engineering Task Package

### Task 1: APFL Instruction Definition

Define:

```text
APFLInstruction
APFLInstructionStream
APFLProgramId
APFLInstructionVersion
```

### Task 2: Instruction Scheduler

Conceptual API:

```text
schedule_instructions(stream) -> ExecutionPlan
```

### Task 3: Compute OS Kernel Interface

Conceptual API:

```text
execute(plan) -> AOEMRuntimeState
```

This is an AOEM interface, not a second kernel.

### Task 4: Unified Memory Layer

Conceptual API:

```text
manage_compute_os_memory(invariant, generator, coeff, residual, state) -> MemoryMap
```

### Task 5: I/O Subsystem Binding

Conceptual API:

```text
handle_novorudp_packets() -> APFLPayloadByteStream
```

### Task 6: Compute OS Benchmark

Metrics:

```text
instruction throughput
scheduling overhead
memory locality
I/O latency
AOEM execution cost
ledger commit cost
semantic mismatch count
```

## Relationship To Previous Layers

Previous layers:

```text
APFL IR
binary encoding
zero-copy wire
batch invariant reuse
cross-batch cache
execution graph compiler
kernel fusion scheduler
fused runtime execution engine
```

Compute OS v0:

```text
unifies these layers into a coherent system abstraction while preserving their boundaries.
```

## Non-Goals

Do not:

```text
collapse APFL / AOEM / NOVORUDP into one untyped subsystem
create AOEM v2
make NOVORUDP schedule compute
make APFL commit ledger state directly
remove physical byte transport
remove deterministic replay
skip semantic equivalence guards
claim production readiness before staged lower layers are signed
```

## First Success Criteria

The first Compute OS milestone should be architectural and observational:

```text
APFLInstruction stream is defined.
ExecutionPlan abstraction is defined.
NOVORUDP I/O boundary is preserved.
AOEM execution boundary is preserved.
MemoryMap abstraction is defined.
No live runtime behavior changes automatically.
Baseline equivalence guard remains mandatory.
```

## Strategic Meaning

NOVOVM is not only a blockchain, VM, or network protocol.

Long-term target:

```text
NOVOVM = APFL-structured compute operating system
```

Where:

```text
APFL is the instruction set.
AOEM is the kernel.
NOVORUDP is the I/O subsystem.
State / Ledger is durable system state.
Fused runtime is the scheduler and execution orchestrator.
```
