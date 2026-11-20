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
