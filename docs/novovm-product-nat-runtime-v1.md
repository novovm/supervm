# NOVOVM Product NAT Runtime — Native Endpoint Qualification v2

2026-11-03 source-candidate description. The executable and Rust API retain
their original names; **configuration version 2** replaces the retired raw
Ed25519/JSON UDP probe. Old raw keys and old configuration are not a fallback.
This document does not record four-host, platform-package or deployment acceptance.

`novovm-product-nat` now uses the original NovoRUDP `NativeEndpoint`: protected
ML-DSA-65 purpose identity → original version-2 handshake + UDP AEAD key confirmation → original
authenticated session and AEAD → bounded opaque echo/observed-address exchange.
It creates no chain database, validator, asset action or second UDP protocol.
It does not interpret APFL or execute ledger payloads.

Source: [node runtime](../crates/novovm-node/src/product_nat_runtime.rs),
[original endpoint](../crates/novovm-network/src/novorudp/endpoint.rs),
[shared qualified identity loader](../crates/novovm-relay/src/transport_identity.rs).
The old network `product_nat` signed-JSON codec is retired; that module retains
only routing-outcome types/helpers, not a second authentication path.

## Four mode names, one authenticated observation exchange

| Configuration mode | Current behavior |
| --- | --- |
| `observed_endpoint_observer` | Listen, accept the pinned peer's authenticated request, reply with the observed UDP source address. |
| `nat_punch_target` | Same server-side exchange; the historical name does not enable an automatic hole-punch coordinator. |
| `observed_endpoint_probe` | Connect to the pinned peer through the original endpoint, send a fresh opaque nonce, verify the matching authenticated reply. |
| `nat_punch_probe` | Same client-side exchange to the explicitly supplied candidate address; no automatic candidate discovery or routing promotion. |

The report normalizes these to `mode: "observer"` or `"probe"`.
This tool does **not** automatically select `PunchedDirect`, switch to a relay,
open a firewall, modify NAT state policy or prove general Internet reachability.
An address observed on this socket/session is only a candidate for that path.

## Identity, artifact and configuration inputs

`identity_key_path` must point to an existing protected **Transport** purpose
envelope (`NVPK0003`, version 3, suite 65) in the configured nonzero `chain_id`.
Use the original [purpose preparation contract](NOVOVM_PURPOSE_IDENTITY_PREPARATION.md).
The tool never generates a replacement identity or reads a raw 32-byte/hex key.
Windows uses the intended service user's DPAPI custody. Linux requires the
original private-directory/credential custody under the intended UID; do not
borrow B's existing mailbox or Relay credential.

`library` and `library_sha256` identify the approved platform AOEM artifact.
The shared loader pins its SHA-256 before/after loading, qualifies ML-DSA-65,
and checks the protected purpose key through the original execution facade.
Incoming network signatures use the same unified-semantic
`OwnedMldsaVerifier` owner as the asset consumer, obtained from the loaded
`MldsaRuntime::verification_owner_v1`; they do not use the old scalar verify ABI.
Signing/key generation retain the existing shared primitive owner—this is not
a claim that a new unified-semantic signing operation has been implemented.
Signing or authentication failure is returned, not retried with another algorithm.
ML-DSA authenticates identity; it does not establish post-quantum key agreement.

Every mode, including the observer/target, requires `peer_public_key`: the
**complete expected remote ML-DSA-65 public key, exactly 1,952 bytes**. The local
key must differ. An address or a hash label cannot replace that full public pin.
Optional `expected_peer_id`, when present, must match the original
`peer_id_from_mldsa65_public_key_v1` label derived from those exact bytes.

`identity_key_path`, `library` and optional `report_path` resolve relative paths
against the configuration file's directory, not the caller's working directory.
`bind_addr` and client `peer_addr` are explicit socket addresses. A probe requires
a nonzero, non-unspecified, non-multicast peer address; server modes do not need
`peer_addr` and learn the pinned caller's route through the authenticated endpoint.

## Configuration templates — not runnable deployment material

The placeholders below deliberately contain **no usable identity, chain domain
or approved artifact hash**. Replace them with reviewed inputs before execution;
`peer_public_key` must become an integer array, not a string, seed or digest.
No real key generation, chain activation or configuration installation is authorized
by these examples. Documentation-range addresses are not actual host endpoints.

Observer/target template:

