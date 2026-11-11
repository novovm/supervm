# 8fed A -> D original-node object — sender complete, receiver return pending

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

**Receiver result is pending return.** D must return its already-running
collector's result/error, receiver snapshots, actual exit code and stdout/stderr.
Do not rerun the collector or resend. Match object/session/nonce/identities,
actual native path, integrity/validity decision and transport custody evidence.
Sender DONE is bounded custody, not business receipt or an independent proof
that D's diagnostic validity check passed. No dual-end acceptance claimed yet.
