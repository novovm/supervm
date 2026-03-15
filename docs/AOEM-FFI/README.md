# AOEM (Abstract Orchestrated Execution Model)

**The first algebraic parallel execution engine that defines correctness without requiring execution order**

_Turning concurrency from a scheduling problem into a semantic primitive · Quantum computing isomorphism · Enabling future hardware (GPU/Quantum/ZKVM) for trusted execution_

**[Documentation Index (EN)](docs/README.md)**  
**[Chinese Docs](docs/README.zh-CN.md)**

---

## What is AOEM

AOEM (Abstract Orchestrated Execution Model) is a **general-purpose parallel execution engine driven by algebraic semantics**. Rather than being designed for a specific domain (such as blockchain), it fundamentally redefines the mathematical foundation of "concurrent correctness," enabling CPU, GPU, ZKVM, and even future quantum computing units to work collaboratively **within the same semantic framework**.

## Product Boundary Contract (Host Coverage)

AOEM is a general-purpose execution kernel and binary capability layer, not a chain-only node product.

- AOEM owns: execution semantics, backend routing/fallback, capability contract, stable FFI.
- Hosts own: domain protocol, networking/consensus, governance, and product logic.
- Typical hosts: blockchain, AI, OS/runtime, robotics, and distributed systems.
- Rule: hosts integrate AOEM; they do not re-implement AOEM execution semantics.

### Core Insights

#### 1. Algebraic Semantics: From "Sequential Traces" to "Equational Theory"

Traditional systems define correctness through **"a single execution order"** (timestamps, locks, MVCC, etc.), which fundamentally leads to:
- GPU's native parallelism cannot be safely integrated into systems
- Distributed concurrency requires global coordination (clock synchronization/versioning)
- Verification must track complete execution traces (uncontrolled proof size)

**AOEM adopts algebraic semantics**:
- Correctness is defined as **"invariance of observables under equational theory"**
- Execution terms can be **rewritten via equations** while preserving semantics
- The scheduler is not a "temporal coordinator" but a **"semantics-preserving rewriting engine"**

```
Traditional: t₁ → t₂ → t₃  (must execute in this temporal order)
AOEM:        t₁ ‖ t₂ · t₃   (if independent(t₁, t₂), commutable/parallel)
```

**This is not "optimization" but a paradigm shift at the semantic level**:

> Concurrency is no longer a "scheduling problem," but **a semantic primitive of the program itself**.

#### 2. Mathematical Isomorphism with Quantum Computing (Core Breakthrough)

AOEM's algebraic semantics is **highly isomorphic to quantum computing at the formal structure level**:

| Concept            | AOEM (Classical Algebraic Superposition)     | Quantum Computing (Physical Superposition)        |
|--------------------|---------------------------------------------|--------------------------------------------------|
| **State Space**    | Abstract semantic space (algebraic objects) | Hilbert space (complex vectors)                  |
| **Superposition**  | Multi-path semantic superposition `a·s₁ ⊕ b·s₂` | Quantum state superposition `α\|0⟩ + β\|1⟩`    |
| **Operators**      | Semantic transformation operators (rewrite/effect) | Quantum gates (unitary/quantum channels)       |
| **Observation**    | commit/finalize/resolve (convergence)       | measurement (projection/sampling/collapse)       |
| **Commutativity**  | independent → reorderable                   | Commutable quantum gates → parallel gate layers  |
| **Uncertainty**    | Multi-path delayed decision (rollbackable)  | Pre-measurement superposition (no-cloning)       |

**Unified Execution Loop** (applicable to both AOEM and quantum computing):
```
σ₀ = init()
for op in program:
    σ = apply(op, σ)        // Classical: state transformation | Quantum: unitary evolution
    if needs_observe(op):
        (c, σ) = observe(σ)  // Classical: commit | Quantum: measurement
return outputs
```

**Key Insight**:

> **AOEM's algebraic semantics represents concurrent execution uncertainty as "composable semantic superposition";  
> Quantum computing represents physical states as "composable linear superposition".  
> Both share the structure of "operator application + confluent observation," enabling unified scheduling and pluggable backends within the same execution framework.**

This makes AOEM:

> **"The Archetype of quantum computing software layer" — In the classical world, the software system most resembling quantum computing.**

**Same IR, Pluggable Backends** (AOEM × Quantum Unified Model):

The same IR/scheduling framework can run simultaneously:
- **Classical Backend**: CPU/GPU classical concurrency
- **Quantum Simulator**: State vector/tensor network simulation
- **Quantum QPU**: Integration with cloud quantum hardware (IBM/AWS/Azure)

Only need to switch:
- **State space definition**: Algebraic objects → Hilbert space
- **Operator constraints**: Algebraic rewrite → Unitary operators
- **Observation semantics**: commit → measurement

**Isomorphic Points**:
- ✅ Linear composition: Semantic superposition ≈ Quantum superposition
- ✅ Delayed decision: Commit delay ≈ Measurement delay
- ✅ Operator composition: Effect composition ≈ Quantum gate composition
- ✅ Final collapse: validate → determined path ≈ measurement → classical result

**Key Differences**:
- ❌ AOEM has **no physical uncertainty** (copyable/rollbackable/enumerable)
- ❌ AOEM has **no no-cloning constraint**
- ❌ AOEM has **no quantum entanglement (in the physical sense)**
- ✅ AOEM is **computable superposition** (deterministic convergence)

#### 3. Three-Layer Execution Architecture: Unified Semantics, Heterogeneous Backends

```
+-------------------------------------------------------------------+
|                     Algebraic Semantics Layer                     |
| Execution Terms:  . || + Tx                                       |
| Equational Theory: Commutativity | Associativity |                |
|                  Concurrency Lifting | Transaction Elimination    |
| Normal Form: (R1||...||Rn) . (W1...Wm) . Emit*                    |
+-------------------------------------------------------------------+
                         |
                         v
+------------------+------------------+------------------+----------+
| CPU Backend      | GPU Backend      | ZKVM Backend     | Quantum  |
| (Semantic Anchor)| (Batch Parallel) | (Verifiable Proof| Backend  |
| Arbiter/Rollback | Execution)       | Normal Form Proof| (Future) |
|                  | SPIR-V           |                  |          |
+------------------+------------------+------------------+----------+
```

- **CPU**: Provides reference implementation, arbitration mechanism, unified interface (strict/lenient dual modes)
- **GPU**: Based on SPIR-V compute IR (not CUDA, cross-vendor), first implementation of "GPU as system-level trusted execution unit"
- **ZKVM**: Proves **Normal Form** rather than execution traces, enabling proof aggregation
- **Quantum**: Same IR extension, classical-quantum hybrid computing (future work)

---

## Algebraic Execution Semantics (Theoretical Foundation)

### Formal Definition

**Execution Term Syntax**:
```
t ::= atom              // Read(k), Write(k,v), Transfer(a,b,x), Emit(e), ...
    | t · t             // Sequential composition
    | t ‖ t             // Parallel composition
    | t ⊕ t             // Choice/branching
    | Tx(t)             // Transactional scope
```

**Core Equations**:
```
Sequential Associativity:  (t₁ · t₂) · t₃ = t₁ · (t₂ · t₃)
Sequential Identity:       Nop · t = t = t · Nop

Parallel Commutativity:    t₁ ‖ t₂ = t₂ ‖ t₁              if independent(t₁, t₂)
Parallel Associativity:    (t₁ ‖ t₂) ‖ t₃ = t₁ ‖ (t₂ ‖ t₃)

Concurrency Lifting:       (t₁ · t₂) ‖ t₃ = (t₁ ‖ t₃) · t₂    if independent(t₂, t₃)

Transaction Elimination:   Tx(t · Fail · u) = Nop
Failure Absorption:        Fail · t = Fail = t · Fail
```