```jsonc
{
  "version": 2,
  "mode": "observed_endpoint_observer",
  "bind_addr": "0.0.0.0:41031",
  "identity_key_path": "purpose/observer.protected",
  "chain_id": "<APPROVED_NONZERO_CHAIN_ID_INTEGER>",
  "library": "artifacts/<APPROVED_PLATFORM_AOEM_LIBRARY>",
  "library_sha256": "<APPROVED_64_LOWERCASE_HEX_SHA256>",
  "peer_public_key": [ /* approved prober public key: exactly 1952 byte integers */ ],
  "timeout_ms": 3000,
  "run_for_ms": 10000,
  "report_path": "reports/observer.json"
}
```

Probe template (use the observer's full public key, not the prober's):

```jsonc
{
  "version": 2,
  "mode": "observed_endpoint_probe",
  "bind_addr": "0.0.0.0:41020",
  "identity_key_path": "purpose/prober.protected",
  "chain_id": "<SAME_APPROVED_NONZERO_CHAIN_ID_INTEGER>",
  "library": "artifacts/<APPROVED_PLATFORM_AOEM_LIBRARY>",
  "library_sha256": "<APPROVED_64_LOWERCASE_HEX_SHA256>",
  "peer_public_key": [ /* approved observer public key: exactly 1952 byte integers */ ],
  "peer_addr": "198.51.100.10:41031",
  "timeout_ms": 3000,
  "run_for_ms": 10000,
  "report_path": "reports/probe.json"
}
```

For the historical target/punch naming, change only the mode to
`nat_punch_target` / `nat_punch_probe`; their current exchange is the same one
described above. `relay_candidate_available` is no longer a configuration field.
Unknown fields and version-1 configurations are rejected.

After reviewed configuration and operation approval, the original invocation is:

```text
novovm-product-nat <approved-config-path>
```

`timeout_ms` is 2–30,000 ms; `run_for_ms` is 1–120,000 ms and must cover the
timeout. The observer/target normally runs for its bounded duration; the probe
can finish after its matching authenticated reply. The tool does not change
production timeouts or start a validator.

## Report interpretation

The report's `scope` is `novovm_native_authenticated_endpoint_v2`,
`handshake_version` is 2 and `signature_suite` is 65.

- **Probe `accepted=true`:** this run received the pinned peer's authenticated
  reply with the matching object/nonce and decoded observed address;
  `round_trip_confirmed=true`. It proves
  this bounded session exchange, not future reachability, arbitrary delivery or
  a ledger/consensus result.
- **Server `accepted=true`:** it received a request from the expected authenticated
  peer and the original synchronous reply submission was accepted. This is **not**
  proof that the remote application received or consumed that reply. Correlate
  the client report/session separately; `round_trip_confirmed` remains **false**.
- `local_peer_id` identifies this run's local Transport identity; `peer_id`
  identifies the configured expected remote identity. Correlate both identities
  and `session_id` when comparing observer/probe reports, not just `accepted`.
- `authenticated_requests` counts matching authenticated requests received by
  the observer; `local_replies_submitted` counts replies accepted by its local
  send owner. Neither counter proves remote receipt. Both counters are zero in
  the probe report.
- `authenticated_exchanges` is role-specific: the probe records one confirmed
  reply; the observer records its locally submitted replies. It is not a shared
  end-to-end delivery counter, a transaction count or a remote acknowledgment.
- `observed_endpoint` is the server-observed source address carried in the
  authenticated reply. It is not an inbound-reachability guarantee.
- `network_only=true`, `aoem_called=true`, `ledger_semantics=false` and
  `apfl_interpreted=false` describe the execution boundary, not an end-to-end
  product or four-host acceptance result.
- `novorudp_wire_changed=false` describes preservation of the inner data-frame
  contract; it does not claim that the old Ed25519 handshake bytes are unchanged.

When a configured report path exists, retain the failed report as well as the
process error. Timeout or rejected authentication must not be relabeled success.
Failures before the report is constructed may produce only an error, not a report.
The evidence verifier enforces these role-specific stages; a signed observer
report alone does not become a confirmed round-trip report. It does not
automatically pair reports or establish four-host topology acceptance.

## Qualification boundary

Loopback/LAN success proves only the tested protocol, socket and local route.
Home NAT, CGNAT, VPN/TUN, public-cloud security rules and mobile networks require
their own mixed-topology evidence. A/B/C/D's real authenticated exchanges,
actual outbound routes and reconnect/replay rejection remain separately measured
gates; this source update does not sign them off.
