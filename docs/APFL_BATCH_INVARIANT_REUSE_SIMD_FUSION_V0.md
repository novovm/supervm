# APFL Batch Invariant Reuse and SIMD Fusion v0

Date: 2026-06-30

Status: `EXECUTION PIPELINE DESIGN / DO NOT IMPLEMENT DIRECTLY`

This document captures the APFL batch execution optimization layer after zero-copy columnar wire layout.

It is a design and task document. It does not authorize immediate implementation.

## Core Thesis

Zero-copy wire format reduces memory copy and layout cost.

The next bottleneck is repeated computation inside a batch:

```text
invariant lookup repeated per tx
generator evaluation repeated per tx
coefficient compute repeated per tx
residual apply repeated per tx
```

The target transformation is:

```text
per-transaction compute
  -> batch-level compute reuse
  -> SIMD / vectorized execution plan
```

This is the transition from:

```text
compressing transactions
```

to:

```text
optimizing the batch execution graph
```

## Pipeline Shape

Target APFL batch execution pipeline:

```text
1. Wire Decode
   -> zero-copy batch view

2. Columnar SIMD Layout
   -> invariant/generator/coeff/residual columns

3. Invariant Dedup Engine
   -> unique invariants + index map

4. Generator Fusion Engine
   -> grouped generator plan

5. Coefficient SIMD Compute
   -> vectorized coefficient application

6. Residual Merge Engine
   -> merged sparse residual plan

7. AOEM Execution Commit
   -> state transition / digest / receipt / ledger
```

## Invariant Dedup Engine

Input:

```text
invariant_id[] = [i1, i2, i3, i1, i2, i4, ...]
```

Output:

```text
unique_invariants = [i1, i2, i3, i4, ...]
index_map = [0, 1, 2, 0, 1, 3, ...]
```

Goal:

```text
lookup each unique invariant once
reuse invariant data across matching transactions
```

Conceptual API:

```text
dedup_invariants(batch_view) -> InvariantReusePlan
```

Report fields:

```text
apfl_invariant_total_count
apfl_invariant_unique_count
apfl_invariant_reuse_count
apfl_invariant_reuse_ratio_bps
apfl_invariant_dedup_elapsed_ns
```

## Generator Fusion Engine

Input:

```text
generator_id[] = [g1, g1, g2, g1, g3, ...]
```

Output:

```text
generator_groups:
  g1 -> tx indexes
  g2 -> tx indexes
  g3 -> tx indexes
```

Goal:

```text
prepare one generator execution plan per generator group
reuse generator outputs where legal
```

Conceptual API:

```text
fuse_generators(batch_view, invariant_plan) -> GeneratorFusionPlan
```

Report fields:

```text
apfl_generator_total_count
apfl_generator_unique_count
apfl_generator_reuse_count
apfl_generator_reuse_ratio_bps
apfl_generator_fusion_elapsed_ns
```

## SIMD Coefficient Compute

Input:

```text
coeff_matrix: batch x features
```

Goal:

```text
operate on coefficient columns or rows with vector lanes
avoid per-tx scalar loops when batch layout permits vector operations
```

v0 rule:

```text
start with a scalar reference path behind the same batch-plan API,
then add SIMD without changing the APFL wire format or AOEM semantics.
```

Conceptual API:

```text
execute_coeff_plan(plan, coeff_matrix) -> CoeffResultBlock
```

Report fields:

```text
apfl_coeff_count
apfl_coeff_compute_elapsed_ns
apfl_coeff_compute_per_tx_ns
apfl_coeff_simd_enabled
apfl_coeff_simd_lane_width
```

## Residual Merge Engine

Input:

```text
residual_sparse per tx
```

Goal:

```text
sort or group residual indexes
merge residual application plan
reduce repeated sparse update overhead
```

Conceptual API:

```text
merge_residuals(batch_view) -> ResidualMergePlan
```

Report fields:

```text
apfl_residual_entry_count
apfl_residual_unique_index_count
apfl_residual_merge_elapsed_ns
apfl_residual_merge_ratio_bps
```

## AOEM Execution Commit

AOEM must consume the optimized APFL execution plan without changing ledger semantics.

Allowed v0 path:

```text
APFL execution plan -> canonical reconstruction -> existing AOEM commit
```

Future path:

```text
APFL execution plan -> AOEM-native structured execution
```

Hard guard:

```text
state transition, tx hash, receipt, and ledger replay semantics must remain equivalent.
```

Report fields:

```text
apfl_aoem_plan_execute_elapsed_ns
apfl_aoem_plan_execute_per_tx_ns
canonical_reconstruction_count
canonical_reconstruction_error_count
canonical_tx_hash_match_count
canonical_tx_hash_mismatch_count
ledger_semantic_mismatch_count
```

## Expected Performance Impact

This stage is not mainly about bytes.

It targets compute redundancy:

```text
old path:
  128 x full compute path

new path:
  unique invariant lookup
  grouped generator execution
  vectorized coefficient compute
  merged residual application
```

Expected direction:

```text
20-40% compute reduction in regular batches
3-8x execution speedup in highly repetitive structured batches
```

These are targets, not signoff claims.

## Engineering Task Package

### Task 1: Invariant Dedup Engine

Target:

```text
dedup_invariants(batch_view) -> InvariantReusePlan
```

Requirements:

```text
deterministic output
stable ordering
no semantic change
reported reuse counts
```

### Task 2: Generator Fusion Engine

Target:

```text
fuse_generators(batch_view, invariant_plan) -> GeneratorFusionPlan
```

Requirements:

```text
group same generator_id
preserve transaction order for final commit
report reuse ratio
```

### Task 3: Coefficient Execution Plan

Target:

```text
execute_coeff_plan(plan, coeff_matrix)
```

Requirements:

```text
scalar reference result first
SIMD path may be added later
scalar and SIMD results must match exactly
```

### Task 4: Residual Merge Engine

Target:

```text
merge_residuals(batch_view) -> ResidualMergePlan
```

Requirements:

```text
deterministic sparse residual merge
stable replay
exact output equivalence
```

### Task 5: End-to-End Plan Benchmark

Measure:

```text
batch latency
tx/s
invariant reuse ratio
generator reuse ratio
coeff compute elapsed
residual merge elapsed
AOEM commit elapsed
canonical reconstruction mismatch count
ledger semantic mismatch count
```

## Relationship To Zero-Copy Wire Format

Zero-copy wire format solves:

```text
memory representation
copy cost
cache layout
decode object allocation
```

Batch invariant reuse solves:

```text
compute redundancy
generator repetition
coefficient execution cost
residual application cost
```

They should be developed in this order:

```text
1. APFL binary codec
2. zero-copy columnar wire view
3. invariant reuse engine
4. generator fusion
5. SIMD coefficient execution
6. AOEM structured execution
```

## Non-Goals

Do not:

```text
change NOVORUDP transport semantics
make transport aware of invariant or generator ids
skip canonical reconstruction guards
change ledger semantics to make performance numbers pass
introduce cross-batch cache before single-batch reuse is signed
claim SIMD speedup without scalar equivalence tests
```

## First Success Criteria

```text
single-batch invariant/generator reuse plan is deterministic
scalar plan output matches baseline canonical execution
reuse metrics are reported
no ledger semantic mismatch
```

Only after that should SIMD and cross-batch caches be introduced.
