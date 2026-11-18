# A→D live Iroh fallback and NovoRUDP recovery

## D return and first failure

[D's original projection](D-receiver/D-receiver-raw.json), commit `b6cfdd9f`,
matched A's nonce, Object ID, Session ID, timestamps and the authenticated
Carrier binding for the first object. The original owner recorded the
sender DONE, then D recorded `probe_rejected_time_untrusted_or_outside_validity`
without an interval. The 5b receiver used the same event for a missing
session time reference and a rejected validity window. Its snapshot was
exported after the session changed again. A's later `relative_ready=true`
belongs to A only (and is after DONE); it cannot establish the time state
at D on arrival. On A, initial native admission preceded carrier admission
by 141 ms, with carrier DONE a further 22 ms later. D recorded a direct
reject, not a pending-boundary event. Missing samples is consistent with
the short handoff but is not retrospectively proven. For the second object D recorded native
`probe_pending_time_boundary` then `probe_received_integrity_verified` on
the same new `native_udp` session as A. D's delayed export contains no
independent fragment rows for either object.

The follow-on fixes the observed receiver design gap in the original
diagnostic owner. A first-reference warm-up on an authenticated bound
carrier may retain the already-authenticated object metadata only within
the original four-slot pending limit and the existing two-second measurement
lifetime. The normal authenticated measurements continue independently of
the object. Final acceptance requires the same session and time generation,
a fully established reference, and the original unmodified validity bounds.
A valid window is clipped to the retained monotonic limit; a failed window,
clock invalidation, expired wait, or replay does not become a retry. A
structured `peer_time_reason` distinguishes these outcomes without changing
wire messages, asset rules, the transport DONE meaning, or network budgets.

Windows release `cargo check`, strict Clippy with the Iroh and test
targets, and the new original-lifecycle pending receipt test passed. The
test exercises established same-session acceptance, original expiry,
local wait expiry and generation invalidation. It uses the local fixture
and does not repeat or certify the A/D network experiment.

The original two objects are never resent. Only after candidate
qualification, the live follow-up may update D's original owner to the
fixed build, verify D existing/health, and send one **new** probe over the
Iroh fallback from A. No B stop and no second native UDP object. Capture
the D-side event and fragment proof immediately, then verify cleanup of
the A/D-only rule window. This is not a release or updated live result.

## D: return the existing receiver evidence only

Keep the current 5b node running. From an administrator PowerShell 7 on D,
run [Collect-D.ps1](Collect-D.ps1) with a fresh `-EvidenceRoot`, or return
the same raw event/transport projections if already collected. Push only
`D-receiver-raw.json` and its SHA256 in `D-receiver/`. Keep the full
progress snapshot on D; do not publish full config/credentials. Preserve
missing or rejected events as recorded, and do not re-send either object,
start an observer or restart a node.

## A-side evidence

A used 55cafb07, original node PID 16528, and D's original signed Status
passed in 32 ms, at the original state root and height 1. From A, the
bounded fault window blocked only the A/D node UDP pair
`192.168.71.74:41051 ↔ 192.168.71.59:41053` for the existing A executable.
It did not block Iroh `42051/42053`; both temporary rules were removed and
verified absent. The independently armed restore task was removed after
rules were verified absent. No B/C changes, node restart or previous
object replay.

One new diagnostic object was admitted on the old native session, then
the original owner recorded a session replacement, resubmitting **the same
new object internally**, and completed on an authenticated bound Iroh
carrier. A second distinct object was sent only after a new UDP session
reported `udp_route_activated` and a valid relative-time reference. It
completed over the actual original NovoRUDP endpoint.

| Stage | Actual sender Object DONE path | Circuit-independent native session | Object |
| --- | --- | --- | --- |
| Fallback | `authenticated_bound_carrier` | `236652bf1c12aea83ebb7ceb0158ac3e` | `8b736dc3d1da071305cb755ebe9aadfde2c1950b4f2253df0a8c71e3412161c2` |
| Recovered | `native_udp` | `4003b8743c8527735eec3f215a4be367` | `f8948354fc2bde2ab65fe09e283327bc9a1f199da5c003d0c37762bcd0fc32c2` |

Receiver acceptance is pending. Bound Iroh is the application carrier
used; evidence here does not identify its direct-vs-external-relay
underlay. No business receipt, consensus or effect on a prior relay test
is asserted.
