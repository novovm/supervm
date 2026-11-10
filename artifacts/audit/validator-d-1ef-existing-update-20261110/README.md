# D 1ef31893 original-database existing update

Status: `D_1EF_EXISTING_READY_FOR_A_QUERY`.

Executed only on validator-D against handoff commit
`36ded3b2779c8520e428106d88fe6c20b8853b1e`, following the latest first section of
`artifacts/audit/novonet-seed-retention-20261110/candidate-1ef31893/CD-UPDATE.md`.
C was not accessed or restarted. No old initialization task was run.

## Fixed inputs and execution

- Windows ZIP already present on D: 150528007 bytes, SHA256
  `a374d1dc47eb209999e65b2066a39a01e96bb1d2e8d459fbff069e1df6fa5b8c`.
- Tools ZIP reused from the synchronized repository (not present in E: inbox):
  17960 bytes, SHA256
  `22490a914d20760605ad8611eb4193e13295fb3c7a9201ae0e805248242dabf0`.
- Both were independently extracted into fresh directories under
  `target/d-1ef-existing-update-20261110`; no rebuilding or archive substitution.
- Administrator PowerShell 7 ran the unmodified packaged `Test-Update-CD.ps1`
  against the extracted package: exit 0, all printed gates passed, including
  the fixed manifest/source and 77 artifact checks. Fixture tests mentioning
  other host IDs did not operate those machines.
- D preflight found the original 082 task Ready, LocalService, exit 0, no
  triggers/restarts, no active node and no bound 41053/42053 ports. Original
  database exists; valid prior progress has current_height=1, head=null,
  execution_failures=0, recovery_in_progress=false. Original exit.json is
  absent; qualification uses the supplied clean-stop guard, not an invented
  exit file or a new graceful-drain qualification.
- Ran the original `Update-CD.ps1` exactly once with `-HostId D -ResumeStopped`,
  the verified PackageRoot and a fresh `run1` EvidenceRoot. Exit 0 at
  2026-11-10 11:26:36 Beijing. No RecoverMissingProgress, RecoverCrashed,
  ResumePrepared, Inspector, old initialize task or database cleanup.

## Result and A handoff

Program source: `1ef3189307382ef579775c7ce2b55a08d829d04a`.
Node SHA256: `d419df614d6b804136eca82fd7427d84155241c82b7f74c3c00bf2fdb3ea6087`.
Actually loaded AOEM SHA256:
`1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50`.

New PID 2068, SID S-1-5-19; task
`NOVOVM-D-1ef31893-Existing-20261110`, original database
`C:\ProgramData\NOVOVM\selfrun\91129655606029\D\chain-state`, START=existing.
Original custody material hashes unchanged, initialize_executed=false,
keys_generated=false. Firewall filters copied exactly to the new program;
original rule retained, no profile or scope expansion.

At 11:28:00 Beijing the same PID/task remained Running with fresh progress:
current_height=1, head=null, execution_failures=0, recovery_in_progress=false,
projection_error=null. Task result 267009 means running, not a final exit code.
The original updater observed UDP 192.168.71.59:41053 and :42053 owned by 2068.
Leave D running for A's authenticated query at `192.168.71.59:41053`, using the
existing D identity and domain 91129655606029. No new probe command was issued.
The supplied owner retains its original 86400000 ms run duration; this report
does not promise an indefinitely running service.

`result.json`: update_completed=true, previous_state_available=true,
progress_state_unchanged=true, custody_unchanged=true.
These are local updater checks, NOT an external authenticated state-root
comparison. A query, actual object delivery, relay/recovery qualification,
four-host DONE, consensus, business receipts and performance remain unclaimed.
root_cause_resolved=false and business_receipt=false are preserved verbatim.

## Public evidence

`original-public-results.zip` preserves the exact bytes of the whitelisted
result/after/old-exit/firewall, clean-stop proof, preflight summary, tests and
exit reports. `evidence-manifest.json` binds every entry and the archive.
No before.json, run/network config, database, protected material, DLL, EXE or
Windows/tools package is uploaded.

An additional read-only observation first failed when attempting to hash the
running node's locked stdout. The captured failure/exit 1 is retained; it was
not a node or updater failure. No process was stopped or update repeated.
The subsequent metadata-only observation exited 0. stdout/stderr were each
0 bytes at observation, but their contents/hashes were not read or certified;
the running node has no exit.json yet. The first uncaptured read-only attempt
also produced no observation file; only the later captured attempt has a
recorded exit code. These limitations do not overwrite the original success.
