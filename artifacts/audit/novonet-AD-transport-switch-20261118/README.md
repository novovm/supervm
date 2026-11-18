# A→D live Iroh fallback and NovoRUDP recovery

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
