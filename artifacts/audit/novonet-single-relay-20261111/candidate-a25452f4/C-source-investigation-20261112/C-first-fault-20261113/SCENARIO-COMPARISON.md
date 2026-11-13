# Repeated C crashes versus the short PageHeap run

The user's correction is accepted: **the C node failure has occurred repeatedly**
and the requested relay-observation workflow has repeatedly failed to complete.
The30-minute PageHeap result means only that this particular diagnostic run did
not capture another fault. It must not be called an unreproduced overall issue,
a resolved crash, or evidence that the original workflow is healthy.

## Original chronology (Beijing time)

| Original evidence | Start / event | Result |
|---|---|---|
| a254 PID27928 / original task | Task start2026-11-12 16:34:17; Application1000 crash17:50:53.835 | `c0000005`, about77minutes after task start |
| Screenshot's C observer attempt | Preflight2026-11-12 22:46:01 | Node/listeners already absent; stopped before invoking Watch-Relay |
| a254 + AOEM966 PID37796 | Controlled start2026-11-13 08:41:00.992; crash10:15:52.801 | `c0000005`, about95minutes after start |
| Associated bounded observer | Window ended2026-11-13 09:42:07.646 | Node crash followed about34minutes later |
| PageHeap PID15012 | 2026-11-13 11:44:44.272 to12:14:52.197 | Target exit0,30minutes8seconds; no first-fault marker |

Originals: [screenshot-attempt preflight](../../C-double-relay-ready-20261112/node-before-public.json),
[attempt explanation](../../C-double-relay-ready-20261112/README.md),
[AOEM966 controlled start](../C-AOEM966-controlled-20261113/backtrace-start-public.json),
[later original crash event](../C-AOEM966-controlled-20261113/backtrace-exit-37796/public-exit-supplement.json),
[observer terminal](../C-AOEM966-controlled-20261113/backtrace-exit-37796/observer-result.json),
[PageHeap terminal](attempt-02-public/terminal-public.json).

These two crash latencies demonstrate that the short PageHeap run did not even
cover the observed later-failure intervals. The explicit original Watch-Relay
loop also was not run in that diagnostic. Changed heap instrumentation changes
allocation layout/timing. Its negative result is therefore limited.

## What the actual scripts and node do

The original [Watch-Relay](../../../Watch-Relay.ps1) invokes its adjacent
[Read-RelayProgress](../../../Read-RelayProgress.ps1), reads the node's
progress JSON and writes observer evidence once per second. It does not call
the node's status RPC, send an object, launch a node or write the database.
The matching node itself periodically executes `product.status()` ->
`profile::snapshot()` -> JSON encode -> progress-file write, even when no
external observer is reading. The original crash stack reaches that internal
status/profile/hex/String-copy path. The first invalid writer remains unknown.

Consequently, repeated crashes are established; **Watch-Relay as their direct
trigger is not established**. One documented crash predates its attempted
launch, and another occurred after the associated observer ended. Neither
point clears the overall diagnostic/status workflow or excludes timing effects.

## Next reproduction target; not executed by this correction

Keep C responsible for the investigation. The next controlled capture should
combine the corrected v3 full-dump gate with the actual original read loop and
a bounded duration covering the original7200-second observation, preserving
the original DB, identity and old progress/log evidence before any path reuse.
Account explicitly for the current diagnostic progress path versus the original
Watch-Relay fixed path; do not read a stale old file or invent a READY result.
An observer used for fault reproduction is diagnostic-only and must not release
A to send a relay object. No A/B/D operation is required for this comparison.

This correction runs no node, observer, object send or automatic retry. It does
not restore double relay. The prior terminal result remains unchanged:
`root_cause_fixed=false`, `relay_ready=false`.
