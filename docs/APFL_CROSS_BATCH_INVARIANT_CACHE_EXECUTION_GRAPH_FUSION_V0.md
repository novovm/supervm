# APFL Cross-Batch Invariant Cache and Execution Graph Fusion v0

Date: 2026-06-30

Status: `SYSTEM-LEVEL EXECUTION REUSE DESIGN / DO NOT IMPLEMENT DIRECTLY`

This document captures the next APFL execution optimization layer after single-batch invariant reuse and SIMD fusion.

It is a roadmap document. It does not claim the feature is implemented.

## Current Assumption

The preceding layers are design targets:

```text
APFL binary codec
zero-copy columnar wire view
single-batch invariant dedup
generator fusion
coefficient SIMD execution
residual merge
```

This document describes the next layer after those are implemented and signed.

## Core Thesis

Single-batch optimization removes repeated work inside one batch.

The next bottleneck is repeated work across batches:

```text
Batch A: invariant i1, i2, i3
Batch B: invariant i1, i2, i4
Batch C: invariant i1, i3, i5
```

Without a cross-batch cache, the system repeatedly performs:

```text
invariant lookup
generator evaluation
coefficient compute
execution graph construction
```

The target transformation is:

```text
batch-local execution
  -> cross-batch computation reuse
  -> fused execution graph
```

## Pipeline vNext

Target pipeline:

```text
1. Wire Decode
   -> zero-copy batch view

2. Columnar SIMD Layout
   -> APFL columns

3. Cross-Batch Invariant Cache Lookup
   -> reuse computed invariant results

4. Execution Graph Fusion Engine
   -> merge repeated graph nodes across batches

5. Generator Memoization
   -> reuse generator output by generator/coeff signature

6. Batch SIMD Coefficient Compute
   -> compute remaining non-cached coefficient paths

7. Residual Merge Engine
   -> apply per-batch residuals

8. AOEM Commit
   -> deterministic state transition
```

## Global Invariant Cache

Conceptual structure:

```text
InvariantCache:
  invariant_id -> computed_value
```

Requirements:

```text
deterministic lookup semantics
bounded memory policy
explicit invalidation/versioning
no dependence on transport packet order
no mutation of ledger semantics
```

Conceptual API:

```text
lookup_or_compute_invariant(invariant_id) -> ComputedInvariant
```

Report fields:

```text
apfl_cross_batch_invariant_cache_enabled
apfl_invariant_cache_lookup_count
apfl_invariant_cache_hit_count
apfl_invariant_cache_miss_count
apfl_invariant_cache_hit_ratio_bps
apfl_invariant_cache_insert_count
apfl_invariant_cache_evict_count
apfl_invariant_cache_bytes
```

## Generator Memoization

Conceptual key:

```text
(generator_id, coeff_signature, invariant_version) -> generator_output
```

Requirements:

```text
stable cache key
explicit versioning
deterministic output
cache hit must equal recompute result
```

Conceptual API:

```text
memoized_generate(generator_id, coeff_signature, invariant_version) -> GeneratorOutput
```

Report fields:

```text
apfl_generator_memo_lookup_count
apfl_generator_memo_hit_count
apfl_generator_memo_miss_count
apfl_generator_memo_hit_ratio_bps
apfl_generator_memo_bytes
apfl_generator_recompute_equivalence_error_count
```

## Execution Graph Fusion

Instead of executing:

```text
Batch A graph
Batch B graph
Batch C graph
```

Build:

```text
FusedExecutionGraph:
  shared invariant nodes
  shared generator nodes
  shared coefficient nodes
  per-batch residual nodes
  per-transaction commit nodes
```

Requirements:

```text
preserve transaction ordering where ledger semantics require it
preserve deterministic replay
preserve receipt generation
make fusion observable and reversible in debug mode
```

Conceptual API:

```text
fuse_execution_graph(batches) -> FusedExecutionGraph
execute_fused_graph(graph) -> StateUpdates
```

Report fields:

```text
apfl_fused_graph_batch_count
apfl_fused_graph_input_tx_count
apfl_fused_graph_node_count_before
apfl_fused_graph_node_count_after
apfl_fused_graph_node_reduction_bps
apfl_fused_graph_build_elapsed_ns
apfl_fused_graph_execute_elapsed_ns
apfl_fused_graph_semantic_mismatch_count
```

## Performance Goal

This stage is not mainly about byte reduction.

It targets:

```text
repeated invariant compute down
generator evaluation down
cache hit ratio up
graph node count down
batch fusion depth up
```

Expected direction:

```text
2x-10x batch throughput potential in highly repetitive workloads
```

This is not a signoff claim. It depends on workload regularity and cache hit rate.

## Safety Rules

Do not:

```text
change NOVORUDP transport semantics
make cache results affect transport ACK/REPAIR
skip canonical reconstruction or semantic equivalence checks
change ledger ordering to make fusion easier
share cached values across incompatible invariant versions
introduce cross-batch cache before single-batch reuse is signed
```

## Required Guards

Every fused path must have a baseline equivalence check:

```text
fused result == unfused scalar result
canonical tx hash matches where applicable
signature verification still passes
ledger state root matches
receipt count and content match
```

Report fields:

```text
apfl_fusion_baseline_equivalence_checked
apfl_fusion_baseline_mismatch_count
apfl_fusion_ledger_root_match
apfl_fusion_receipt_mismatch_count
```

## First Success Criteria

```text
cross-batch invariant cache hit/miss metrics are reported
generator memoization has deterministic keys
fused graph output matches unfused scalar baseline
ledger semantic mismatch count = 0
receipt mismatch count = 0
```

Only after this layer is signed should adaptive transaction pattern learning be introduced.
