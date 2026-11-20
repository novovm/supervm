# First authorized resident NOV transfer — submitted, not finalized

On 2026-11-20 A used the original protected promotion controller key and
the pinned AOEM runtime to sign one native NUAS transaction for the
closed deployment (chain 91129655606029). The beneficiary was the
existing research-operations account. No private material was exported.

- Amount: 2 NOV. Signed maximum fee: 0.004 NOV (20 bps); actual
  charged fee is not yet known. Controller epoch 1, nonce 0.
- Transaction hash: `2d0436f0893d5a6e1e996470a2fd223b3b256a11a6240e39418078e9be9a5f3d`.
- Submitted exactly once via the original native client to A; it returned
  one row, `transport_delivery_only`, `finalized=false`, in 30 ms.
- Authenticated receipt queries at A returned `Received` with indexed
  height 0. A's progress showed current height 1, no finalized head,
  one pending input, zero native APFL execution rows and zero execution
  failures. At this snapshot round 580 named C as the scheduled proposer;
  this alone does not attribute the pre-finality blockage to C.

Follow the same hash only. Use read-only receipt/head and node progress
on A/B/C/D to distinguish gossip, proposer selection, and vote/QC
progress. Do not resubmit, regenerate, or claim a debit, final business
receipt or three-signature QC until independently observed. B was not
stopped and no previous transport tests were repeated.

## Four-host follow-up (2026-11-20 10:36 UTC)

All four authenticated queries for this exact hash returned `Received`,
indexed height 0. Two Status sweeps six seconds apart returned one pending
input, zero APFL execution rows and the same genesis state root on all four.
All were at current height 1 / finalized height 0, with rounds A=580,
B=580, C=294, D=581 in both sweeps. A's progress records one successfully
verified strict signature, zero executed APFL rows and zero durable
decisions. This rules out incomplete transaction propagation as the present
blocker; there is no verified proposal or QC for this transaction yet.

The current owner bounds ordinary future-vote admission to 64 rounds, while
C is 286–287 rounds behind the observed peers. The configured phase
timeout grows as 5 s plus 1 s per round (585 s at round 580); the short
sweep does not prove a permanently frozen timer. These are concrete
constraints to check on the pacemaker/catch-up path, not proof of a
specific rejected vote. Preserve the live journal and use signed-round
and timer evidence to locate the first failed stage; do not lower the
three-signature gate, clear rounds, restart B, or replay the transfer.

## 10:43 UTC read-only follow-up

Authenticated receipt queries on A/B/C/D all still returned `Received`,
indexed height 0. Status still showed rounds 580/580/294/581, one pending
input and zero APFL rows on each. A ticked from 36,399,734 to 36,904,159
between these captures. A's later progress had `executed_batches=1` but
`durable_decisions=0`; the aggregate batch counter is not tied to this
hash and does not prove this transfer executed.

Code review: the existing collector requires >1/3 distinct validator
weight **in one exact future round** for catch-up (two equal-weight
validators here). Ordinary vote admission applies the 64-round bound
before evidence is collected. Thus a round-580/581 vote cannot enter C's
round-294 collector by that path, even with a valid signature. This is
a code constraint for votes in those rounds, not proof C received such
a vote. The pacemaker checks acknowledged journal state and starts
phase waits from qualifying same-round vote weights. The present public
progress and empty A stdout/stderr do not expose phase timers, retained
vote weights or the reason for a rejected vote. Next capture those from
the existing owner without reopening or resetting the live journal, or
reproduce the observed round split in an isolated regression.

## Isolated collector reproduction

The new `resident_round_split_requires_admissible_exact_round_evidence`
test models the observed round numbers with four isolated fixture
validators and the resident collector bounds (64 future rounds, 28 vote
entries). Independently valid fixture votes at 580/581 are rejected by the
collector at 294. Two round-580 precommits plus one from 581 do not
qualify the round-580 phase timeout; the lone 581 vote is not a catch-up
witness. At round 516, two round-580 signers do form a catch-up witness at
the inclusive boundary, without finality or a state transition merely
from collecting the witness. All 22 collector tests passed locally.

This reproduces an admission and quorum constraint, not the exact
historical live vote set. It changes no runtime behavior. The next
implementation gate is a journal/pacemaker regression for authenticated
recovery across this gap, preserving existing signer, context, lock and
three-vote finality requirements. Do not treat a bound increase, unsigned
status, or an operator-chosen round as a recovery proof.

## Isolated journal and pacemaker regression

`resident_round_gap_verified_witness_is_ack_gated_and_not_finality`
reproduces the 294 → 580 gap on the existing journal and pacemaker, using
an in-memory ACK backend and four fixture identities. Ordinary admission
at round 294 rejects both round-580 votes and the pacemaker remains idle.
An independently verified two-signer, exact-round witness, assembled by
a separate fixture collector at round 516, advances the journal only
after the matching persistence ACK; reopening retains round 580. The
transition emits no signed message and records neither a decision nor a
new head. Two round-580 votes plus one round-581 vote do not form QC;
three round-580 nil precommits form a nil quorum, not block finality.

Windows locked release consensus library: 184 passed, including all 8
journal tests; rustfmt check passed. This exercises the existing consumer,
not a deployed wire route, a real disk ACK, AOEM execution or transaction
finality. The next missing step is a bounded authenticated intake for
exact-round evidence outside the ordinary 64-round vote window. No live
nodes were changed and the original transfer was not resubmitted.

## Bounded far-round witness admission (consensus owner)

The collector now admits a complete far-round witness against the exact
local context, with at most one signature per validator, a single future
round, and strictly over one-third of the configured validator weight.
Admission is atomic and reuses the existing pinned witness slot; invalid
or stale evidence does not replace it. Individual far votes still fail
the ordinary window check. The 294 → 580 regression now drives the real
collector → pacemaker → journal path, with the same simulated durable ACK
and unchanged finality rules. Windows locked release consensus library:
185 passed; strict Clippy passed.

