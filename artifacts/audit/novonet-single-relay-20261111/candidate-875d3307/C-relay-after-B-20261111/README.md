# C NOT READY — confirm operability before opening a new window

User coordination update: first confirm C can operate immediately, then start an
observer; A sends only after receiving a fresh real READY and current liveness.
This report is **not** C_RELAY_OBSERVER_ARMED. Do not send on this receipt.

## Actual attempt and read-only findings

- Synced 94d98656 for the attempt; later synced 5d90a002 (D public receipt).
- Previous exact C v2 observer PID6592 was absent, its exit0 / wait_expired and
  final inactive liveness were checked. No observer was terminated.
- One new v2 child PID1700 started 2026-11-11T14:49:35.7103702Z and exited
  14:49:36.2943880Z, exit1: `Original LocalService owner not confirmed`.
  It failed before READY/liveness/output-directory creation. No relay snapshots
  were produced. Wrapper stderr and actual exit are preserved byte-for-byte.
- Elevated read-only inspection 14:56:36.4212029Z–14:56:37.8824182Z found no
  novovm-node.exe process and no UDP41052/42052 listener. The original875 task
  is Ready (not Running), LOCAL SERVICE; last result3221225477 (0xC0000005).
  Its exit file independently contains -1073741819 (same 32-bit0xC0000005).
  This establishes termination, not the underlying crash cause.
- Original progress last write13:27:48.2904620Z (Beijing21:27:48); exit file
  last write13:27:58.2423939Z. These are filesystem times, not inferred exact
  crash times. Only metadata/hash and whitelisted exit fields are public.
- Ordinary task lookup initially could not see the protected task. The elevated
  result supersedes that lookup; no claim that the task was deleted.
- A's old documented sender wait expired about Beijing21:48:23, before this
  attempt at22:49. Current A sender availability is not independently verified.
  D's separate receipt does not make C ready.

## Actions and boundary

No second observer invocation, node restart/update, object send, config change,
key/database operation, or D operation was performed. Existing node/observer
failures remain unchanged. No full CI was manually triggered.

C requires a separately coordinated node recovery before another observer can
be opened. Do not reuse an old READY, assume recovery, or automatically retry.
After recovery is authorized and proved, confirm C operability first; then use
a new evidence root and send the actual fresh READY/liveness to A before A sends.

`SHA256.json` covers the public files. Raw private progress/config/key/database
contents are not included. Administrative inspector is read-only; its output
completed, but the launcher did not capture its process exit code. The actual
observer exit code1 is independently present in `observer-exit.json`.
