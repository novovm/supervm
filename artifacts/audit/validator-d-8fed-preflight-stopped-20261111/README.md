# D 8fed update held at original-owner preflight

Status: `D_8FED_OLD_OWNER_EXIT_UNQUALIFIED_UPDATE_NOT_STARTED`.
Handoff: main `a3b24a2e`; observation 2026-11-11 10:40-10:41 Beijing.

The previously verified package and all eight tool hashes were rechecked using
the pinned tool manifest. Administrator PowerShell 7 ran the original
Test-Update-Windows.ps1 against the staged package: exit 0, all printed gates
and the fixed 77-artifact package gate passed. Updater SHA256:
`1abe055135ac1469bac56b91d813844befc14173e8461edbf61e003e2f641a98`.

The actual original D task `NOVOVM-D-1ef31893-Existing-20261110` is Ready,
LocalService, result 267014 (0x41306), zero restarts/triggers. No active or
kernel-signaled exited novovm-node process and no 41053/42053 listener were
found. Last task run: 2026-11-10T03:26:33Z.

Original progress remains valid JSON: chain_id=91129655606029,
current_height=1, head=null, execution_failures=0, recovery_in_progress=false;
last write 2026-11-11T00:00:08.9510573Z. exit.json is absent; stdout and stderr
are both zero bytes. Application event 1000 query since the old task start
returned no matching original executable crash and no query errors.
The reason for exit is not established by this evidence.

Per the latest handoff, the live-process entry was NOT called. Neither the
supplied clean-stop gate (requires task result 0) nor crash gate (requires
3221225477 and matching crash evidence) is satisfied. No recovery flags were
borrowed from C, no updater was retried, and no node, key, configuration,
firewall, clock or database was changed. C was not operated.

A: select the existing recovery entry based on this D-specific evidence;
the previous stop authorization remains in effect. This is not a request for
another stop approval. D cannot report D_8FED_EXISTING_READY_FOR_A_QUERY yet.
No result/after/old-exit update reports exist because the updater never ran.

The original public preflight, test output/exit, and exit-evidence collection
are preserved byte-for-byte alongside this report. No full configuration,
protected material, database, program or raw progress is published.
