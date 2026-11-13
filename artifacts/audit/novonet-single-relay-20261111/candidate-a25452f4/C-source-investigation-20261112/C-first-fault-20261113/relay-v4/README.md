# C original observer + corrected PageHeap capture v4

Status at preparation: **not started, Windows UAC pending**. This is a new,
single bounded diagnostic of the already repeatedly observed crash. It does
not restore double relay or publish a network-service READY.

The user explicitly asked to continue after the short30-minute run missed the
prior77/95-minute crash intervals. This version uses the original unmodified
Watch-Relay.ps1 and Read-RelayProgress.ps1, original source/hash/work parameters,
and MaximumWaitSeconds7200. The diagnostic node writes the observer's original
progress path; its previous contents are archived locally and hash-compared
before reuse. The original config, DB, identities and previous diagnostic
evidence remain. The private diagnostic owner uses `existing`, original a254
node and AOEM966. No A/B/D node or phone operation is included.

## Actual preparation versus required live gates

21 preparation checks passed: both PowerShell ASTs, fixed bounds/path/owner
contracts, original observer/reader hashes and the real full-dump validator
against the previously captured isolated dump. These are not node acceptance.
The fresh LocalService run must still pass clean PageHeap control, deliberate
one-byte HeapAlloc overrun, CDB dump-completion output, MDMP signature and
full-memory flag before launching the node. V4 uses forward-slash CDB dump
paths and preserves a fault-observed record even if dump validation fails.

## Bounds and termination

- Original observer window7200seconds, up to64original snapshots.
- Node runtime7320000ms (122minutes) to cover observer startup/window plus
  shutdown margin. CDB hard bound7440seconds (124minutes). LocalService task
  maximum128minutes, elevated coordinator maximum127minutes.
- Target private memory6GiB; stop below10GiB free on D. A bound stop is inconclusive.
- First AV/verifier stop: original context/stack and full dump, then exit;
  no retry or replacement key/DB. Actual target exit is separate from CDB exit.
- Prior node/debugger/listeners/observer must be absent. Previous diagnostic
  terminal/owner state must agree. Existing IFEO settings cause preflight stop.
- Only this run's PageHeap entries are removed, after verified node startup
  and in cleanup. No system-wide heap, VPN, routing or firewall change.
- The original observer runs hidden as an elevated read-only process. Its
  creation time is captured via the same WMI precision used for final identity
  checks. Only that matching PID/path/creation/command may be stopped when
  this diagnostic ends or fails. No other observer or node is stopped.

## Evidence and coordination

Local paths are `target/c-first-fault-20261113/run-private-03` and
`launch-public-03`; task `NOVOVM-C-FirstFault-Relay-20261113-03`. Fresh roots and
task guards prevent reuse/retry. Dump/log/config/prior progress and raw original
observer records stay under restricted local ACLs, never Git.

The original observer's own READY is retained privately. Public status is
`C_FAULT_DIAGNOSTIC_RUNNING`, **relay_ready=false, A_must_not_send=true**.
It is not C_RELAY_OBSERVER_ARMED for network acceptance. No object is sent by
this procedure. After UAC, actual control/startup/terminal evidence must update
this preparation record; UAC or a successful launcher is not node health.

An exception follows the original call chain; a clean two-hour run still does
not establish a fix. The known original crashes and earlier capture limitations
remain preserved. No production timeout or network policy is changed.
