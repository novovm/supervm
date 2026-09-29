# Isolated candidate promotion protocol v1 — implementation contract

Status: first-height publication/finality implemented; continuous-height activation incomplete.
Original audit baseline: `b049f25`. This is not production acceptance or permission
to mark any block finalized without the full first-height verification below.

Latest first-height implementation: a separate pinned finality record now stores
the full V3 decision envelope and genesis-derived epoch authority. Validation
checks the scheduled proposal, signatures/quorum, round/new-view dependencies,
and exact decision equality with the selected promotion. Only after live AOEM
readback and complete ledger publication can the coordinator persist the record.
A distinct finalized capability marker fences older readers/writers. Replay and
read-only evidence queries verify the manifest, candidate graph, selected intent,
all block indexes and the complete finality proof; no signed header flag is edited.
The main-node relay reports first-height BFT finality only after that verification.
This is quorum-certified execution under the pinned validator set and its BFT
fault assumptions, NOT a zero-knowledge execution proof. Subsequent heights,
competing-workspace cleanup and hard-crash acceptance remain incomplete.

The next-height input boundary now exposes an opaque, immutable
`FinalizedGenesisParentV1` captured under workspace/authority locks only after
full AOEM/output/ledger/finality verification. Its `successor_plan` derives the
parent batch/state/receipt bindings and authenticates transactions against the
captured nonce state, without pending admission, nonce writes or execution.
It is historical data, not a transferable live signing/publication capability;
child staging/publication must recheck current-parent ownership.
`create_from_finalized_genesis_v1` now stages height two after a live-parent
recheck, including exact ready replays. Its separate archived parent variant
binds the full first-block decision, state and receipt roots; it never fabricates
a legacy production envelope. `execute_v1` persists the isolated result and
reopens it idempotently without changing first-height authority. Legacy signing
and registration reject fresh-chain artifacts. Main-node second-height admission,
confirmation and promotion are still pending; this is not continuous finality.

Second-height unsigned subjects use the distinct
`novovm-native-proof-seal/fresh-successor-decision-v1` profile. In this profile,
the serialized `justify_qc_hash` field means the stable parent V3 decision target,
not a prepare-QC hash or certificate hash. The complete parent decision envelope
must verify before deriving that target; a prepare QC alone is insufficient.
This preserves the same child dependency across valid parent signer subsets.
`successor_seal_subject` also checks genesis identity, exact parent batch/roots,
height, slot, timestamp and transaction-count state-version advancement. It is
historical construction only: old transport authorities reject this profile,
and network activation/publication still require new wiring.

`register_finalized_successor_v1` now registers height-two artifacts atomically,
under workspace -> authority -> ledger locks after a fresh live-parent check.
The finalized manifest validates the exact candidate, execution pin, height and
parent/children indexes; it never accepts arbitrary new key prefixes. Exact
replay preserves the record; missing committed evidence fails closed rather than
being rebuilt. Competing candidates remain unselected and unsealed. Authority,
published first-block indexes and finality records are unchanged. Older readers
reject the newly populated keys via their exact manifest allowlist. Candidate
cleanup and second-height network activation/promotion remain unimplemented.

`with_verified_finalized_successor_v1` supplies a non-escaping, read-only live
signing view under workspace -> authority -> ledger locks. It rechecks published
parent AOEM state, full V3 finality and the exact registered child output before
allowing the existing seal store to prepare a subject. The view supplies the
verified stable parent decision target, never a caller-selected prepare-QC hash.
Existing signature safety locks, round admission and V3 durable signing are
reused. Separate first-height signer stores can continue at height two, form a
3-of-4 decision and reopen it without resigning; a decision certificate alone
still does not select the candidate, publish state or finalize height two.

Ledger-only prerequisite now implemented: `reserve_fresh_genesis_v1` atomically
reserves an unused ledger with configuration commitments, an independent digest
pin and a capability marker. Exact retries verify the existing reservation;
ordinary access (including already-open writer handles) is fenced. Missing
schema in a nonempty DB is rejected rather than repaired. The production
initialization now has an explicit preparation CLI; the complete promotion sequence remains incomplete.