**Independence Predicate**:
```
independent(t₁, t₂) ⇔ RW(t₁) ∩ RW(t₂) = ∅
```
where `RW(t) = (ReadSet(t), WriteSet(t))`

**Normalization Theorem**:
> Any execution term `t` can be rewritten via equations to normal form `NF(t)`:  
> ```
> NF(t) = Tx( (R₁ ‖ R₂ ‖ ... ‖ Rₙ) · (W₁ · W₂ · ... · Wₘ) · Emit* )
> ```
> satisfying: `t ≡ NF(t)` (semantic equivalence)

**This is the mathematical foundation for GPU batch execution, ZK proof aggregation, and quantum circuit compilation.**

### Semantic Equivalence and Scheduler Correctness

**Definition (Semantic Equivalence)**:
```
t₁ ≡ t₂  ⇔  ∀M, O(⟦t₁⟧ₘ) = O(⟦t₂⟧ₘ)
```
where:
- `M` is a model satisfying the equational theory
- `O` is the observation function (state root, balance, event stream, etc.)

**Scheduler Correctness Theorem**:
> If the scheduler only uses rewriting rules from the equational theory,  
> then any execution plan it generates is semantically equivalent to the original program.

This means:

> **AOEM's scheduling layer is a sound rewriting system (semantics-preserving rewriting system)**

### Why This Matters (Paradigm Comparison)

| Dimension              | Traditional Systems                          | AOEM                                      |
|------------------------|---------------------------------------------|------------------------------------------|
| **Concurrency Definition** | Scheduling problem (OCC/locks/timestamps/MVCC) | Semantic primitive (algebraic composition `‖`) |
| **GPU Role**          | Peripheral accelerator (untrusted)          | First-class execution unit (trusted backend) |
| **Correctness Anchor** | Global sequential consistency (total order) | Observables invariant under equational theory |
| **ZK Proof**          | Prove execution trace (trace proof)         | Prove normal form (aggregatable)          |
| **Composability**     | Global state machine (state explosion)      | Modular algebraic effects (effect composition) |
| **Scheduler**         | Global coordinator (requires clock/version/lock) | Rewriting engine (local equational transformations) |

**AOEM's true barrier is not implementation complexity, but cognitive threshold**:

> Daring to abandon "unique execution order" as the sole anchor of correctness.

---

## GPU Execution Engine (AOEM-GPU)

### Why GPU Could Not Previously Be a System-Level Execution Unit

Traditional systems require **global execution order** (timestamps/locks/MVCC) as the correctness anchor, while GPU inherently doesn't obey order:
- ❌ CUDA execution is **non-deterministic** (warp scheduling uncontrollable)
- ❌ Results are **unprovable** (cannot serve as consensus/legal-grade input)
- ❌ Runtime is **not arbitrable** (no "ground truth" source)
- ❌ Cross-vendor has **no unified semantics** (CUDA ecosystem lock-in)

**AOEM's Breakthrough**:

> **Our semantics no longer requires order, only equivalence.**  
> GPU merely "selects an execution representative from the equivalence class."

This is the moment GPU is formally accepted by semantics.

### Capability Layers

#### 1) AFP (Atomic Function Primitives) Layer
**Positioning**: General-purpose system-level GPU parallel primitives, providing execution skeleton

**Core Primitives**:
- Scan / Prefix-Sum
- Reduce / Partition
- Scatter / Gather
- Histogram / Reorder

**Not "algorithm library," but**:
> **"The minimal execution skeleton for GPU to be safely, stably, and predictably used in systems"**

**Key Features**:
- Based on **SPIR-V compute IR** (not CUDA, cross-vendor)
- **Strict CPU-GPU consistency verification** (bit-exact)
- **Arbitrable** (CPU as ground truth)
- **Reproducible** (deterministic execution)

**Fundamental Problems Solved**:
- GPU native parallelism ≠ System-level composable execution
- CUDA primitives are unverifiable and non-arbitrable
- Multiple GPU vendors lack unified execution semantics

