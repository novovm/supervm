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