The first-block promotion journal is implemented and used by the explicit fresh node loop:
`prepare_genesis_promotion_v1` revalidates live genesis/output and a locally archived V3
decision, then atomically stores its exact candidate/output binding, certificate, digest
pin and capability marker. Exact replay is allowed; another target, damaged evidence,
signing/registration after intent, and abort of the source workspace are rejected.
No AOEM authority pointer or selected ledger index is changed by this operation.
AOEM publication is now available separately; ledger completion/recovery and
continuous-height activation still need implementation.

`publish_genesis_promotion_v1` verifies the pinned intent, complete candidate output
and live captured genesis under workspace/authority locks, then publishes an NVP1
pointer to the existing immutable AOEM output. No transaction is re-executed.
The evidence sidecar precedes the authority completion write. Exact retries and
`verify_genesis_promotion_v1` fully read back the same target without repairs.
Unknown commit outcomes retain the authority lock until process exit. Old authority
readers reject this new codec. Ledger publication is a separate completion step,
and finalized remains false. The fresh node loop now invokes the coordinator below;
ordinary state queries are not yet activated. Real-AOEM tests
inject errors before/after publication and reject damaged pointers/output; these
are not independent-process kill/recovery evidence.

`complete_genesis_promotion_v1` now extends verified AOEM publication with a single
synchronous ledger batch: unchanged signed header/body/evidence, head/height,
transaction/receipt and external AOEM ID indexes, and an intent-bound completion
marker. A distinct capability schema retains old reader/writer fences. Full
manifest/graph/intent and every exact projection entry are checked on replay;
damaged or missing committed indexes are not rebuilt. The explicit
`load_fresh_genesis_published_block_v1` reads this complete ledger projection;
it does not attest live AOEM or chain finality. Finalized remains false until
finality/query and node lifecycle integration are completed. The existing
candidate graph remains immutable historical evidence, not a finalized view.

The fresh first-height main loop now invokes publication after a locally durable
V3 decision. Startup resumes the same intent/output/archive through
`resume_genesis_promotion_v1`; mismatched service ledger paths or archived decisions
are errors. Once published, a keyless driver relays the archived prepare QC and
decision certificate using the existing bounded/fair resend machinery. It verifies
the live published AOEM/ledger state before sending and never re-enters signing.
Both certificates are necessary for a previously offline peer to catch up. This
does not enable new transaction admission, subsequent heights or chain finality.

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

`fresh_genesis::publication::publish_v1` now provides explicit AOEM initial-state
publication, still without CLI/startup activation. It requires the complete
reserved manifest, matching runtime protocol pin, explicit AOEM ownership and
no existing Host projection in any supported backend. First publication refuses
an existing AOEM DB; a synchronized local ownership claim binds retry to the
physical authority lock, namespace, configuration and exact image digest. Lost
or incomplete claims stop recovery; they are not automatically reconstructed.

The generic AOEM graph writes 512-byte chunks, then a distinct 152-byte `NVG1`
authority head as its completion write. No transaction batch result, receipt or
QC is fabricated. Readback compares the complete exact image derived from the
archived configuration. No head plus matching partial chunks permits replay;
an existing head with missing/changed chunks fails closed instead of repair.
An uncertain commit/readback retains the authority OS lock until process exit.
The reserved ledger and incompatible genesis head deliberately keep ordinary
transaction startup blocked until genesis trust-anchor activation is implemented.
Local graph fault-state tests are not independent-process crash or power-loss
proof; claim-file crash durability, startup integration and finality remain open.

`verify_persisted_v1` is a separate verification-only entrypoint: it requires
existing RocksDB storage and claim, never submits a graph, never creates the
claim, and refuses a missing completion head instead of replaying publication.
It acquires the normal authority lock (including diagnostic file updates) and
opens the generic provider; it is not a filesystem-read-only RPC. Its returned
status must not be reused later as permission to activate a ledger without live
revalidation under the authority lock.

### First-block integration constraints found in the current code

