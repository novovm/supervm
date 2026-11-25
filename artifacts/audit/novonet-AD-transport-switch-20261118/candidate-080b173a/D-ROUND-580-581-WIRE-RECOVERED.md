# D cold-copy wire evidence for C

Follow-up to `a4380a93` and `d058fc5e`. At 14:24:50 Beijing on 2026-11-25, D verified 20 files in a disposable copy of the `654b9991/database-cold-before` backup. The raw read-only scan inspected 1,843 visible default-column-family values. It did not inspect obsolete or deleted versions, other cold backups, or the live database. Recovery time is not the signing time.

| Height / round | Phase | Value | Wire bytes | SHA-256 |
| --- | --- | --- | ---: | --- |
| 1 / 580 | Prevote | nil | 3580 | `30b9ad3b79f288cadc7ef590d5ec63e8a204b03b4d34bcee9734c35bfae53b3b` |
| 1 / 580 | Precommit | nil | 3580 | `aa98d8c398b1b517f177712eb43e93f829ab8ea221b60dd9b5159d37699385cb` |
| 1 / 581 | Prevote | nil | 3580 | `e6457b0b75e3727829294180c88f3cb52b4732c34ff1021082c83984416022b4` |

Parsed chain ID: `91129655606029`. All three wire signer IDs match D in the existing validator mapping: `d90f5145f324790d6cef7b592d25747d0fb21958327eaeeb2367c800d74a8f90`. Parsed validator-set commitment: `a69cb7c7dbfe5b1a888940e6d742936f8507bce7a220ac77ae3afb2ef894b8cd`. The wire parent block and parent decision hashes are both zero. C must independently verify signatures and the full consensus context before counting any vote.

No matching encoded QC was recovered by this scan. The 581 Prevote alone does not establish a transition mechanism, timer completion or sufficient catch-up weight. The recovered 580 Precommit is nil, not a vote for an application block or proof of transaction finality.

Private pickup on D: `%LOCALAPPDATA%/NOVOVM/D-round-580-581-wire-for-C-20261125.zip`, 11,772 bytes, SHA-256 `7d491d42897f883cda3f0e6adcc6c7cf8e66c56b5210d30d55b0a99c261107d2`. Contains only three wire files and a scan summary. D has no configured validator-B SSH handoff and no removable drive mounted. Raw transfer to C is pending; no raw bytes are committed here.

No node restart, vote injection, new signatures or transaction resend.
