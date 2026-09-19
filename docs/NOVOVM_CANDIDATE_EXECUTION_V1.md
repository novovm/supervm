# NOVOVM Candidate Execution V1

Status: local isolated candidate execution and result persistence contract.
Not authoritative publication, remote proposal admission, consensus, proof
sealing, canonical promotion, or mainnet sign-off.

## Scope and ownership

This extension consumes a ready [Candidate Workspace V1](NOVOVM_CANDIDATE_WORKSPACE_V1.md)
with a complete plan and independently copied, verified local parent state.
The local API is
`tx_ingress::candidate_workspace::{execute_v1, load_execution_v1}`.
It preserves the workspace's existing AOEM ownership, persistent backend,
namespace, chain and operator protocol-pin requirements.

The execution sequence is:

```text
ready input and copied parent
  -> authenticate the entire ordered transaction batch
  -> AOEM generic raw precommit
  -> Host business computation against the parent copy
  -> bound output reservation and chunks in AOEM storage
  -> complete readback and validation
  -> independent execution-completion marker
```

NOV account, balance, nonce, fee, policy and module semantics remain Host code.
AOEM executes existing generic precommit operations and persists the candidate
result through its generic graph/storage interface. The reported owners are
`business_transition_computation_owner = "SUPERVM_host"` and
`persistence_owner = "aoem_runtime"`. This is not a claim that AOEM understands
NOV business rules or computes those business transitions internally. No AOEM
repository, ABI, export, DLL, or NOV-specific kernel opcode changes are needed.

V1 accepts authenticated native `Execute` transactions, not the separate raw
`Transfer` or `Governance` variants. A business execution failure can produce a
valid failed receipt; completion does not require every receipt to have
`status_ok = true`.

## Authentication and isolation

Before raw precommit, the entire ordered batch must pass decoding, canonical
hash, signature/signer, subject authority and chain-domain verification.
Candidate-local nonce sequencing starts from the copied parent's durable
nonce map. Duplicate signed intents, duplicate nonce keys, nonce gaps,
overflow and intents already committed in the parent are rejected. A later
invalid transaction rejects the batch before any of its AOEM raw precommit
operations or output reservations are submitted.

Authentication calls the pure verifier rather than ingress, including ingress
variants that suppress admission but still publish rejection observations.
It neither reserves nor releases process-wide pending nonces. Two competing
candidates may therefore use the same identity/nonce against the same copied
parent without conflicting with one another or consuming the live pool.

Host dispatch receives a cloned store, a fixed plan timestamp, the already
authenticated key algorithm, supplied precommit metadata, an in-memory mirror
record collector and disabled policy observability emission. Policy checks
still apply. Dispatch must not query a machine-local UCA database, append the
live semantic mirror, or publish candidate demand as live governance events.

The isolation guarantee concerns NOV authority: authoritative AOEM head and
snapshot chunks, balances, durable nonces and receipts, Host projection,
ledger pointers and indexes, prepared slot, pending records and runtime nonce
reservations are not published or advanced by candidate execution. It is not
a byte-for-byte freeze of every shared AOEM key or runtime counter. Generic raw
precommit can update auxiliary digest keys, caches and metrics; candidate
input/output writes also change the AOEM database. These auxiliary effects are
not candidate promotion or a published authoritative NOV state transition.

## Persisted results and honest status

Each output contains the complete candidate store, batch result, expected
output commitment, workspace ID and input digest. A bounded descriptor binds
the output length, domain-separated digest and exact input digest. Validation
checks transaction authentication again, deterministic batch identity, state
and receipt roots, result/evidence bindings, authority domain, protocol pin,
nonce progression, receipt count, state-version continuity and unchanged
ancestor receipts. This local consistency checking is not an independent
validator replay or a cryptographic proof that the Host executed correctly.

Only a validated complete result returns `ExecutionInfoV1` with:

```text
transactions_authenticated = true
aoem_called = true
execution_completed = true
candidate_state_persisted = true
authority_state_published = false
chain_canonical = false
proof_sealed = false
safe = false
finalized = false
```

`aoem_called` in a recovered result describes the persisted execution; loading
or replaying that result does not mean AOEM raw execution was submitted again.
The returned post-state root, receipt root and execution-evidence commitment
are candidate outputs, not finalized-chain assertions.

The input API and its `WorkspaceInfoV1` remain input-only. Its
`transactions_authenticated` and `execution_completed` fields stay false even
when an execution result exists. An input ready marker cannot substitute for
the separate execution-completion marker or validated result readback.

## Capacity, recovery and abort

The output budget is independent of the input budget:

- At most 8 MiB per serialized result and 64 MiB of output reservations in a
  chain/namespace scope.
- Outputs attach to the existing 32 input slots; no additional slot pool is
  created.
- Incomplete and aborted output reservations still count. Abort does not
  refund slots, input bytes or output bytes, and V1 has no candidate GC.

These serialized-payload limits do not bound total RocksDB disk consumption,
memory usage, auxiliary AOEM precommit keys, or computation spent before an
oversized result is rejected.

Output reservations, chunks and completion markers occupy separate workspace
keys. Completion is published only after complete output readback and
validation. It binds the input workspace, scope and output descriptor. A
missing, malformed, mismatched or corrupted completed output fails closed.

