# A→D Iroh fallback: session-relative time evidence

The 1281 retry delivered all fragments intact to D. A recorded transport
DONE; D recorded `reference_warming_up` followed by `reference_wait_elapsed`,
not `validity_window_rejected` or receiver acceptance. D's
[time snapshot](candidate-1281d8b8/D-receiver-result/protocol-time-A.json)
had `relative_ready=false` on that session. Its old aggregate counters and
single in-flight snapshot cannot identify where that session's measurements
stalled. A different session becoming ready later does not close this gap.

The existing lifecycle progress projection now exposes `session_flow` per
peer, with separate request and response counters. These reset on a session
change. Old completions still return custody to the original owner but are
not counted against the new session. The existing cumulative counts and
in-flight field are unchanged; `session_inflight` excludes stale handles.
The snapshot is fixed-size per configured peer and uses the existing report
serializer; it adds no network messages, payload logging, or private material.

Read the counters in order on the matching session at both ends:

- `queue_attempted` / `queued`: submission to the original bounded node
  queue, not a send; `expired_before_admission` is a local queue removal.
- `admitted`: the existing object sender accepted custody, not proof of
  emission or remote receipt. `transport_done` is link-level DONE; other
  returned states are tracked separately.
- `received`: the authenticated object reached the time handler with a
  session context. `accepted` / `rejected` track request validation or
  response sampling. Rate-limited requests are counted separately.
- `relative_ready`: time reference maturity, not probe acceptance. Read the
  same object's receiver validity and integrity result separately.

Windows strict Clippy with `iroh-transport` and three targeted tests passed:
session partition/stale-flight exclusion, expiry before I/O, and preserving
the queued Carrier probe's original expiry and time generation. The last
two ran the local original lifecycle with the pinned AOEM DLL, not live
failover. No new Full run or live acceptance was performed.

No time-policy change or live deployment is implied. If a new instrumented
candidate is qualified, collect A and D deltas for the same newly
authenticated Iroh session before and across a fresh probe. Instrumenting
only D can show its local stages but cannot certify A's unseen stages. Do not
reuse the rejected object's identity, stop B, or repeat the independently
passed NovoRUDP return.
