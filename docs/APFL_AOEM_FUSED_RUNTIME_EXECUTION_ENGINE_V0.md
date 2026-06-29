# APFL / AOEM Fused Runtime Execution Engine v0

Date: 2026-06-30

Status: `RUNTIME INTEGRATION DESIGN / DO NOT IMPLEMENT DIRECTLY`

This document defines the future AOEM fused runtime execution engine layer for APFL-generated and fusion-scheduled execution graphs.

It is a design document. It does not authorize immediate code implementation.

## Boundary

This layer does not create a second execution runtime.

```text
AOEM remains the single execution kernel.
Fused Runtime Execution Engine is the AOEM scheduling, memory planning, dispatch, and commit orchestration layer.
```

Required flow:

```text
APFL IR
  -> Execution Graph Compiler
  -> Kernel Fusion Scheduler
  -> Fused AOEM Kernels
  -> AOEM Fused Runtime Execution Engine
  -> State / Ledger / Consensus
```

## Core Thesis

Fused kernels are not enough.

The runtime must coordinate:

```text
compute
memory
schedule
state commit
```

The target transformation is:

```text
fused kernels as isolated executable units
  -> fused kernels as a continuous AOEM execution pipeline
```

## Unified Execution Scheduler

Responsibilities:

```text
batch scheduling
kernel dispatch ordering
dependency ordering
AOEM execution control
sync point management
fallback path selection
```

Conceptual API:

```text
schedule_fused_execution(fused_graph) -> ExecutionPlan
```

Report fields:

```text
aoem_runtime_schedule_batch_count
aoem_runtime_schedule_kernel_count
aoem_runtime_schedule_dependency_count
aoem_runtime_schedule_sync_point_count
aoem_runtime_schedule_elapsed_ns
```

## Memory Planner

Responsibilities:

```text
zero-copy buffer reuse
cache locality
fused kernel memory layout
intermediate buffer elimination
host/device transfer planning
alignment planning
```

Conceptual API:

```text
plan_runtime_memory(execution_plan) -> MemoryMap
```

Report fields:

```text
aoem_runtime_memory_buffer_count
aoem_runtime_memory_reused_buffer_count
aoem_runtime_memory_bytes_total
aoem_runtime_memory_intermediate_bytes
aoem_runtime_memory_host_device_transfer_count
aoem_runtime_memory_zero_copy_region_count
aoem_runtime_memory_plan_elapsed_ns
```

## Compute Dispatcher

Responsibilities:

```text
choose CPU / GPU / SIMD lane execution
dispatch fused kernels
coordinate backend execution order
track kernel completion
surface backend errors
```

Conceptual API:

```text
dispatch_runtime_kernels(execution_plan, memory_map) -> KernelExecutionStream
```

Report fields:

```text
aoem_runtime_dispatch_kernel_count
aoem_runtime_dispatch_cpu_count
aoem_runtime_dispatch_gpu_count
aoem_runtime_dispatch_simd_count
aoem_runtime_dispatch_backend
aoem_runtime_dispatch_elapsed_ns
```

## Pipeline Executor

Responsibilities:

```text
execute fused kernel stream
advance dependency graph
reuse memory plan
collect state updates
collect receipts / digests
handle fallback on validation failure
```

Conceptual API:

```text
execute_fused_pipeline(stream) -> StateUpdates
```

Report fields:

```text
aoem_runtime_pipeline_elapsed_ns
aoem_runtime_pipeline_batch_tps
aoem_runtime_pipeline_tx_tps
aoem_runtime_pipeline_kernel_error_count
aoem_runtime_pipeline_fallback_count
```

## AOEM State Commit Layer

Responsibilities:

```text
state transition commit
ledger write
digest generation
receipt generation
deterministic replay guard
```

Conceptual API:

```text
commit_runtime_state(state_updates) -> LedgerEntry
```

Report fields:

```text
aoem_runtime_commit_elapsed_ns
aoem_runtime_commit_state_update_count
aoem_runtime_commit_ledger_entry_count
aoem_runtime_commit_receipt_count
aoem_runtime_commit_digest
```

## Validation Guard

Fused runtime execution must match the existing deterministic AOEM baseline.

Required:

```text
same state root
same ledger entries
same receipts
same replay result
same tx hash semantics
same signature verification result
fallback path available
```

Report fields:

```text
aoem_runtime_baseline_equivalence_checked
aoem_runtime_baseline_mismatch_count
aoem_runtime_state_root_match
aoem_runtime_receipt_mismatch_count
aoem_runtime_replay_match
aoem_runtime_fallback_available
```

## Benchmark Metrics

Measure:

```text
end_to_end_latency
batch TPS
transaction TPS
memory bandwidth usage
kernel utilization
scheduling overhead
dispatch overhead
commit overhead
fallback count
semantic mismatch count
```

Suggested fields:

```text
aoem_runtime_end_to_end_elapsed_ns
aoem_runtime_transactions_per_sec
aoem_runtime_batches_per_sec
aoem_runtime_memory_bandwidth_bytes_per_sec
aoem_runtime_kernel_utilization_bps
aoem_runtime_scheduler_overhead_bps
aoem_runtime_dispatch_overhead_bps
aoem_runtime_commit_overhead_bps
```

## Relationship To Earlier Layers

Execution Graph Compiler:

```text
builds optimized logical graph
```

Kernel Fusion Scheduler:

```text
turns graph nodes into fused AOEM kernel plan
```

Fused Runtime Execution Engine:

```text
executes fused kernel plan as one coordinated AOEM runtime pipeline
```

## Non-Goals

Do not:

```text
create AOEM v2
replace AOEM runtime
change ledger semantics
change receipt semantics
change transaction hash semantics
make NOVORUDP participate in scheduling
skip baseline deterministic equivalence
introduce hardware-specific behavior without deterministic fallback
```

## First Success Criteria

```text
ExecutionPlan is produced from fused graph.
MemoryMap is produced and reported.
KernelExecutionStream is dispatched.
StateUpdates are committed through AOEM.
Fused runtime output matches deterministic baseline.
Fallback path exists.
Runtime timing metrics are reported.
```

## Strategic Meaning

This stage moves NOVOVM from:

```text
optimized computation graph
```

to:

```text
coordinated computation runtime
```

In long-term terms:

```text
APFL is the instruction / graph language.
AOEM is the execution kernel.
NOVORUDP is the I/O transport subsystem.
The fused runtime coordinates AOEM execution of APFL-generated work.
```
