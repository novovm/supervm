# A→D Iroh fallback — transport passed, receiver acceptance failed

Both nodes ran `63b0ef4a` with their original databases. This was one new
object on an authenticated bound Iroh carrier. The temporary A/D native UDP
rules were removed; B was untouched. No old object was replayed and no
second object was sent for NovoRUDP return.

- Object: `fcdc488891fd82f9c6288e26ee07e6a499ee372759cda23874beeb823a3bc996`
- Session: `04735632aa719ffe2ec2489ee6861e3b`
- [A sender proof](A-fallback/iroh-fallback-proof.json)
  records sender DONE at `09:14:03.800 UTC`.
- [D receiver projection](D-receiver-result/D-receiver-result.json)
  matches object, nonce, session, actual carrier, carrier binding and
  independently recomputed transfer tag in all four snapshots. Both D
  export hashes matched their [pins](D-receiver-result/SHA256SUMS).
  D received 1/1 fragments, hashed one chunk and recorded verification.
- D recorded `reference_warming_up` at `09:14:03.844 UTC` and
  `reference_wait_elapsed` at `09:14:05.844 UTC`. No receiver acceptance
  event was present. This is not `validity_window_rejected`, fragment loss
  or a failed Carrier authentication.

[A's target-session projection](A-fallback/A-protocol-time-D.json) first
sampled at `09:14:04.152 UTC`: one accepted response and not ready.
[D's target-session projection](D-receiver-result/D-protocol-time-A.json)
was published at `09:14:08.480 UTC`: three accepted responses,
`relative_ready=true`, zero response rejections and zero time-transport
expiry/cancellation. D had received four time requests and rate-limited
one; A's earlier projection also records one rate-limited request. These
are per-session snapshots, not timestamps for individual samples. They
show successful convergence on the same Iroh session after the object had
already been rejected, but do not locate the third response within the
wait window or prove that rate limiting caused the rejection.

## Local follow-up: keep a slightly early reply until its send turn

The original sender spaces challenges from the issue time, while the
receiver previously dropped requests arriving before its reply pacing
boundary. A small decrease in path delay can therefore drop a legal
challenge. Its requester can remain waiting for that response until the
existing measurement deadline. The captured rate-limit events are
consistent with this defect, but the live records do not prove it was the
only contributor to the failure.

The original bounded delivery queue now holds one validated reply per
peer until the existing pacing boundary. Actual admission advances the
unchanged pacing guard, including across session replacements. Deferred
replies do not occupy a send turn before they are due; other existing
delivery can continue. There is no added queue capacity, message, timeout,
or authentication exception. The original handoff timestamp means queued
time stays inside the measured uncertainty. A session replacement or
expiry still retires the reply. The progress projection adds
`reply_deferred`; `reply_rate_limited` now counts an occupied reply slot.

Windows strict Clippy with `iroh-transport` passed. Five targeted tests
passed: bounded reply deferral/admission, expiry before I/O, pending Carrier
probe expiry/generation, clock skew/jump/replay, and session counter
isolation. The first test uses the original local sender and a
deterministic pacing boundary; this is not physical Iroh acceptance.
No new Full, package, deployment, or live probe was run for this fix.
