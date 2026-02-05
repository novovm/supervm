# SuperVM

SuperVM is a **decentralized infrastructure operator** for the Web3 era. It provides composable, metered, and verifiable execution and settlement capabilities. It is not “another public blockchain,” but a general-purpose execution infrastructure for a multi-chain, heterogeneous ecosystem.

## What it is / What it isn’t

**SuperVM is:**
- A Web3 infrastructure layer that offers execution, verification, settlement, and resource pricing
- Built around `AOEM`, a high-concurrency execution kernel with stable P99 latency
- Trust-minimized through `zkVM`-based verifiable execution and proof aggregation

**SuperVM is not:**
- A monolithic high‑TPS public blockchain
- A single‑purpose cross‑chain bridge
- A network sustained primarily by inflation

## Architecture overview

- **Unified execution kernel (`AOEM`)**
	- Semantic concurrency with `OCCC` as the primary execution path
	- `OCC` as a validation baseline and `MVCC + 2PC` as a strict safety fallback
- **Three‑channel routing layer**
	- Standard Path / Consensus Path / Privacy Path
	- Transparent to developers; acts as a QoS routing mechanism
- **Four‑layer network (L1–L4)**
	- L1: Finality & arbitration
	- L2: Execution & proof workers
	- L3: Edge & routing nodes
	- L4: Clients & devices
- **L0 security kernel**
	- Zero‑knowledge verification (Groth16, Bulletproofs, RingCT / MLSAG)
	- Post‑quantum readiness (multi‑level ML‑DSA signatures)

## Core design goals

1. Execution‑first: execution is a first‑class capability, not a byproduct of consensus
2. Verification over trust: correctness is proven, not assumed
3. Meterable & settleable resources: compute, storage, and bandwidth are economic resources
4. Stable P99 latency under high throughput

## Verifiable execution path

SuperVM separates execution, proof, and consensus:
- **Execution** is handled by AOEM
- **Correctness** is proven via `zkVM`
- **Consensus** is limited to finality and arbitration

Proof generation is decoupled from execution:
- Proofs can be **lazy**, **batched**, and **recursively aggregated**
- `RISC0 zkVM` proves correctness, `Halo2` aggregates proofs

Verification is **value‑aware**:
- Standard execution (no immediate proof)
- Auditable execution (on‑demand proofs)
- High‑value execution (mandatory zk proofs)

## Governance & evolution

SuperVM is built for long‑term infrastructure evolution:
- Upgradable protocols
- Post‑quantum readiness
- Layered governance

## Economics: execution‑driven, not inflation‑driven

SuperVM’s economic model is anchored in **real, verifiable execution demand**:
- Execution is economic activity
- Compute, storage, and bandwidth are **settleable labor**
- Value capture is service‑driven, not speculative

**Native token boundaries (explicitly limited):**
- Unit of account for execution and service settlement
- Governance participation and risk‑bearing
- **Not** equity, **not** income‑sharing, **not** a stablecoin

**External value is required:**
- External assets (stablecoins, fiat‑pegged assets, other chains) provide pricing references
- Token circulation requires verifiable external value inflows
- No issuance driven purely by time or internal loops

**Dual‑track pricing:**
- Rigid redemption / clearing track
- Market trading / liquidity track

## Performance (whitepaper‑reported)

AOEM’s triple breakthrough toward a distributed execution plane:
- Compute Plane (L0): **8M+ TPS**
- Coordination Plane (L1): **4M+ TPS**
- Network Plane (L4): **1M+ msgs/s**

## Developer interface & SDK

SuperVM exposes a **unified Execution API** rather than exposing concrete execution engines.
Developers declare:
- Execution target (function, transaction, or task)
- Required consistency and security guarantees
- Required privacy / verification properties

The system automatically handles:
- Three‑channel routing selection
- Execution and proof generation
- Settlement and verifiable commitment of results

SuperVM is **WASM‑first** and multi‑language:
- Rust, C/C++, Zig, AssemblyScript, and more
- Portable, verifiable execution
- Reuse of existing high‑performance system code

## Privacy & security

Privacy and security are treated as **infrastructure primitives**, not optional features:
- Verifiable execution via `zkVM`
- Privacy proofs (e.g., Bulletproofs, Groth16, RingCT / MLSAG)
- Execution‑proof decoupling with on‑demand / batched / aggregated proofs
- Security boundaries backed by post‑quantum readiness

## Ecosystem positioning

SuperVM is a **collaborative infrastructure layer**:
- It does not replace existing chains
- It augments them with execution, verification, and settlement services
- Value capture is service‑driven, not sovereignty‑driven

## Compatibility & chain relationships

SuperVM complements heterogeneous systems rather than competing for their state:
- **L1 public chains**: execution outsourcing / clearing collaboration
- **L2 / rollups**: shared execution and proof infrastructure
- **Specialized chains**: plugin chains / protocol recomposition
- **Private chains**: verifiable execution with privacy‑preserving settlement

## One‑sentence summary

SuperVM is not about building a faster blockchain—it is about building sustainable, verifiable infrastructure for Web3.
