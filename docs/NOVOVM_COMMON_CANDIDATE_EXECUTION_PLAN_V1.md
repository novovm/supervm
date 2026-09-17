# NOVOVM Common Candidate Execution Plan V1

Status: local Host execution slice; not network consensus or finality.

## Purpose

Independent nodes consuming their own pending queues and wall clocks do not
necessarily produce the same block. This slice introduces an explicit input
plan that can be selected by a local Host and executed against its own AOEM
authority. The same plan fixes the transaction order and execution-visible
time, including for the first block. It does not choose the plan for the network.

`NovNativeCandidateExecutionPlanV1` binds:

- protocol version and business protocol configuration commitment;
- chain, height, parent block hash, slot, and execution timestamp;
- pre-state root and the optional previous AOEM execution identity, state
  version, state root, cumulative receipt root, and root codecs;
- the complete ordered raw transaction body and canonical transaction hashes.

A domain-separated, length-delimited commitment covers these inputs. Paths,
local AOEM namespaces, worker counts, and wall clocks are not plan fields.
Structural validation caps the body at 1,024 transactions and 2 MiB; it is not
signature, nonce, proposer-authority, or execution-proof validation.

The execution plan is distinct from `NovNativeSealProposalV1`: the latter
signs an already executed candidate's subject and contains no raw body.

## Execution boundary

The Rust Host API is
`tx_ingress::run_nov_native_candidate_execution_plan_v1(plan, node_params)`.
It requires explicit AOEM production ownership and the existing operator
protocol pin. Configuration remains local; the plan cannot select a data
directory, AOEM namespace, auth exception, execution result, or finality flag.

The API does not read or fill its batch from pending. It uses exactly the plan
body, independently authenticates each signed V3 transaction, reconstructs its
canonical hash, checks chain and nonce, and compares the pre-state and AOEM
parent against local authority under the existing store write lock. A new
context must extend the local durable execution head without time/slot
regression. A competing prepared plan is rejected rather than overwritten.

Only after these checks does the normal Host business transition / generic
AOEM graph persistence path execute. AOEM remains domain-neutral: this slice
changes no AOEM repository, DLL, export, opcode, or NOV-specific engine logic.

`block_execution_context` in transaction/RPC JSON remains forbidden, including
on the exact-replay path. Typed Host configuration cannot override the plan
with another body or JSON context. No RPC method or node CLI is added for plans.

## Replay and crash recovery

The existing durable prepared candidate stores the exact context, body, hashes,
pre-state and AOEM parent; the ledger ownership record binds the protocol pin.
Together these persist the inputs covered by the plan commitment without
introducing a second execution journal.

- A completed replay must match the local durable block's complete input, not
  just its transaction hashes. Changed time, order, parent, or pre-state is
  not an exact replay even if the transactions have already been committed.
- Recovery after AOEM persistence but before block commit must match the
  durable prepared input and validate AOEM readback through the existing
  recovery path. It must not execute the transactions again.
- Replaying an older completed plan must not commit an unrelated newer
  prepared candidate.
- Invalid plans do not advance AOEM business state or the block head. Existing
  local metadata binding and recovery of a stale Host projection from existing
  AOEM authority can still occur; this is not a promise of zero filesystem I/O.

## Evidence and limits

The `candidate_plan` test filter covers both the plan contract and actual AOEM
execution tests with isolated data stores, deterministic genesis and successor
inputs, durable readback comparison, exact replay, invalid-input rejection,
and the AOEM-before-block-commit recovery fault point. The canonical mainline
gate runs this filter with one test thread because the existing AOEM fixture
helpers temporarily set process environment variables.

This slice introduced the 43-field mainline lockset, including
`test_common_candidate_execution_plan`. The subsequent
[candidate workspace slice](NOVOVM_CANDIDATE_WORKSPACE_V1.md) extended the
lockset to 44 fields; [nonce identity V2](NOVOVM_NATIVE_NONCE_IDENTITY_V2.md)
now extends it to 45. Serializer, preflight and node-runtime locksets remain
synchronized; older status reports must be regenerated, not reused as current
evidence.

Two isolated stores in one test process are not two physical nodes. Matching
local candidates are not a quorum certificate or proof seal. All produced
candidates retain:

```text
canonical_local = true
chain_canonical = false
proof_sealed = false
safe = false
finalized = false
```

This API advances a single local execution head. It is not a scratch-state
executor and must not be used to try competing untrusted remote proposals.
The automatic node loop still uses its existing local pending scheduler;
this slice does not activate a new execution mode or silently disable it.

Still required before remote proposal-driven execution and finality:

- authority-bound body transport and acquisition, including common genesis;
- a bounded durable scheduling owner and an exclusive mode that prevents
  local pending execution from racing the selected network plan;
- isolated candidate state / safe handling of competing proposals;
- independent output validation, voting/QC, fork choice and recoverable
  canonical promotion;
- multi-process, physical LAN, public-network and long-run evidence.

No block interval, TPS, phone-validator role, or mainnet readiness is signed off
by this slice.
