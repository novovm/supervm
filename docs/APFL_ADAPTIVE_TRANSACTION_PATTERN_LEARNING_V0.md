# APFL Adaptive Transaction Pattern Learning v0

Date: 2026-06-30

Status: `ADAPTIVE STRUCTURE LEARNING DESIGN / DO NOT IMPLEMENT DIRECTLY`

This document captures the APFL layer that learns repeated transaction structures over time and feeds better invariants, generators, batch shapes, and cache policies back into the execution system.

It is a roadmap document. It does not claim the feature is implemented.

## Entry Condition

This stage should not start until these stages are implemented and signed:

```text
APFL binary codec v0
zero-copy columnar wire view
single-batch invariant reuse
cross-batch invariant cache
execution graph fusion
AOEM semantic equivalence guards
```

Reason:

```text
adaptive learning must optimize an already deterministic and measurable execution system.
```

## Core Thesis

Manual APFL design can define:

```text
invariant
generator
coefficient layout
residual format
batch structure
cache rule
```

But long-running systems need to discover:

```text
which transaction patterns are hot
which invariants are reusable
which generators should be grouped
which coefficient patterns are low entropy
which batch shape maximizes reuse and SIMD efficiency
```

The target transformation is:

```text
developer-defined structure
  -> system-discovered structure
  -> feedback-driven APFL bank evolution
```

## Pipeline Position

Target pipeline:

```text
APFL IR input stream
  -> zero-copy wire decode
  -> cross-batch invariant cache
  -> execution graph fusion
  -> SIMD batch execution
  -> adaptive pattern learning engine
  -> AOEM commit / feedback
```

The learner observes the execution stream and proposes structure updates.

It must not make unsafe semantic changes inline.

## Pattern Extractor

Input:

```text
stream of APFL batches
execution metadata
cache hit/miss history
residual distributions
coefficient distributions
ledger-safe semantic labels
```

Output:

```text
PatternSet
```

Conceptual API:

```text
extract_patterns(stream) -> PatternSet
```

Report fields:

```text
apfl_pattern_observed_batch_count
apfl_pattern_observed_tx_count
apfl_pattern_signature_count
apfl_pattern_hot_signature_count
apfl_pattern_extraction_elapsed_ns
```

## Invariant Mining Engine

Goal:

```text
detect repeated invariant groups
detect stable cross-batch invariants
detect high-frequency invariants
propose InvariantBankDelta
```

Conceptual API:

```text
mine_invariants(patterns) -> InvariantBankDelta
```

Report fields:

```text
apfl_invariant_candidate_count
apfl_invariant_candidate_accepted_count
apfl_invariant_candidate_rejected_count
apfl_invariant_expected_reuse_ratio_bps
apfl_invariant_bank_delta_count
```

## Generator Clustering

Goal:

```text
group generators by output similarity
group generators by coefficient shape
group generators by residual pattern
identify generator reuse candidates
```

Conceptual API:

```text
cluster_generators(generators) -> GeneratorGroups
```

Report fields:

```text
apfl_generator_cluster_count
apfl_generator_cluster_member_count
apfl_generator_cluster_reuse_estimate_bps
apfl_generator_cluster_validation_error_count
```

## Coefficient Pattern Compression

Goal:

```text
detect repeating coefficient vectors
detect low-entropy coefficient sequences
detect shared coefficient bases
propose coefficient compression plans
```

Conceptual API:

```text
compress_coefficients(coeff_stream) -> CoeffBasis
```

Report fields:

```text
apfl_coeff_pattern_count
apfl_coeff_low_entropy_stream_count
apfl_coeff_basis_candidate_count
apfl_coeff_estimated_savings_bps
```

## Batch Shape Optimizer

Goal:

```text
learn optimal batch grouping
maximize invariant reuse
maximize generator reuse
improve SIMD alignment
reduce residual entropy
control payload size
```

Conceptual API:

```text
optimize_batch_shape(stream) -> BatchPlan
```

Report fields:

```text
apfl_batch_shape_candidate_count
apfl_batch_shape_selected
apfl_batch_shape_expected_payload_bytes_per_tx
apfl_batch_shape_expected_reuse_ratio_bps
apfl_batch_shape_expected_simd_alignment_score
```

## Feedback Loop

The learner may propose updates to:

```text
InvariantBank
GeneratorBank
cache policy
batch routing rules
codec selection rules
```

But applying a proposal requires validation.

Required states:

```text
proposed
validated
activated
rolled_back
rejected
```

Report fields:

```text
apfl_learning_proposal_count
apfl_learning_validated_count
apfl_learning_activated_count
apfl_learning_rollback_count
apfl_learning_rejected_count
```

## Safety and Determinism Rules

Adaptive learning must not break consensus or replay.

Hard rules:

```text
learning may propose new structure
learning may not silently change execution semantics
activation must be versioned
activation must be deterministic for all validators/executors
rollback must be possible
canonical equivalence must be proven before activation
```

Do not:

```text
let the learner mutate AOEM ledger semantics
let the learner change transaction hash semantics
let the learner create non-deterministic batch routing
let the learner depend on local-only timing for consensus behavior
let the learner affect NOVORUDP ACK/REPAIR
```

## Performance Goal

This stage is not a one-time speedup.

It targets long-running adaptation:

```text
batch gets more structured over time
invariant count per tx decreases
generator diversity decreases on hot paths
coefficient entropy decreases
cache hit ratio increases
payload bytes per tx decreases
execution graph node count decreases
```

Target metrics:

```text
apfl_adaptive_cache_hit_ratio_delta_bps
apfl_adaptive_bytes_per_tx_delta_bps
apfl_adaptive_graph_node_reduction_delta_bps
apfl_adaptive_tps_delta_bps
```

## First Success Criteria

The first version should only observe and propose.

Required:

```text
pattern signatures are extracted
candidate invariants are proposed
candidate generators are clustered
candidate batch shapes are reported
no runtime behavior changes automatically
no ledger semantic changes
```

Only after observation mode is signed should activation be allowed.

## Strategic Meaning

This stage moves NOVOVM from:

```text
executing optimized structures
```

to:

```text
learning better structures from live transaction streams
```

The long-term direction:

```text
APFL observes repeated computation
APFL proposes structure
AOEM executes validated structure
NOVORUDP transports compact structure bytes
```

This is the beginning of a self-optimizing structure system, but v0 must remain observation-first and deterministic.
