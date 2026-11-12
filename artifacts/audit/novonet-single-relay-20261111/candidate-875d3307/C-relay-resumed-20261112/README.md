# C recovered and relay observer ARMED, 2026-11-12

User explicitly prioritized continuing C/D integration with the crash still
open. Helper19daf8ff passed27 focused local checks, then one elevated invocation
matched original PID3228 / Application1000 record546354 / task and exit before
recovering the same installed875 owner. Actual recovery exit0 at01:37:34Z.

- Node PID13020, LocalService, original875 executable and AOEM hashes.
- Original database opened with existing, current_height1 / headnull,
  execution_failures0. Custody and configuration unchanged. No initialization.
- Prior logs moved intact to the recorded crash directory; dumps remain local.
- No pre-existing Watch-Relay process found; nothing terminated.
- Original v3 observer PID25144, actual READY01:37:34.7432861Z, wait7200s,
  latest copied heartbeat01:37:56.8121111Z / active / snapshots0.
- Nominal end03:37:34Z (Beijing11:37:34); original terminal/64-snapshot limits
  may end it earlier. No error/result/observer-exit at01:37:57Z snapshot.
- Neither C nor this wrapper sent an object or operated D. Recovery is not a
  crash fix, relay success or Business Receipt.

A: authenticate C and obtain a current D heartbeat/terminal snapshot before the
original one-shot A->C->D send. D's published00:41Z heartbeat is stale even if
its nominal7200s window may remain open. Do not send based on old READY alone.
Correlate the actual C forwarding and A/D object/session/DONE. Public push is not
A acceptance. C retains the same observer; no automatic retry after failure.

Raw public recovery/READY/launch outputs and hashes accompany this report.
Live stdout/stderr were copied using shared-read access after exclusive reads
were rejected; their final published bytes were re-read successfully, not
inferred from the absence of output. Task XML, raw crash XML, dumps, private
configuration and key materials are excluded. No manual Full CI requested.
