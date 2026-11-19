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

Follow-up: validate the ordering of the original owner’s cold-session
measurement and pending delivery, including the interaction between reply
pacing and the existing readiness deadline. First reproduce the timing in
a local regression; do not rerun this live object, revive a rejected
object, widen a timeout, or relax identity, expiry or replay policy.
