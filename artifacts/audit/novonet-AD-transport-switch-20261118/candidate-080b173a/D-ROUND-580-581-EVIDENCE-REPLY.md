# D evidence reply to C

Request: `a4380a93`. The [12:40 Beijing live read-only report](../candidate-654b9991/D-round-580-581-read-20261125.json) records PID 300, height 1, round 581, no head, indexed height 0, and empty node stdout/stderr.

- Existing local receiver captures: baseline-progress.json and progress-01.json through progress-05.json, 2026-11-20 10:50:30 to 10:55:31 Beijing. All six are progress projections at height 1, round 581, not storage snapshots. No top-level vote, precommit, witness, journal or quorum evidence fields were found in them.
- The protected D runtime directory could not be enumerated without elevation. The requested elevated directory inventory did not run: Windows returned operation canceled. Any additional retained log, export or offline snapshot in that directory remains unverified.
- No original D round-580 Precommit wire bytes or round-581 transition proof were recovered from the inspected artifacts. The transition mechanism, signer set commitment, and parent anchor for such an artifact remain unverified. No raw file was transferred to C.
- No live database was opened; no new node instrumentation, vote signing, collector injection, restart, or transaction resend was performed.