> **AFP is AOEM's foundation. Without AFP, AOEM would only be an "algorithm collection," not an "execution middleware."**

#### 2) MSM (Multi-Scalar Multiplication) Layer
**Positioning**: First heavyweight scenario "structurally uncoverable by CUDA ecosystem"

**Capability Definition**:
- 10⁵–10⁶ scale elliptic curve multi-scalar multiplication
- **Financial-grade determinism requirements** (zero-tolerance for non-determinism)
- Directly enters **ZK/consensus/state proof pipeline**

**Why MSM is a Key Watershed**:

CUDA's problem is not performance, but:
- ❌ Execution results are unprovable
- ❌ Runtime is non-arbitrable
- ❌ Cannot serve as consensus or legal-grade input

AOEM MSM provides:
- ✅ CPU/GPU strict consistency (automatically verifiable)
- ✅ Auditable, reproducible execution path
- ✅ Can enter ZK/state proof system

**Engineering Evidence (Frozen Paper-Level Conclusions)**:
- ✅ **Real bucket-list distribution replay mechanism** (SPIR-V pipeline export + dataset replay)
- ✅ **Auto-tuning strategy robustness verification**: overall_hit_rate = 100% (multi-seed/multi-round)
- ✅ **Thermal/frequency drift quantification**: Proves many "performance regressions" stem from measurement noise, not code changes

**Three Frozen Paper Skeletons** (Project Canon Level):
1. **GPU MSM Auto-Tuning** - Data-driven strategy based on realistic distributions
2. **AOEM-GPU Execution Model** - System architecture of GPU as general-purpose verifiable execution unit
3. **GPU Benchmark Robustness Methodology** - Reproducible benchmarking under thermal/frequency drift

> **MSM is AOEM's first heavyweight capability "structurally uncoverable by CUDA ecosystem," proving AOEM is not a theoretical middleware but an irreplaceable real-world execution engine.**

#### 3) CPU Backend
**Positioning**: Unified execution semantics anchor

**Responsibilities**:
1. **Execution semantic reference implementation** (semantic reference)
2. **Arbitration and verification baseline for GPU results** (arbiter / ground truth)
3. **System-level unified execution interface** (adapter pattern)

CPU doesn't pursue extreme performance but is **the final arbiter of semantic correctness**.

---

## Verification & Proof (ZKVM Integration)

### Normal Form Proof (vs Trace Proof)

Traditional ZKVM:
```
Proof Target:     Complete execution trace (100k transactions × N state transitions)
Proof Complexity: O(interleaving count × trace length)
Aggregation:      Irregular structure, hard to batch
```

AOEM + ZKVM:
```
Proof Target:     Normal form NF(t) = (R‖...‖R) · (W·...·W) · Emit*
Proof Complexity: O(normal form structure) << O(arbitrary interleaving)
Aggregation:      Same-class normal forms can be batch-proved
```

**Why Normal Form Proof is More Efficient**:
1. **Regular structure**: All executions rewritten to same form (Read || Write · Emit)
2. **Reusable**: Same normal form structure appears repeatedly → Proof circuits reusable
3. **Aggregatable**: No need to prove "why ordered this way," only prove "normal form executes correctly"

**This is why Phase 13–15 could progressively decompose stages, pinpoint bottlenecks, implement bucket-list, implement passthrough — the mathematical foundation is correct.**

---

## Quantum-Classical Unified Execution Model (Extension Direction)

### Unified IR (Minimal Viable Version)

```
QInit(q)                    // Quantum: prepare |0⟩  | Classical: initialize register
QGate(g, targets, ctrls)    // Quantum: unitary gate | Classical: parallel operator
QMeasure(q -> C[i])         // Quantum: measurement  | Classical: observe
COp(op, args)               // Classical operation   | Quantum: classical logic post feed-forward
Branch(cond, then, else)    // Classical branch      | Quantum: branch based on measurement
Barrier(scope)              // Scheduling barrier    | Equivalent to AOEM phase boundary
Observe(tag)                // Unified observation   | Classical=commit, Quantum=measurement bundle
```

