# Isolated candidate promotion protocol v1 — implementation contract

Status: DESIGN / NOT IMPLEMENTED. Code audit baseline: `b049f25`.
This is not production acceptance or permission to mark a block finalized.

## Observed implementation boundaries

- `native_candidate_execution.rs::Output` already contains the complete verified
  business store, batch result and expected output commitment. Promotion must
  publish that exact output; it must not execute the transactions again.
- `native_candidate_block_artifact.rs::with_verified_block_candidate_v1` holds
  workspace, authority and ledger locks, but hands out a read-only ledger view.
  Calling a ledger writer inside this callback would deadlock or be rejected.
  Promotion needs its own locked orchestration, not mutation inside this callback.
- `native_block_seal_commit_v3_store.rs` archives a verified certificate and pins
  its original witness. This proves a decision, not an authoritative state update.
- `native_block_ledger.rs::prepare` deliberately fences registered isolated
  plans. `commit` only implements the old selected/unsealed projection. Neither
  method is a promotion API and their guards must remain intact.
- `tx_ingress.rs::commit_native_state_envelope_via_aoem_graph_v1` writes chunks
  first and publishes the authority head as the AOEM completion write. Later GC
  can remove previous chunks. This is NOT atomic with the separate ledger DB.
- `recover_nov_native_block_ledger_from_aoem_v1` reconciles the old prepared record
  and rejects divergent AOEM/ledger heads. A new promotion cannot be recovered
  merely by ignoring this check or fabricating an old prepared record.
- Durable block/header/head/index validators currently reject sealed/finalized
  flags. Mutating those flags is not a compatible finality implementation.

## Required commit sequence

1. Hold workspace then authority locks. Revalidate exact live output, registered
   binding, current parent and locally archived V3 certificate against the pinned
   epoch authority. Reject competing decisions. Do not trust caller booleans.
2. Before any AOEM authority write, synchronously persist a ledger promotion
   intent plus an independent immutable evidence pin. Pin the complete decision,
   workspace/plan/output commitments, source parent, destination block and exact
   expected AOEM envelope commitment. Only one target per chain/height is allowed.
   A database capability marker must fence unsupported older writers.
3. Publish the exact verified output through AOEM's generic graph interface.
   Keep NOV-specific authorization and recovery policy in the Host. Read back
   and validate the complete authoritative envelope; an error is not proof that
   nothing was written. Unknown completion must fence further authority writes.
4. In one synchronous ledger batch publish the selected block/indexes, promotion
   completion and finality record/pointer. Preserve the original candidate and
   QC evidence. Abort competing unselected descendants under explicit rules.
   Do not rewrite a signed block commitment to attach finality metadata.
5. Only after durable verification may Host projections, pending removal and
   receipt/transport notifications report the corresponding completion level.
   A failure in projection/notification must be replayable without re-execution.

The promotion intent must fence ordinary execution, abort/GC of its source
workspace, and competing promotions. Startup must resolve it before the old
recovery path, network signing, or transaction execution can run.

## Ancestry and trust root

A child QC alone cannot finalize an arbitrary historical parent. Normal promotion
requires the selected parent to be the recorded finalized ancestor, with matching
state/receipt roots and epoch authority. First-chain activation needs a separate,
explicitly authorized genesis/checkpoint initialization contract: the current
operator-pinned genesis hash must not silently promote an existing unsealed tail.
No implicit migration of an old selected chain is authorized by this document.

## Recovery matrix to implement and test

| Durable intent | AOEM state | Ledger pointer | Required action |
| --- | --- | --- | --- |
| Present and valid | Exact parent | Parent | Replay only the pinned publication |
| Present and valid | Exact target | Parent | Verify output, finish ledger batch |
| Present and valid | Exact target | Target | Verify indexes/finality; replay projection only |
| Missing/corrupt pin or intent | Any divergence | Any | Stop; never synthesize authorization |
| Valid intent | Neither parent nor target | Any | Stop; no automatic rollback or replacement |
| Conflicting decision or target | Any | Any | Stop and preserve both evidence sets |

Fault injection must cover before/after intent sync, partial AOEM chunks, authority
completion before acknowledgement, before/after the ledger batch, and before/after
Host projection. Include independent-process restart, loss/tampering of evidence,
wrong epoch, missing body, 2/4 signatures, competing same-height candidates and
attempted legacy mutation while recovery is pending. Finality flags remain false
until the complete implementation and these tests prove the transition.

Multi-service V3 tests prove consensus on unpromoted candidates only. Main-process,
physical LAN, public-network, Linux installation and long-run release acceptance
remain separately required by the production-readiness tracker.