- `NovBlockExecutionContextV1` requires height 1 to have a zero parent hash.
- Candidate plans and durable transaction blocks require nonempty transactions.
- Legacy `prepare_local_subject` and seal-store binding fetch height 1 as the
  genesis block; the legacy profile requires `genesis_block_hash == block_hash`.
  The explicit fresh-genesis signing scope below now uses a separate profile.
- Legacy isolated parent capture requires a transaction-state envelope and a
  durable parent block. The explicit `create_from_genesis_v1` path now supports
  the distinct genesis image as described below; legacy signing still rejects it.

Consequently the new initial state cannot be relabeled as an existing-format
transaction block. The next integration must explicitly represent the genesis
trust anchor and bind first-candidate admission, signing domain and verified
initial state to it. It must preserve legacy rejection rules/version fencing,
not synthesize transactions, batch results, QCs or historical finalized flags.
Ordinary startup remains blocked until these interfaces and recoverable ledger
activation are implemented together.

### First isolated candidate from fresh genesis

`candidate_workspace::create_from_genesis_v1` takes an explicit expected genesis
commitment, obtains workspace then authority locks and checks the complete
reserved manifest against the live AOEM genesis head/chunks. The bounded reader
validates length, digest, chain, namespace, state root, full configuration and
the exact fresh state. No parent transaction result or parent QC is synthesized.
Only height 1, zero parent sentinel, absent transaction-parent metadata and a
timestamp no earlier than genesis are accepted. The pre-state root must match.

Stored candidate inputs represent exactly one of an executed parent or a fresh
genesis parent. Old transaction-parent JSON remains readable; older readers
reject the genesis variant. The existing authentication, isolated AOEM execution,
output verification and block-artifact code are reused. Ready replay is historical
input reuse, not evidence that genesis is still current; the expected genesis pin
must still match the saved input. Legacy registration/signing remains fenced;
the explicit fresh-genesis coordinators below perform live validation separately.
Ordinary startup and finality activation remain incomplete.

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

## Fresh-genesis identity foundation (implemented, not signing activation)

`CompiledFreshGenesisV1::identity()` derives a typed chain identity with the
domain `novovm-fresh-genesis-chain-identity-v1\0`, big-endian chain ID and the
canonical configuration commitment. That commitment binds genesis time, initial
state, protocol and validator set. Local paths/namespaces and first transaction
candidate hashes are not identity inputs. Private fields prevent unchecked
construction; compilation itself still does not prove operator approval or live
AOEM ownership.

Verified isolated first-block artifacts expose this identity, reconstructed from
their validated archived genesis input rather than caller-provided metadata.
Legacy transaction-parent artifacts expose `None`; no chain identity is guessed
from missing genesis input. This describes historical input only and does not
authorize signing, registration, state publication or finality.

Real AOEM regression executes two competing first-block candidates, verifies
different block hashes with identical genesis identity, reopens the artifact and
confirms the authoritative genesis head remains unchanged. Reserved-ledger
legacy registration still rejects both. Existing seal subjects, authority codecs and
height-one signature checks remain unchanged: the new anchor MUST NOT be inserted
into the legacy `genesis_block_hash` field as if it were a transaction block hash.
The independently versioned signing domain and live ledger scope described below
are separate from this historical identity accessor.

## Fresh first-candidate registration (implemented, no implicit signing)

The explicit `register_genesis_block_candidate_v1` coordinator now holds the
workspace lock, verifies completed AOEM output and the approved configuration pin,
then holds the authority OS lock while rereading the live genesis image. A missing
or changed authority head or aborted workspace rejects even an exact replay.

Under the ledger writer lock it validates the complete reserved manifest and all
existing first-candidate evidence. One synchronous batch writes a new capability
marker (`v1+genesis-isolated-candidates-v1`), immutable block artifact, isolated
execution binding pin, graph record and height/children indexes. Competing height
one candidates are allowed only here, under the same approved genesis. There is
no selected head, AOEM transaction ownership record, transaction/receipt index or
state publication. Ordinary ledger APIs and previous binaries remain fenced.