### Extension Path

**Step 1**: Add `aoem-backend-quantum-sim` crate (similar to GPU backend)
- 2–8 qubit state vector simulation
- Basic quantum gates: H, X, Z, CNOT, Measure

**Step 2**: Add feature gate in Adapter layer
- `feature = "quantum"`
- `AOEM_BACKEND=quantum_sim`

**Step 3**: Reuse existing Scheduler/Router
- Replace "key-space/conflict domain" with "qubit-space"
- Start with mutex scheduling, then add commutative optimization

**Step 4**: Same IR dual-backend demo
- ClassicBackend: Classical probabilistic simulation (control group)
- QuantumSimBackend: Quantum state computation (experimental group)
- Compare measurement histograms

---

## Academic Lineage Positioning

AOEM is not "invented from scratch" but an **engineering synthesis** of existing theories:

| Academic Field              | AOEM Adoption & Innovation                                    |
|----------------------------|--------------------------------------------------------------|
| **Concurrency Semantics**  | Adopts **partial order semantics**, making `‖` first-class citizen instead of interleaving |
| **Rewriting Systems**      | Scheduler is **semantics-preserving rewriting engine**, normal forms directly map to hardware/proof-friendly forms |
| **Algebraic Effects**      | Modularizes side effects (State/Exception/Writer/Resource), enabling **pluggable backends** |
| **Linearizability/Serializability** | **Selective linearization**: Only introduces order at conflicts, independent terms gain parallelism via equational exchange |
| **Proof Systems**          | Proves **normal forms** instead of arbitrary interleavings, reducing circuit complexity, improving aggregation regularity |
| **Quantum Computing**      | Algebraic superposition **mathematically isomorphic** to quantum superposition, AOEM can serve as classical archetype of quantum software layer |

**Precise Academic Positioning**:

> AOEM adopts a partial-order view of execution,  
> elevating concurrency from "implementation detail" to "semantic primitive,"  
> constructing a semantics-preserving rewriting system to support heterogeneous backends (CPU/GPU/ZKVM/Quantum).

**Rigorous Statement for Papers/Whitepapers**:

> **AOEM adopts a partial-order view of execution, turning concurrency from an implementation artifact into a semantic primitive.** The scheduler is a **semantics-preserving rewriting engine** targeting hardware- and proof-friendly normal forms. AOEM's semantics is compatible with **algebraic effects**: the same term admits multiple backends via structure-preserving interpretations.

---

## Application Scenarios

AOEM is a **general-purpose execution engine** applicable to any scenario requiring **concurrency, verifiability, and heterogeneous execution**:

### 1. Blockchain/Web3
- ✅ High-performance transaction execution (OCCC/OCC/MVCC tri-path adaptation)
- ✅ GPU-accelerated state proofs (MSM/Merkle)
- ✅ ZK-Rollup batch verification aggregation (normal form proofs)
- ✅ Auditable cross-chain bridge execution
- ✅ Third-generation distributed internet execution kernel

### 2. Distributed Database/Storage
- ✅ Algebraic concurrency control (beyond OCC/MVCC)
- ✅ Cross-node semantic consistency (no global clock required)
- ✅ Auditable execution path (compliance/finance)

### 3. Scientific Computing/HPC
- ✅ Unified scheduling for heterogeneous hardware (CPU/GPU/TPU/FPGA)
- ✅ Reproducible computation results (bit-exact across backends)
- ✅ Verifiable intermediate execution states (checkpointing + proof)

### 4. Quantum-Classical Hybrid Computing (Future Direction)
- ✅ Same IR supports both classical and quantum backends
- ✅ Quantum simulator for rapid validation (local debugging)
- ✅ Quantum hardware integration (IBM Qiskit/AWS Braket/Azure Quantum)
- ✅ Classical-quantum feed-forward coordination