No native message producer, decoder, or node dispatch is connected to
this admission yet. The next gate is qualified binary witness delivery
over the existing consensus channel, followed by an isolated node-level
verification. This is not a fix already active on the four hosts.

## Native far-round proof transport

The existing authenticated consensus control channel now carries a
dedicated binary witness (tag 23), with a checked signer count and exact
frame length before allocating the vote list. The channel verifies sorted
distinct signers, exact context/round, threshold and signatures; the
stateful collector reuses those verified votes, checks its exact live
context and ordinary span, and stages catch-up through the original
pacemaker and journal.

An existing exact-parent recovery request can receive a proof drawn from
the responder's currently acknowledged round. No proof is advertised
without the required signed weight. Replies share the original auxiliary
send budget and are paced per peer; they do not occupy the reserved
proposal/vote/decision outbox. No new requests, listener, or authorization
path is introduced. Old decoders do not understand tag 23. Neither
network DONE nor a witness is block finality. Existing transaction and
journal state are untouched; four-host recovery remains unverified.

Windows locked release: 185 consensus, 36 transport codec, and 17 native
channel tests passed; strict Clippy passed for both crates. The new
loopback UDP case assembles two current-round signatures, delivers the
same binary witness to real authenticated NativeChannels, and admits it
at round 294 without a QC or an immediate round change. The separate
journal regression exercises the ACK boundary. These do not prove a
live node's recovery-request lifecycle or the original transfer's receipt.

## Isolated native lifecycle recovery and reopen

`native_far_round_request_catches_up_and_reopens_existing_store`
uses the original four-channel loopback fixture and real AOEM stores in
an isolated temporary directory. Fixture signers establish acknowledged
rounds 294 and 580 using ordinary in-window votes. The test then polls
only the original active lifecycles; it does not construct or inject a
recovery request or far-round witness. It requires the lagging lifecycle
to reach round 580 with the same context, no decided head and no new
execution, then shuts down the isolated stores and reopens the lagging
one in `Existing` mode with its same identity. The reopened journal must
retain round 580 with no durable signed message or decided head.

Windows locked release result: 1/1 passed (test runtime 0.85 s), with
the real AOEM DLL SHA-256
`f9e8ded21753dfd32573de7bf66da8a785120444e810ba9fa865bfa4d639fcaa`,
matching the verified Windows candidate library. This is local UDP and a
clean isolated store shutdown/reopen, not process-kill or four-machine
acceptance. No resident process/database was touched, and the original
transfer was not resubmitted.

## Far-round candidate qualification

The frozen build source is `1b6808f6b93f43a8bd5d20a9da8e56759d75723d`.
Its only change from the isolated-test source `b93655e3` is the
NOVOCHAT handoff document. [Full #44735067787](https://github.com/novovm/supervm/actions/runs/44735067787)
was dispatched once and its actual head SHA was verified.

The [Rust security job](https://github.com/novovm/supervm/actions/runs/44735067787/job/136284530809)
failed while installing host tools: the unpinned resolution selected
`cargo-deny 0.22.0`, which requires Rust 1.95 or newer, under the
project's pinned Rust 1.94. Cargo Audit and Cargo Deny were both skipped
by that job; neither can be treated as passing. The CI-only repair pins
`cargo-audit 0.23.0` and `cargo-deny 0.22.0` and uses the job's already
installed stable toolchain only to install those host tools. Product
Rust, dependency locks and all four security checks remain unchanged.
The original Full is not an eligible deployment gate.

Local install with stable succeeded for both pinned tools. With the
project toolchain still active, root and runtime `cargo audit` and
`cargo deny check advisories bans licenses sources` all exited zero;
existing allowed warnings remain. This is local evidence, not a
replacement for the failed GitHub job.

Both frozen archives are in
`C:\Users\leadb\AppData\Local\NOVOVM\candidate-common-1b6808f6-20261120`.
Neither may be installed on the basis of this record.

| Archive | Bytes | SHA-256 | Archive files / manifest members |
| --- | ---: | --- | ---: |
| `novovm-common-1b6808f6-windows-x86_64.zip` | 162907197 | `89d8264fbfc855d291af9d8ca56748c63169c28ea4571eb38230bcd3e03ec22f` | 94 / 91 |
| `novovm-common-1b6808f6-linux-x86_64.tar.gz` | 376080450 | `9d10e7800570714a7af543e5383831829f8924254492c7c92cab5b6ba965d767` | 95 / 92 |

- Windows node: `9d145d6e3a0f58e6fa0bea90a2815c53713c7e299d9483cc88a6afb3a92d88a5`
  (AOEM `f9e8ded21753dfd32573de7bf66da8a785120444e810ba9fa865bfa4d639fcaa`,
  manifest `ca76f253968cfafb1913e5d30eed67e20e298ae1b308e792a73bccb067de21c2`).
- Linux node: `5e5cefa16f47fbabe9fb8b9b9caa07039588b1518b3309528159a24836f67e64`
  (AOEM `21cf190ede73c2dc3040cf5ca51502a7bf56d1aeffbf7baeb31afdd5f71d0231`,
  manifest `4fb0eda25a8684432de11ab3e8311dc4b3ee8cc07020a7260864cdb5f2479461`).

Every archive file matched the published package tree, both manifest
checksum lists passed, and the packaged ML-DSA runtime qualification
passed on Windows and Linux/WSL. No packaged-node cold reopen, resident
update, or further Full was performed. The archive manifests continue
to identify the actual build source, not the later CI-only repair.
