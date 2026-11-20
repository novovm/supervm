# A→D Iroh fallback: live delivery passed

On 2026-11-20, A (PID 33356) and D (PID 300) ran `654b9991`
using their original databases. A submitted exactly one new probe.

- Object: `acf2fd79a8b7032d369adb9e0dbe14abe9da35cef79336c7e2dc778e2b5bbe97`
- Session: `186bfa0798f975d81d5e0607592fae68`
- Actual path: `authenticated_bound_carrier` (Iroh). The session differs
  from the pre-test native session.
- [A recorded sender DONE](A-fallback/iroh-fallback-proof.json) at
  02:55:09.268 UTC. [D's export](D-receiver-result/D-receiver-result.json)
  matched object, nonce, session, Carrier binding and recomputed transfer tag.
  The export SHA256 matched [D's pin](D-receiver-result/SHA256SUMS).
- D verified 1/1 fragments and recorded one hash chunk. The same
  acceptance event appeared in four snapshots, not four deliveries.
- On D's clock, `reference_warming_up` at 02:55:09.618 UTC was followed by
  `probe_received_integrity_verified` at 02:55:09.814 UTC: 196 ms,
  within the unchanged 2 s wait. The accepted peer-clock interval
  `[1795143309440, 1795143309471]` was entirely within the original
  validity window `[1795143308705, 1795143318705)`. No rejection was present.
- The early same-session snapshot showed `reply_deferred=1` and
  `reply_rate_limited=0` on D. That snapshot preceded readiness; the
  final acceptance event and its validated interval prove time acceptance.

This passes the new Iroh object test, not an asset/business receipt or
consensus claim. The temporary A/D native UDP rules were removed. No B
stop, node restart, old-object replay, or NovoRUDP return retest.