### 5. AI/Machine Learning Inference
- ✅ Verifiable model execution (trustworthy AI)
- ✅ GPU inference acceleration + CPU arbitration
- ✅ Trusted aggregation in federated learning

---

## Project Structure

```
AOEM/
├── crates/
│   ├── core/              # AOEM core: algebraic semantics, execution model
│   ├── adapter/           # Backend adapters (CPU/GPU/ZKVM)
│   ├── backend/           # CPU reference implementation
│   ├── storage-backend/   # Storage backend interface
│   ├── runtime/           # Execution runtime
│   ├── ffi/               # Foreign function interface
│   ├── optional/          # Optional components
│   │   ├── zkvm-executor/ # ZKVM integration
│   └── tests/             # Integration tests
├── docs/
│   └── whitepaper/
│       ├── capability-layer-diagram.md
│       ├── project-canon-aoem-gpu-research-skeletons.md
│       └── algebraic-semantics/
│           ├── semantics.md
│           ├── 01-algebraic-vs-operational-semantics.md
│           ├── 02-algebraic-signature-and-key-equations.md
│           ├── 03-semantics-vs-occ-mvcc.md
│           ├── 04-aoem-execution-gpu-zk-3d-mapping.md
│           ├── 05-why-gpus-failed-before.md
│           ├── 06-formal-definition-paper-level.md
│           └── 07-academic-lineage-positioning.md
├── docs-CN/              # Localized Chinese docs and PDFs
│   └── README.zh-CN.md
├── examples/              # Example code
├── scripts/               # Build/test scripts
├── vendor/                # Dependencies (offline build)
└── Cargo.toml
```

---

## Quick Start

### Prerequisites
- Rust 1.93+ (nightly recommended)
- C++17 compiler (MSVC 14.44, Visual Studio 2022)
- CMake 3.31+
- **RocksDB dependencies** (vendored)

### Build

```powershell
# Standard build
cargo build --release

# Enable GPU backend (requires Vulkan)
cargo build --release --features gpu

# Enable ZKVM integration
cargo build --release --features zkvm

# Full build (all backends)
cargo build --release --all-features
```

### Run Tests

```powershell
# Unit tests
cargo test

# Integration tests
cargo test --test '*' --release

# Benchmarks (requires nightly)
cargo +nightly bench
```

### Development Mode

```powershell
# Use cargo-aoem scripts (recommended)
.\scripts\cargo-aoem.ps1 build
.\scripts\cargo-aoem.ps1 test
.\scripts\cargo-aoem.ps1 bench

# Validate RocksDB integration
.\scripts\Validate-RocksDB-Integration.ps1
```

---

## Technical Documentation

### Core Concepts
- [Algebraic Semantics Specification](docs/whitepaper/algebraic-semantics/semantics.md) - Formal definition and equational theory
- [Capability Layer Diagram](docs/whitepaper/capability-layer-diagram.md) - From execution skeleton to irreplaceable operators
- [AOEM x GPU Technical Whitepaper (PDF)](docs/whitepaper/algebraic-semantics/AOEM×GPU(Technical-Whitepaper).pdf)

