# APFL Execution Graph Compiler v0

Date: 2026-06-30

Status: `EXECUTION GRAPH COMPILER DESIGN / DO NOT IMPLEMENT DIRECTLY`

This document defines the APFL layer that compiles APFL IR batches into optimized execution graphs for AOEM.

It is a design document. It does not authorize immediate code implementation.

## Boundary

This layer does not create a second execution kernel.

```text
APFL compiles execution graphs.
AOEM executes execution graphs.
```

Required flow:

```text
APFL IR
  -> Execution Graph Compiler
  -> Optimized Execution Graph
  -> AOEM Kernel Mapper
  -> AOEM Runtime Execution
```

## Core Thesis

APFL IR is not only a data structure.

APFL IR should become:

```text
compiled execution graph input
```

The key problem is:

```text
IR -> how to become the optimal execution graph
```

## Compiler Pipeline

Target pipeline:

```text
1. Graph Builder
   APFLTransactionIR batch -> ExecutionGraph

2. Node Fusion Engine
   merge equivalent invariant / generator / coeff nodes

3. Generator Reuse Optimizer
   execute repeated generators once when legal

4. Coefficient Layout Planner
   produce SIMD-friendly coefficient layout

5. Residual Scheduler
   schedule sparse residual application with cache locality

6. AOEM Kernel Mapper
   map graph nodes to AOEM kernels / backends

7. Graph Validation Guard
   prove semantic equivalence before execution
```

## Graph Builder

Input:

```text
APFLTransactionIR batch
```

Output:

```text
ExecutionGraph
```

Conceptual API:

```text
build_graph(ir_batch) -> ExecutionGraph
```

Report fields:

```text
apfl_graph_input_tx_count
apfl_graph_node_count
apfl_graph_edge_count
apfl_graph_build_elapsed_ns
```

## Node Fusion Engine

Fusion candidates:

```text
same invariant_id
same generator_id
same coeff signature
compatible residual schedule
```

Conceptual API:

```text
fuse_nodes(graph) -> OptimizedGraph
```

Report fields:

```text
apfl_graph_node_count_before_fusion
apfl_graph_node_count_after_fusion
apfl_graph_node_fusion_count
apfl_graph_node_reduction_bps
apfl_graph_fusion_elapsed_ns
```

## Generator Reuse Optimizer

Rule:

```text
if same generator can be reused legally:
  execute once
  reuse output
```

Conceptual API:

```text
optimize_generators(graph) -> GeneratorReusePlan
```

Report fields:

```text
apfl_graph_generator_node_count
apfl_graph_generator_unique_count
apfl_graph_generator_reuse_count
apfl_graph_generator_reuse_ratio_bps
```

## Coefficient Layout Planner

Goal:

```text
convert coefficient vectors into SIMD-friendly layout
```

Conceptual API:

```text
layout_coefficients(graph) -> CoeffLayoutPlan
```

Report fields:

```text
apfl_graph_coeff_count
apfl_graph_coeff_layout_kind
apfl_graph_coeff_simd_ready
apfl_graph_coeff_layout_elapsed_ns
```

## Residual Scheduler

Goal:

```text
schedule sparse residual application to reduce cache misses
```

Conceptual API:

```text
schedule_residuals(graph) -> ResidualSchedule
```

Report fields:

```text
apfl_graph_residual_entry_count
apfl_graph_residual_schedule_kind
apfl_graph_residual_schedule_elapsed_ns
```

## AOEM Kernel Mapper

Maps graph nodes to AOEM kernels:

```text
invariant node -> lookup kernel
generator node -> compute kernel
coeff node -> SIMD coefficient kernel
residual node -> sparse residual kernel
commit node -> state transition / ledger kernel
```

Conceptual API:

```text
map_to_aoem_kernels(graph) -> AOEMKernelPlan
```

Report fields:

```text
apfl_aoem_kernel_plan_node_count
apfl_aoem_kernel_plan_backend
apfl_aoem_kernel_cpu_count
apfl_aoem_kernel_gpu_count
apfl_aoem_kernel_vulkan_count
apfl_aoem_kernel_mapping_elapsed_ns
```

## Validation Guard

Before any graph is considered executable, it must pass:

```text
deterministic graph id
schema version check
canonical reconstruction guard where applicable
signature verification guard
ledger state root equivalence
receipt equivalence
baseline scalar execution equivalence
```

Report fields:

```text
apfl_graph_validation_passed
apfl_graph_validation_error_count
apfl_graph_baseline_equivalence_checked
apfl_graph_baseline_mismatch_count
apfl_graph_ledger_root_match
apfl_graph_receipt_mismatch_count
```

## Benchmark Fields

Benchmark should report:

```text
apfl_graph_build_latency_ns
apfl_graph_fusion_latency_ns
apfl_graph_kernel_mapping_latency_ns
apfl_graph_execution_latency_ns
apfl_graph_total_latency_ns
apfl_graph_tps
apfl_graph_node_reduction_bps
apfl_graph_cache_miss_reduction_bps
```

## Relationship To Earlier Layers

Earlier layers solve:

```text
wire optimization
batch data layout
invariant reuse
cross-batch cache
pattern learning
structure evolution proposals
```

Execution Graph Compiler solves:

```text
how the structure becomes an optimized AOEM execution path
```

## Non-Goals

Do not:

```text
create AOEM v2
execute APFL outside AOEM
change ledger semantics
change receipt semantics
change tx hash semantics
skip scalar baseline equivalence tests
make NOVORUDP inspect graph nodes
```

## First Success Criteria

```text
APFL IR batch builds an ExecutionGraph.
Node fusion reduces graph nodes without semantic mismatch.
AOEM kernel mapping is explicit and reported.
Graph validation passes against scalar baseline.
Ledger and receipt equivalence hold.
```
