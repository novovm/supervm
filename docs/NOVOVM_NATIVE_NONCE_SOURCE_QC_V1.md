# NOVOVM Native Nonce Source Prepare-QC Verification V1

Status: bounded, read-only verification of the source history's prepare
proposals and quorum certificates. This is not source finality, an execution
proof, upgrade authorization, state import or protocol activation.

## Purpose and trust boundary

The [checkpoint bundle](NOVOVM_NATIVE_NONCE_CHECKPOINT_BUNDLE_V1.md) checks a
complete legacy snapshot and its local block commitments. The separate
[upgrade authorization](NOVOVM_NATIVE_NONCE_UPGRADE_AUTHORIZATION_V1.md)
records validator consent to a deterministic nonce-rule transition. Neither
one establishes that the old block history received consensus prepare votes.
This verifier closes that specific gap without promoting prepare votes into
finality or treating validator consent as proof of execution.

Inputs are explicit: the source bundle, expected bundle digest, checkpoint
anchors, old epoch authority, independently established expected authority
commitment, and the source proposal/QC history. The verifier validates the
authority and compares it with the independent pin before trusting its
validator set. Taking that pin from the same untrusted authority document
only establishes self-consistency, not authorization. Bundle and checkpoint
pins likewise require independent establishment.

The supported old authority remains epoch 1, with its validator set active
from height 1. This slice does not add validator-set rotation, new epoch
admission or governance membership. Chain ID, genesis, legacy protocol and
validator authority must match the supplied, independently pinned source.
Namespace and snapshot anchors remain part of checkpoint verification.

## Complete source-history verification

Verification requires the complete ordered sequence from height 1 through
the source tip, with one proposal and one matching prepare QC for each
source block. A valid tip certificate alone does not replace missing source
history. The verifier:

- Revalidates the entire bundle and checkpoint, including raw transaction
  authentication, nonce reconstruction, state/receipt commitments and parent
  continuity.
- Reconstructs each expected seal subject from the verified source block,
  not from the supplied proposal's asserted roots or identifiers.
- Requires exact block, body/DA, receipt, AOEM-parent, execution-context,
  protocol and execution-evidence commitment bindings.
- Verifies the proposal signature and the authority's scheduled leader for
  that height. This offline V1 profile admits round 0 only.
- Verifies every included vote, uniqueness, membership and signed weight.
  Quorum requires strictly more than two thirds of total validator weight;
  for four equal-weight validators three signatures suffice, while other
  distributions must be judged by their signed weight.
- Requires the genesis proposal to have no parent QC and every later
  proposal to justify the immediately preceding verified source QC.

Missing, reordered, duplicated or extra entries reject. A certificate from
another chain, genesis, epoch, validator set, protocol, round or source block
does not satisfy this profile, even if it has internally valid signatures.
Invalid or redundant votes are not silently discarded to manufacture a
quorum. An upgrade-authorization certificate is not a source block QC.

Round 0 is an intentional admission limit, not a claim that higher-round
consensus is invalid in general. This verifier does not consume timeout
observations/certificates, advance rounds, change leaders or modify the
existing timeout-signing and persistence work. Future round admission needs
its own explicit protocol and evidence support.

## What a successful result means

`prepare_qc_chain_verified = true` means that the independently pinned old
authority supplied sufficient valid prepare votes for every exact source
block in the supported history profile. The source ledger remains a local,
unsealed candidate history; no historical header or lifecycle flag changes.

Prepare is not a commit/finality phase. Success does not establish source
finality, fork-choice selection, chain canonicality, safety, proof sealing or
upgrade activation. It does not retire the old protocol or grant permission
to publish a new authoritative state. The existing offline upgrade artifact
and its authorization certificate retain their separate meanings.

In particular, `aoem_evidence_commitment`, `canonical_inclusion_proof` and
`durable_ledger_close_proof` are not independently verifiable NOV business
execution proofs merely because they are bound into a signed subject. The
current close/inclusion values are deterministic commitments. This verifier
checks their block binding, not an execution circuit, AOEM owner readback or
historical execution replay. It must not report AOEM execution evidence or
execution correctness as verified.

## Read-only CLI

Paths may be relative and do not require a shared drive letter or workspace
directory. Hash placeholders mean canonical lowercase 32-byte hex without
a `0x` prefix.