Manifest recovery validates the full first-candidate graph and an exact key
allowlist: missing records, pins, artifacts or indexes, mismatched indexes,
orphaned keys, changed bindings and capability downgrade cannot be repaired by
registration retry. Wrong chain, namespace, initial root, pre-genesis timestamp or
non-initial state version reject before the candidate batch. These checks are
historical ledger evidence, not live AOEM capabilities. Workspace abort prevents
reuse but does not rewrite the historical registration record.

This registration entrypoint does not admit network proposals or sign votes.

## Explicit fresh-genesis signing and V3 certificates (local API implemented)

`with_verified_genesis_block_candidate_v1` shares the live workspace/output and
AOEM genesis verification used by registration. It then validates the complete
manifest/graph under the ledger lock, verifies the exact registered artifact and
execution binding and provides only a callback-scoped read-only ledger view.
Ordinary opens remain fenced. Neither selected height-one nor an AOEM transaction
ownership record is synthesized. The view cannot authorize another candidate or
perform ledger writes, and workspace abort rejects subsequent use.

The proof profile `novovm-native-proof-seal/fresh-genesis-v1` interprets the
existing signed `genesis_block_hash` slot as the domain-separated genesis anchor,
not a transaction block hash. This explicit discriminator is covered by subject,
proposal, prepare-vote and V3 decision signatures. Legacy profile validation and
wire field layout remain unchanged; older verifiers reject the new proof version.
Only height one / epoch one / activation one / state version one is admitted by
this initial profile. Continuation heights remain fenced until promotion and
ancestry verification are integrated. The supplied validator set must equal the
compiled approved genesis set, not merely be a self-consistent alternate set.

Seal-store binding uses a distinct fresh-genesis schema and hash domain, with the
shared genesis anchor plus local namespace and protocol pin. Competing first
candidates share that binding and the existing per-height safety locks; this is
not a new signing journal per candidate. V3 preparation/decision thresholds and
persist-before-emit rules are reused without relaxing old-version lock exclusion.

Real AOEM regression uses four signers with separate seal databases and one
candidate authority. Two signatures fail both prepare and decision quorum; three
form a QC and V3 certificate. Conflicting candidate signing rejects, decision vote
replay survives reopen, and certificate persistence/reopen is idempotent without
changing the AOEM genesis head or finalized flags. This is not four independent
executing nodes, network delivery, process-kill recovery or mainnet finality.
The explicit Overlay authority/wire integration below is separate from node
service dispatch and independent-node fresh-genesis acceptance, which remain
incomplete.

## Fresh-genesis Overlay identity and bounded admission (implemented)

`derive_operator_pinned_fresh_genesis_epoch` constructs a deterministic authority
manifest from the complete approved genesis configuration and explicit transport
bindings. It verifies the expected configuration commitment, uses the compiled
validator set and shared genesis anchor, canonicalizes bindings and retains
existing peer/validator bounds. Its distinct authority kind is included in the
authority commitment. Pure construction is not signing permission: validation
against a ledger requires the live fresh-candidate scope and exact genesis,
protocol and validator-set match. Legacy ledger authority checks are unchanged.

Proposal/prepare artifacts, round messages, highest-QC new-view evidence and the
V3 collector now share an authority-to-subject domain check that explicitly pairs
the fresh authority kind with the fresh proof version. A valid signature under a
different semantic profile cannot enter merely because its chain fields match.

Pre-first-block admission allows local execution height zero only for the fresh
authority kind activated at height one. Existing legacy ingress still rejects
zero height. The first-block proof profile and existing bounded height/round/wire
checks remain in force; receiving a proposal does not select or finalize a block.

The real AOEM fixture verifies canonical authority ordering, wrong approval and
different genesis rejection, proposal wire roundtrip, authenticated-source checks,
quarantine authority persistence/reopen and proposal reconciliation against live
local execution. It also transports V3 votes/certificates through the bounded
round codec and collector: two votes stay incomplete, three complete. Sources are
fixture-supplied authenticated identities, NOT actual socket connections. The
fixture initially exposed the old height-zero rejection; this was fixed in the
fresh authority path instead of fabricating an executed height-one ledger head.

Main service configuration/dispatch still needs an explicit approved-genesis pin
and fresh scope on every poll. No production startup, automatic registration,
continuous-height processing or finality promotion is enabled by this slice.

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
