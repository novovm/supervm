# 8fed A -> D original-node object — dual-end verification PASS

## Final matching result (2026-11-11)

A pulled D receipt **d68074f7** and verified the 310,817-byte archive SHA256
`4d78a2dc0481a1e1a1c3c029e379f9a53cd59f3f03b8d60882472e55e67a0837`
and all **13 original members**. `Verify-Pair.ps1` rechecks both original archives
and generates [dual-end-match.json](dual-end-match.json), without node/network IO.

| Evidence | Matched result |
| --- | --- |
| Object / session / nonce / sender / recipient | Exact match in all seven D snapshots |
| Signed diagnostic lifetime | Original issued/expires unchanged on both sides |
| Actual path | A -> `192.168.71.59:41053`; D observes `192.168.71.74:41051`; native_udp |
| A transport | One initial send, ACK coverage=1, terminal delivered and Object DONE |
| D transport | One fragment/one assembly, completed fragment set, hash verified, matching feedback epoch |
| D diagnostic acceptance | `probe_pending_time_boundary` then `probe_received_integrity_verified` |
| D time reference | Same authenticated session, relative_ready=true |
| Duplication | One unique verified event; seven snapshots are repeated observations, not seven deliveries |
| Collector | Automatic completion 11:13:39 Beijing, exit0, empty stderr, no timeout |

**This one A-to-D real native UDP object passed dual-end verification.** Receiver
acceptance and sender DONE are distinct events and now match. The receiver-side
transport_done_us is null (the sender terminal field); it is not fabricated as a
second local DONE. Receiver fragment/hash/feedback evidence and its integrity
acceptance are preserved instead. No Business Receipt, asset execution, consensus,
relay/multihop, all-device or sustained performance acceptance is implied.

The time-boundary path participated successfully without changing clocks or the
original 10-second object validity. This is not a controlled clock-skew/jump or
malicious-time test and does not close all ATC security scenarios or the prior C
failure. Session-relative time is not trusted UTC. No resend or restart occurred.
`sender-public.json` retains its original pending-at-capture status; this matched
result supersedes that historical status without rewriting the original evidence.

## Original sender phase (receiver had not yet returned)

A pulled D readiness `0ce2733e`: collector PID21680 started 11:10:37 Beijing,
original D node PID19756, bounded 30-minute observation. A invoked the pinned
installed `novovmctl native-network-probe prepare` exactly once through its
original 8fed LocalService owner, PID13996, at **11:13:33 Beijing**.
No new socket, alternate network, restart, initialization, clock change or
asset operation. The original empty command slot was populated, not overwritten.

| Identifier | Actual value |
| --- | --- |
| Object ID | `a89c1dac10c0405ca4d153bb03b03279d6339cd4d465b7aa81352b1902dcafd4` |
| Session ID | `d748367f3ba5e381bc8468467db43fb0` |
| Nonce | `c1850dd49d79e5e33a241e12acbc759b9483a9abbbefdc76982ea2e56116d26d` |
| Actual route | `native_udp`, `192.168.71.59:41053` |
| Sender result | `probe_owned` -> `probe_admitted` -> `probe_object_done` |

Matching original transport record: one initial packet, no repair, one observed
send attempt, acknowledged_best=1, send_terminal_state=delivered, transport_done
observed. Sender monotonic admission-to-DONE is 4,998us for this single object;
this is not a cross-device one-way latency, TPS or sustained performance claim.
Session-relative D time reference was ready (98 accepted/0 rejected samples).
That observation does not prove trusted UTC or receiver validity acceptance.

The 94,002-byte `A-original-evidence.zip` preserves original before/after public
diagnostics, command bytes/JSON, runner exit/log and executed scripts. Original
member digests are in `A-original-members.json`; archive digest and matched
transport/time projection are in `sender-public.json`. No protected configuration,
keys, database or journal is published. Runner exited 0; execution_failures=0.

**Historical state at sender capture: receiver result was pending return.** D was to return its already-running
collector's result/error, receiver snapshots, actual exit code and stdout/stderr.
Do not rerun the collector or resend. Match object/session/nonce/identities,
actual native path, integrity/validity decision and transport custody evidence.
Sender DONE is bounded custody, not business receipt or an independent proof
that D's diagnostic validity check passed. No dual-end acceptance claimed yet.