```text
novovmctl native-nonce-migration verify-source-qc --bundle ./checkpoint/nonce-checkpoint-v1.bin --bundle-digest <bundle-digest> --chain-id <chain-id> --namespace-digest <namespace-digest> --legacy-protocol-commitment <legacy-protocol-commitment> --tip-block-hash <tip-block-hash> --snapshot-digest <snapshot-digest> --authority ./authority/old-authority.json --expected-authority-commitment <authority-commitment> --source-qc ./checkpoint/source-prepare-qc.json
```

There is no target-protocol argument: the command verifies old source
history, not a proposed upgrade destination or the running binary's current
protocol configuration. It emits a verification report on success and a
nonzero exit status on invalid input. It does not open live ledger, signer
or AOEM databases, write a staging workspace, sign or send votes, import
state, activate a protocol, update authority pointers or restart services.
This slice provides no source-QC exporter and no signing CLI.

Source-QC JSON has a 64 MiB encoded-input limit, in addition to the existing
bundle/history, authority and validator limits. The input limit is not a
hard RSS or runtime limit: JSON decoding and verification can allocate more
memory than the encoded file. Unknown fields, duplicate fields and malformed
or trailing JSON must be rejected rather than accepted as future protocol
extensions. The CLI does not fetch missing artifacts or manufacture them.

## Required gate and tests

`test_native_nonce_source_qc` starts false and becomes true only after both
commands succeed:

```sh
cargo test -p novovm-node --lib native_nonce_source_qc -- --test-threads=1
cargo test -p novovmctl native_nonce_source_qc -- --test-threads=1
```

This slice introduced 49 required fields. The subsequent
[native seal new-view slice](NOVOVM_NATIVE_SEAL_NEW_VIEW_V1.md) adds
`test_native_seal_new_view`; producer, preflight and node-runtime contracts
now require 50 fields in the same frozen order. Missing or false source-QC
or new-view fields reject. Older gate artifacts must be regenerated, not relabeled.

Required tests cover a complete equal-weight 3-of-4 source history, weighted
quorum accounting, insufficient/duplicate/unknown voters, bad signatures,
missing parents, wrong justify links, proposal/QC disagreement, wrong leader
or round, mismatched authority/pins/roots, incomplete or reordered histories,
input bounds, strict decoding and real-process read-only CLI success/failure.
Success must leave finality, execution-proof, activation and import claims
false. Synthetic legacy fixtures contain test commitments and test keys;
passing their QC checks is not evidence of historical AOEM execution.

These are acceptance requirements, not a record of tests already executed.
Local unit/CLI results must be recorded separately from GitHub CI, physical
multi-device quorum operation, public-network behavior, Linux installation,
long-run stability and mainnet readiness. No AOEM repository, ABI, opcode or
DLL change is required; all source-chain policy remains in SUPERVM Host code.

## Remaining work

Upgrade publication still requires an explicit source finality protocol and
execution-evidence policy, coordinated old-protocol retirement, and a
recoverable switch across AOEM authoritative state, ledger pointers and
protocol rules. Completing this prepare-QC verifier does not remove those
requirements or activate existing chat nodes.

## Local validation record (2026-09-20)

On Windows, this slice on top of `b11d10f` passed the full canonical mainline
gate and preflight: 49 of 49 required fields true, generated at
`2026-09-19T18:49:34.700304300+00:00` (September 20 local time). The full nonce
filter passed 67 tests; two dedicated ignored workers were actually launched
by their parent tests. Source-QC tests passed 10, seal/overlay/timeout/round
tests passed 25, and the complete CLI suite passed 24 unit plus 8 real-process
integration tests. Node/CLI all-targets Clippy, formatting and diff checks passed.

The checked-in source certificate fixture is rebuilt through persistent test
signer stores. Strong negative tests first construct cryptographically valid
QC artifacts, then reject wrong source state roots, leader or round, a different
valid QC for the same parent block, and insufficient weight despite three
genuine signatures. None of these test results grants source finality or
independent execution proof. Full-gate log:
`artifacts/audit/nonce-source-qc-20260920/mainline-gate.log`.

After that full-gate run, concurrent network-only commit `e8ee744` was
fast-forwarded without conflict. Post-integration checks passed 35 network
Overlay tests, 23 node Product-mainline tests, 10 source-QC tests and all-targets
Clippy for network/node/CLI. The full 49-field run above is recorded against
its actual pre-network-merge baseline; these additional checks are separate.
