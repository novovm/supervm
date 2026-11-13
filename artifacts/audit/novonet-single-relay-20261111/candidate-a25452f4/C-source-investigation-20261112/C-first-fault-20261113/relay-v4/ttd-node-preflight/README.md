# C instruction recorder: preserve LocalService and stop before node on failed control

2026-11-13 19:05 Beijing: the bounded original-node preparation stopped before
node authorization. LocalService TTD returned 0x80070005 / -2147024891:
Administrative privileges are required to record program execution. No trace,
node, original observer or database open was attempted. Coordinator exit 1.
The original package/runtime hashes and existing-only intent are in preflight.
This is a diagnostic harness failure, not a node crash or negative reproduction.

## Original privilege model

Microsoft documents that launch inherits the elevated recorder's privileges;
attach/monitor preserve the target's normal privileges. C must remain LocalService.
The next control therefore uses an administrator recorder attached to a suspended
LocalService **isolated fixture**, not an elevated NOVOVM node or an admin grant
to LocalService. No monitor driver or PageHeap is installed.

Reference: https://learn.microsoft.com/en-us/windows-hardware/drivers/debuggercmds/time-travel-debugging-ttd-exe-command-line-util

## Actual failed controls and cleanup correction

At 19:10 the initial suspended fixture started but its WMI image-path check failed.
Cleanup also failed because PowerShell's 0xe0000001 literal is signed while the
native TerminateProcess argument is unsigned. The fixture stayed suspended;
no NOVOVM process was started. This cleanup defect is not hidden by task exit 1.
At 19:12 the corrected entry refused cleanup when the same WMI identity check
failed. The old fixture and both failed reports were preserved.

The subsequent entry reads image path, creation time and token SID through kernel
APIs, checks the pinned original fixture and parent, then terminates only that
exact fixture. New suspended children retain kernel handles for exit verification
and cleanup. The exit code is the explicit positive 3758096385 uint value. A local
isolated check verified exact creation time and handle-based termination; it is
not evidence that the administrator attach or node recording succeeded.

At 19:14 the V3 control completed with coordinator exit 0. Both targets were
verified through kernel image/time/token facts as LocalService. Recorder exits
were 0; actual target exits were 0 and 0xc0000005. Replay recovered the expected
writer RVA 0x2b37 and verified valid-pointer -> 0xb8. The old PID28544 was exactly
identified and its exit confirmed. Both new controls exited; task removed.
See attach-control-03. This proves the attach method, not a node crash fix.
The scripts are preserved for review and have fixed fresh evidence directories:
**do not rerun them blindly**. Raw traces and private logs stay on C. The node
remains NOT READY, first corrupting writer unknown, double relay blocked.

## Planned node boundary, not completed

The LocalService attach control has recorded and reverse-replayed a known
writer. The prepared node entry can now let the original node open its existing DB once, using original keys and
runtime. Proposed maximum is 7200 seconds with a 2048 MiB ring trace (retained tail
only; not full-run coverage). Any failed gate stops before node start; all failures
remain evidence. No object sending, installation, reinitialization or fee changes.
