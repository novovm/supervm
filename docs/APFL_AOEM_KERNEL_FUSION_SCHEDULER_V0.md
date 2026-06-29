# APFL / AOEM Kernel Fusion Scheduler v0

Date: 2026-06-30

Status: `PHYSICAL EXECUTION OPTIMIZATION DESIGN / DO NOT IMPLEMENT DIRECTLY`

This document defines the AOEM kernel fusion scheduler layer for APFL-generated execution graphs.

It is a design document. It does not authorize immediate code implementation.

## Boundary

This layer does not create a second AOEM kernel.

```text
AOEM remains the single physical execution runtime.
The fusion scheduler plans how AOEM kernels are fused and dispatched.
```

Required flow:

```text
APFL IR
  -> Execution Graph Compiler
  -> Optimized Execution Graph
  -> AOEM Kernel Fusion Scheduler
  -> Fused AOEM Kernel Plan
  -> AOEM Runtime
```

## Core Thesis

An optimized execution graph is not automatically execution-cost optimal.

The final physical bottlenecks are:

```text
kernel launch overhead
intermediate buffers
global memory read/write
CPU/GPU synchronization
cache misses
memory round trips
```

The scheduler target is:

```text
multiple graph nodes
  -> fewer fused AOEM kernels
  -> less memory movement
  -> lower launch overhead
```

## Example

Before fusion:

```text
invariant_lookup_kernel
generator_compute_kernel
coeff_simd_kernel
residual_apply_kernel
state_update_kernel
```

After fusion:

```text
fused_invariant_generator_coeff_kernel
fused_residual_state_commit_kernel
```

The exact grouping depends on backend, graph shape, memory layout, and semantic barriers.

## Fusion Analyzer

Goal:

```text
identify which AOEM graph nodes can be fused safely
```

Conceptual API:

```text
analyze_fusion(graph) -> FusionPlan
```

Fusion candidates:

```text
invariant lookup + generator compute
generator compute + coeff apply
coeff apply + residual apply
residual apply + local state update
```

Fusion barriers:

```text
state transition ordering boundary
receipt emission boundary
signature verification boundary where required
memory aliasing hazard
backend synchronization requirement
debug / replay guard
```

Report fields:

```text
aoem_fusion_candidate_count
aoem_fusion_barrier_count
aoem_fusion_barrier_reason_sample
aoem_fusion_analyze_elapsed_ns
```

## Fusion Scheduler

Goal:

```text
turn FusionPlan into a FusedGraph
```

Conceptual API:

```text
schedule_fusion(plan) -> FusedGraph
```

Requirements:

```text
deterministic scheduling
stable node ordering where ledger semantics require it
backend-aware grouping
rollback to unfused graph
```

Report fields:

```text
aoem_kernel_count_before_fusion
aoem_kernel_count_after_fusion
aoem_kernel_reduction_bps
aoem_fused_group_count
aoem_fusion_schedule_elapsed_ns
```

## Fused Kernel Generator

Goal:

```text
generate AOEM-executable fused kernel descriptors
```

Conceptual API:

```text
generate_fused_kernels(fused_graph) -> KernelSet
```

v0 may produce descriptors, not runtime-generated machine code.

Kernel descriptor:

```text
kernel_id
backend
input_layout
output_layout
fused_node_ids
memory_plan_ref
semantic_barrier_flags
```

Report fields:

```text
aoem_fused_kernel_descriptor_count
aoem_fused_kernel_backend
aoem_fused_kernel_generation_elapsed_ns
```

## Memory Optimization Pass

Goal:

```text
reduce intermediate buffers and memory round trips
```

Conceptual API:

```text
optimize_memory_layout(fused_graph) -> MemoryPlan
```

Targets:

```text
reuse buffers
avoid materializing intermediate graph nodes
improve cache locality
align SIMD / GPU access
reduce host-device transfers
```

Report fields:

```text
aoem_memory_buffer_count_before
aoem_memory_buffer_count_after
aoem_memory_bytes_before
aoem_memory_bytes_after
aoem_memory_roundtrip_count_before
aoem_memory_roundtrip_count_after
aoem_memory_reduction_bps
```

## Execution Planner

Goal:

```text
produce final AOEM execution order and backend dispatch plan
```

Conceptual API:

```text
plan_execution(fused_kernels) -> AOEMExecutionPlan
```

Plan should include:

```text
kernel order
backend selection
memory plan
sync points
semantic barriers
fallback path
```

Report fields:

```text
aoem_execution_plan_kernel_count
aoem_execution_plan_sync_point_count
aoem_execution_plan_backend
aoem_execution_plan_elapsed_ns
```

## Validation Guard

Every fused execution plan must match the unfused baseline.

Required checks:

```text
fused result == unfused result
ledger state root matches
receipt output matches
deterministic replay matches
debug fallback path exists
```

Report fields:

```text
aoem_fusion_baseline_equivalence_checked
aoem_fusion_baseline_mismatch_count
aoem_fusion_ledger_root_match
aoem_fusion_receipt_mismatch_count
aoem_fusion_fallback_available
```

## Benchmark Fields

Measure:

```text
kernel_count reduction
kernel launch overhead
memory bandwidth usage
memory roundtrip count
execution latency
TPS gain
state root mismatch count
receipt mismatch count
```

Suggested report fields:

```text
aoem_kernel_launch_overhead_ns_before
aoem_kernel_launch_overhead_ns_after
aoem_execution_latency_ns_before
aoem_execution_latency_ns_after
aoem_execution_speedup_bps
aoem_memory_bandwidth_bytes_per_sec
```

## Relationship To Execution Graph Compiler

Execution Graph Compiler solves:

```text
IR -> optimized logical execution graph
```

Kernel Fusion Scheduler solves:

```text
optimized logical graph -> lower-cost physical AOEM execution plan
```

The compiler decides what should be computed.

The scheduler decides how AOEM should physically execute it.

## Non-Goals

Do not:

```text
create AOEM v2
generate non-deterministic execution paths
skip baseline equivalence checks
change ledger semantics
change receipt semantics
change transaction hash semantics
make NOVORUDP inspect or schedule kernels
introduce runtime codegen before descriptor-based fusion is signed
```

## First Success Criteria

```text
FusionPlan is generated from an optimized APFL execution graph.
Kernel count is reduced.
Memory plan is reported.
Fused execution matches unfused baseline.
Ledger and receipt equivalence hold.
Fallback to unfused execution is available.
```