### Academic Depth
- [Difference Between Algebraic and Operational Semantics](docs/whitepaper/algebraic-semantics/01-algebraic-vs-operational-semantics.md)
- [AOEM's Position in Academic Lineage](docs/whitepaper/algebraic-semantics/07-academic-lineage-positioning.md)
- [Why GPUs Failed Before](docs/whitepaper/algebraic-semantics/05-why-gpus-failed-before.md)

### Engineering Practice
- [Project Canon - Frozen Paper Skeletons](docs/whitepaper/project-canon-aoem-gpu-research-skeletons.md)
- [AOEM x Execution Layer x GPU x ZK Three-Dimensional Mapping](docs/whitepaper/algebraic-semantics/04-aoem-execution-gpu-zk-3d-mapping.md)

### Performance / FFI Reports
- [AOEM FFI BETA09 TPS Seal (EN)](docs/perf/AOEM-FFI-BETA09-TPS-SEAL-2026-03-09.md)
- [AOEM FFI Fullmax Capability Matrix (EN)](docs/perf/AOEM-FFI-FULLMAX-CAPABILITY-MATRIX-2026-03-12.md)
- [AOEM FFI Host Call Parameters (EN)](docs/perf/AOEM-FFI-HOST-CALL-PARAMS-V1-2026-03-10.md)
- [AOEM GPUization Gap & Priority (EN)](docs/perf/AOEM-GPUIZATION-GAP-PRIORITY-2026-03-10.md)
- Chinese originals:
  [BETA09 TPS Seal (CN)](docs-CN/perf/AOEM-FFI-BETA09-TPS-SEAL-2026-03-09.md),
  [Fullmax Capability Matrix (CN)](docs-CN/perf/AOEM-FFI-FULLMAX-CAPABILITY-MATRIX-2026-03-12.md),
  [Host Call Parameters (CN)](docs-CN/perf/AOEM-FFI-HOST-CALL-PARAMS-V1-2026-03-10.md),
  [GPUization Gap & Priority (CN)](docs-CN/perf/AOEM-GPUIZATION-GAP-PRIORITY-2026-03-10.md)

---

## Key Publications (Submitted/In Preparation)

### Paper 1: GPU MSM Auto-Tuning (Submittable Independently)
**Title**: A Data-Driven Auto-Tuning Strategy for GPU MSM under Realistic Bucket Distributions

**Core Contributions**:
- Construct GPU replay mechanism for realistic bucket-list distributions
- Discover and quantify catastrophic misjudgments of auto strategies under realistic distributions
- Propose conservative auto-local-size strategy
- Multi-seed/multi-round robust validation: overall_hit_rate = 100%

### Paper 2: AOEM-GPU Execution Model (Strategic Main-Line Paper)
**Title**: AOEM-GPU: A General-Purpose Verifiable Execution Model for GPU-Based Distributed Systems

**Core Contributions**:
- Propose AOEM-GPU execution model (GPU as general-purpose execution engine)
- Use SPIR-V passthrough as execution IR (not CUDA)
- Correctness-first GPU execution path
- Distill "structural knife" methodology for GPU optimization

### Paper 3: GPU Benchmark Robustness Methodology
**Title**: Robust Benchmarking of GPU Kernels under Thermal and Frequency Drift

**Core Contributions**:
- Systematically quantify GPU thermal/frequency drift impact on microbenchmarks (5–40% fluctuation)
- Propose robust benchmarking method: A/B alternation + median-of-medians
- Prove many performance regressions stem from measurement noise, not code changes

---

## Contributing

AOEM is currently in research and development phase and does not accept external contributions. Detailed contribution guidelines will be provided after the project is open-sourced.

---

## License

This project uses a dual-license model:
- **Commercial License** (LICENSE.COMMERCIAL) - Commercial use requires authorization
- **Open Research License** (LICENSE.RESEARCH) - Academic research and non-commercial use

Third-party dependency licenses are detailed in [THIRD_PARTY_NOTICES.txt](THIRD_PARTY_NOTICES.txt)

---

## Contact

- **Technical Consultation**: leadbrand@me.com
- **Business Cooperation**: leadbrand@me.com
- **Academic Exchange**: leadbrand@me.com

---

## Acknowledgments

AOEM's theoretical foundation builds upon research from multiple fields:
- Concurrency semantics theory (Pomset semantics, Event structures)
- Algebraic effects and Handler theory
- Formal verification (Term rewriting systems, Confluence)
- Quantum computing theory (Linear superposition, Quantum gates)

We thank all researchers who have contributed to these fields.

---

**AOEM is not a faster execution engine, but a semantic framework that enables future hardware to participate in trusted execution.**
