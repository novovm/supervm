# Isolated candidate promotion protocol v1 — implementation contract

Status: DESIGN / NOT IMPLEMENTED. Code audit baseline: `b049f25`.
This is not production acceptance or permission to mark a block finalized.

Ledger-only prerequisite now implemented: `reserve_fresh_genesis_v1` atomically
reserves an unused ledger with configuration commitments, an independent digest
pin and a capability marker. Exact retries verify the existing reservation;
ordinary access (including already-open writer handles) is fenced. Missing
schema in a nonempty DB is rejected rather than repaired. The production
initialization and promotion sequences below remain NOT IMPLEMENTED.

This library API has no CLI/RPC/startup activation. It does not validate the
full genesis manifest or operator authorization, inspect AOEM namespace usage,
publish initial state, generate keys, or mark genesis finalized. Its current
purpose is to hold the ledger closed until an explicit recovery coordinator is
implemented; do not invoke it against a running node's data directory.

`tx_ingress::fresh_genesis` now compiles an explicit, bounded v1 configuration
into a fresh unowned input store and a shared configuration commitment. Only
canonical NOV base-unit allocations and validated weighted public keys are
accepted; test snapshots, nonce history and receipts are not input fields.
Allocation ordering and validator ordering do not affect the result. The
commitment binds chain, timestamp, protocol commitment, existing consensus state
root, validator-set hash and declared allocation total. v1 uses epoch 1 and
activation height 1. Every other module field starts at its fresh default; these
defaults are bound by the state root, not populated from the environment.

The compiled commitment is NOT a block hash, execution proof or certificate.
The compiler does not approve economic policy or validator membership. An
out-of-band expected commitment must match before producing reservation inputs.
The local AOEM namespace is bound by the reservation, never the shared genesis
commitment. The future coordinator must still verify the runtime protocol pin,
storage/namespace freshness,
publish via AOEM and verify readback before activating the ledger trust anchor.

Full manifest reservation is now available through the explicit library API
`reserve_fresh_genesis_config_v1`. One synchronized RocksDB batch stores the
manifest, exact-byte archive digest, recomputed reservation, reservation pin and
a distinct capability marker. `load_fresh_genesis_config_v1` opens read-only,
requires the externally pinned configuration commitment and local namespace,
recompiles the stored manifest and compares every reservation field. Missing,
corrupt or conflicting evidence fails closed; retry never repairs it. Existing
hash-only reservations are not upgraded. Ordinary startup/writers remain fenced.
This is durable input recovery only, not AOEM publication recovery, a finalized
genesis or proof that the AOEM namespace is unused. Hard-crash tests and actual
production activation remain unexecuted.

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
state/receipt roots and epoch authority.

The user confirmed on 2026-09-29 that production starts from a fresh genesis,
without inheriting a test ledger. First-chain activation therefore needs a
separate, explicit fresh-genesis initialization contract, not a checkpoint import
or conversion of the current selected chain. The operator-pinned genesis hash
must not silently promote an existing unsealed tail.

Required fresh-genesis boundaries (implementation and acceptance still pending):

- Use explicitly selected production storage and AOEM state namespace, separate
  from test ledgers, candidate workspaces, seal stores and signing journals.
  Preserve all existing test data; never clear a nonempty target automatically.
- Refuse pre-existing chain state at initialization. A restart is allowed only
  through verified recovery of the exact pinned initialization record; an empty
  directory alone is not proof that the AOEM namespace is unused.
- All production nodes must verify the same canonical genesis configuration and
  hash, including chain domain, protocol versions, initial state commitment and
  validator authority. Independently generated node keys do not authorize each
  node to invent a different genesis.
- Do not import test balances, nonces, receipts, block indexes, candidate/QC
  evidence or signing history. Initial allocations, if any, come only from the
  explicitly approved production genesis configuration, never test fixtures.
- Keep genesis authorization separate from a normal block QC. Do not synthesize
  validator signatures or relabel historical test blocks as finalized.
- Persist initialization intent and immutable configuration before publishing
  the AOEM initial state. Verify readback before completing the ledger trust
  anchor. Recovery must reject different configuration, namespace or state.
- No default production activation, deployment, key generation or economic
  parameter selection is authorized by the fresh-genesis decision alone.

Acceptance must cover clean initialization, exact-config restart, interrupted
initialization, changed configuration, occupied test storage/namespace and
preservation of test data. Relative/config-resolved paths remain supported; no
particular drive letter or workspace directory name is required.

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
