# NOVOVM Native Nonce Upgrade Authorization V1

Status: independent validator-consent certificate verification and a persistent
local signer fence for an offline nonce-upgrade proposal. Not protocol
activation, a canonical-checkpoint proof, an AOEM import, a block QC or mainnet
sign-off.

## Purpose and authority

The [checkpoint bundle](NOVOVM_NATIVE_NONCE_CHECKPOINT_BUNDLE_V1.md) verifies a
complete pinned legacy history. The [staging slice](NOVOVM_NATIVE_NONCE_UPGRADE_STAGING_V1.md)
constructs its deterministic four-field nonce-rule transition in a separate
workspace. This slice binds validator consent to that exact proposal without
changing the proposal, activating its rules or writing authoritative state.
All upgrade semantics remain in SUPERVM Host code. AOEM repositories, ABI,
opcodes and DLLs remain unchanged.

The verifier requires both an explicit old `NovNativeSealEpochAuthorityV1`
document and an independently established expected authority commitment. It
validates the authority and compares its commitment to that pin before
accepting its validators. Deriving the expected pin from an untrusted supplied
authority only proves internal consistency, not who may authorize the chain.
The expected source bundle digest and checkpoint anchors require the same
independent establishment described by the bundle document.

This uses the existing genesis epoch authority boundary: epoch 1, with the
validator set active from height 1. It does not add validator-set rotation,
new epoch admission, dynamic governance membership or an authority-fetch RPC.
An otherwise self-consistent authority from another chain, genesis, namespace
or legacy protocol is not an authorization for this transition.

## Signed subject and quorum

Upgrade signatures have their own domain and cannot be substituted for block
votes. The independently recomputed subject binds:

- Chain ID, genesis commitment and execution namespace.
- Old authority commitment, epoch and validator-set commitment.
- Source tip height and hash, with the proposed activation height exactly
  one greater than that tip; overflow rejects.
- Legacy and target protocol commitments.
- Source and proposed state roots, unchanged cumulative receipt root and
  semantic state version.
- Transition ID, exact source bundle and snapshot digests, and the ordered
  historical transaction commitment.

The verifier regenerates the transition from the explicit pinned bundle and
target; it does not accept certificate claims as the expected roots or
transition ID. Historical transaction authentication and nonce checks remain
required. The target commitment is explicit; observing this binary's current
configuration is not authorization to change an existing chain.

Votes use Ed25519 signatures from distinct members of the pinned old authority.
Every included vote must match the same complete subject and verify under its
validator key; duplicates, unknown members, tampered signatures and mismatched
subjects reject. Quorum requires strictly more than two thirds of the total
validator weight, not a fixed number of signatures. For four equal-weight
validators, three signatures suffice; other weight distributions must be judged
by their signed weight. Two of four equal-weight
votes do not authorize the proposal, and exactly two thirds never qualifies.

An accepted certificate records consent by that authority under the supplied
pins. It does not independently prove that the source tip is canonical, that
its historical business execution was correct or that the proposed state has
been published. A quorum of consenting validators is not an execution proof.

## Persistent local signer boundary

The library signer uses an explicit new-store creation or existing-store open;
it does not silently create an absent store while resuming. RocksDB's exclusive
database lock prevents cooperating processes from opening the same signer
store simultaneously. The store persists its identity and upgrade boundary
fence with a synchronous write before releasing a signature.

The anti-equivocation boundary is the signing key, chain, genesis, namespace
and activation height. Once fenced, that boundary may sign only the identical
subject. Changing the source checkpoint, target protocol, roots or authority
does not create another opportunity to sign at the same boundary. Repeating
the identical request is resumable, including an interruption after the fence
was committed but before a vote was returned. Corrupt or inconsistent stored
records reject instead of being replaced.

There is no signer-store reset, overwrite, key-replacement or signing CLI in
this slice. The operator must preserve the store together with the key and
must not clone the key into another signer store, restore a stale store backup
or delete the signing history. This local fence cannot prevent equivocation
by copied keys or an operator with filesystem/key control. It is also separate
from block-signing locks: cross-domain upgrade/block sequencing and operational
key custody are not supplied by this API. An upgrade vote does not stop a
validator from participating in the old protocol.

Synchronous engine writes and process-reopen checks are not a claim of tested
hardware power-loss behavior on every storage device or filesystem. Record
the actual interruption, process and durability tests separately.

## Read-only CLI verification

Paths are explicit and may be relative; no drive letter or common workspace
directory is required. Hash placeholders below mean canonical lowercase
32-byte hex without a `0x` prefix. Supply the certificate and authority as JSON:

```text
novovmctl native-nonce-migration verify-upgrade-authorization --bundle ./checkpoint/nonce-checkpoint-v1.bin --bundle-digest <bundle-digest> --chain-id <chain-id> --namespace-digest <namespace-digest> --legacy-protocol-commitment <legacy-protocol-commitment> --tip-block-hash <tip-block-hash> --snapshot-digest <snapshot-digest> --target-protocol-commitment <target-protocol-commitment> --authority ./authority/old-authority.json --expected-authority-commitment <authority-commitment> --certificate ./offline-upgrade/authorization.json
```

Before accessing input files, the CLI requires the explicit target commitment
to match this binary's current environment commitment. This configuration
comparison does not prove target-runtime compatibility or grant activation.
The command verifies inputs and emits a JSON report. Invalid pins, signatures,
subjects, quorums or files fail with a nonzero exit status and a JSON error. It
does not open signer, ledger or AOEM databases, write a workspace, sign a vote,
publish state, activate a protocol or restart services. Input limits apply to
encoded files and accepted validator/vote sets, not a hard process-memory or
runtime bound.

Certificate acceptance is reported separately as `quorum_verified = true`.
The immutable transition's `qc_verified`, `authority_state_published`,
`independent_provenance_verified`, `activation_ready`, `import_performed` and
`chain_canonical` remain false. Verification does not relabel the transition
or local ledger as proof-sealed, safe or finalized. It does not claim an AOEM
execution replay or AOEM execution-evidence verification.

## Required tests and gate

The new `test_native_nonce_upgrade_authorization` field starts false and
becomes true only after both explicit commands succeed:

```sh
cargo test -p novovm-node --lib native_nonce_upgrade_authorization -- --test-threads=1
cargo test -p novovmctl native_nonce_upgrade_authorization -- --test-threads=1
```

This slice introduced 48 required fields. The subsequent
[source prepare-QC slice](NOVOVM_NATIVE_NONCE_SOURCE_QC_V1.md) adds
`test_native_nonce_source_qc`. The later
[native seal new-view slice](NOVOVM_NATIVE_SEAL_NEW_VIEW_V1.md) adds
`test_native_seal_new_view`, extending the contract to 50 fields. The subsequent
[new-view candidate admission slice](NOVOVM_NATIVE_SEAL_NEW_VIEW_ADMISSION_V1.md)
adds `test_native_seal_new_view_admission`; producer, preflight and node-runtime
locksets now require 51 fields in the same order. The frozen contract rejects missing
or false authorization/source-QC/new-view/new-view-admission fields. Older evidence must be regenerated,
not relabeled. Source prepare-QC acceptance is still not finality or execution proof.

Required cases include independently pinned authority validation, exact
subject reconstruction, equal and unequal weighted thresholds, duplicate and
unknown voters, malformed or invalid signatures, block-vote substitution,
source/target/root/activation-height changes, persistent fencing before
signature release, exact-request recovery, conflicting reopen rejection,
corrupt stores and lock contention, and read-only JSON CLI success/failure.
These are acceptance requirements, not a record of tests already executed.

## Remaining publication boundary

A production upgrade still needs source canonicality and execution-evidence
acceptance, coordinated old-protocol retirement, and recoverable publication
across AOEM authoritative state, ledger pointers and protocol rules. That
integration must bind the accepted authorization and transition without
rewriting historical roots or treating an offline proposal as engine state.
There is no importer, activation switch, cross-owner commit protocol or
automatic live-node upgrade in this slice.

Local unit/CLI passes do not establish Linux CI, physical multi-device quorum
operation, public-network readiness, long-run stability, mainnet finality or
completion of legacy-chain migration. Running chat nodes are not restarted or
migrated by these offline APIs.

## Local validation record (2026-09-17)

Windows validation passed the full canonical mainline gate and its preflight:
48 of 48 required fields true, generated at
`2026-09-17T11:15:04.201010600+00:00`. The authorization filter passed 12 tests;
its dedicated ignored worker was invoked by the parent test in separate real
processes at both persistence barriers. The complete nonce filter passed 57
tests, with two dedicated workers invoked by their parent tests. The complete
`novovmctl` suite passed 22 unit and 6 real CLI-process integration tests.
Node/CLI all-targets Clippy with `-D warnings`, formatting and diff checks passed.

The isolated test-key certificate fixture is regenerated through three durable
signer stores and compared with the checked-in JSON; it is not a real validator
authorization or an AOEM execution proof. These results do not expand the
deployment or durability claims above. Local full-gate log:
`artifacts/audit/nonce-authorization-20260917/mainline-gate.log`.