After a reservation or partial output write, retry computes against the same
copied parent and must reproduce the exact reserved output descriptor and
bytes. A different recomputation cannot silently replace that reservation.
The serialized store includes local runtime metadata as well as consensus
state. Changing hardware/thread settings during a partial-output retry can
therefore reject exact-byte recovery even when consensus roots would agree;
abort remains available. Arbitrary cross-configuration partial resume is not
guaranteed.
If all output bytes were written before interruption, retry reads and validates
them and can publish completion without rerunning business transitions or AOEM
raw precommit. A completed result is a read-only replay with validation. The
read-only loader does not promote an incomplete result to completed.

A ready input copy remains executable even after authority has advanced and
garbage-collected its old snapshot. Existing results can also be recovered
without requiring the live head to remain at their parent. Protocol/config
drift still fails closed; this is not permission to execute under new rules.

The input abort marker has priority over input readiness and all output states.
An aborted workspace cannot execute, load a completed result as usable, or be
revived. Abort is not authority rollback. Execution uses the workspace's same
physical lock and uncertain-commit poison fence; an unknown graph commit
outcome requires process exit before durable recovery can proceed safely.

## Versioned nonce identity

Candidate authentication shares the authority and ingress rule in
[Native Nonce Identity V2](NOVOVM_NATIVE_NONCE_IDENTITY_V2.md). The authenticated
Ed25519 public key now identifies the nonce domain, so account-text aliases
and the same key's accepted 20-byte/32-byte caller forms cannot open separate
nonce buckets. Signed intent hashes and business account keys are unchanged.

Legacy or unknown parent-state identity markers fail closed. The compiled
protocol pin changes with the new rule; there is no candidate-only rewrite or
automatic legacy-state normalization. The pure offline migration preflight
requires complete signed history and rejects canonical duplicate nonce use.
It does not authorize an import or activation. An existing chain still needs
an explicit, verified upgrade/bootstrap procedure before V2 can be activated;
a passing isolated parity test does not establish that transition.

## Verification contract

The canonical required gate already runs:

```sh
cargo test -p novovm-node --lib candidate_workspace -- --test-threads=1
```

The execution tests use `candidate_workspace_execution` names and are covered
by the existing `test_native_candidate_workspace` field. Execution introduced
no additional field. The subsequent nonce-identity slice adds the required
`test_native_nonce_identity_v2` field. The later
[offline checkpoint bundle](NOVOVM_NATIVE_NONCE_CHECKPOINT_BUNDLE_V1.md) adds
`test_native_nonce_checkpoint_bundle`. The subsequent
[upgrade staging slice](NOVOVM_NATIVE_NONCE_UPGRADE_STAGING_V1.md) adds
`test_native_nonce_upgrade_staging`, extending the contract to 47 fields. The
later [upgrade authorization slice](NOVOVM_NATIVE_NONCE_UPGRADE_AUTHORIZATION_V1.md)
adds `test_native_nonce_upgrade_authorization`. The subsequent
[source prepare-QC slice](NOVOVM_NATIVE_NONCE_SOURCE_QC_V1.md) adds
`test_native_nonce_source_qc`, extending the contract to 49 fields. The later
[native seal new-view slice](NOVOVM_NATIVE_SEAL_NEW_VIEW_V1.md) adds
`test_native_seal_new_view`, making the current contract 50 fields.
Source prepare votes do not establish execution or finality.
Bundle verification, staged upgrade artifacts and separate
validator consent certificates do not activate legacy-state migration or
promote a candidate.
The direct execution-only filter is `candidate_workspace_execution`.

Before recording a pass, verification must establish:

- Competing candidates produce separate results from the same parent without
  changing the authoritative/pending fingerprint, including when live pending
  already uses their identity and nonce.
- A chosen candidate matches the existing authority path's full store, state
  root, receipt root, nonce outcome and batch/evidence result for the same plan;
  include both successful and failed business receipts.
- A ready but not-yet-executed input can execute after real authority snapshot
  GC, and completed results remain loadable without resubmitting raw precommit.
- Invalid signatures, subjects, chain domains, nonce sequencing and already
  committed intents reject before output publication and leave authority and
  pending unchanged, including a batch whose last transaction is invalid.
- Interruptions after output reservation, partial write, full write and
  completion recover according to durable facts; abort remains terminal.
- Corrupt output descriptors/chunks/markers and exhausted output capacity fail
  closed rather than overwriting previous reservations or claiming completion.

These are acceptance requirements, not a claim that all commands or scenarios
have passed. Record actual test output separately. Same-process reopen and
acknowledged-checkpoint interruptions are not hard-crash, power-loss, or
independent-process execution-result recovery evidence. The existing input
workspace subprocess test alone does not establish those result guarantees.
The execution filter adds its own three-process test with phase/PID evidence:
execute and exit, reopen/replay/abort and exit, then reopen and reject revival.
It also checks a writable governance-event sentinel during explicit and
fee-asset-induced privacy-policy rejection. This remains clean-exit evidence,
not a hard-crash claim.
Linux CI, multiple devices, public-network operation, concurrent process
contention and long-running operation each require separate evidence.

## Remaining boundary

This API adds no remote proposer admission, body-acquisition authority, durable
network scheduler, voting/QC, fork choice, authoritative head publication,
ledger block insertion, canonical promotion, CLI/RPC command, or automatic node
mode. Isolated candidate execution is preparation for independently verified
consensus and recoverable promotion, not proof-sealed mainnet finality.
