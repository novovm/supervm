# NOVOVM Product Evidence v1

2026-11-03 source-candidate update. The executable/API names retain `v1`, but
the current signed manifest is **version 2 / signature suite 65**. Old Ed25519
manifests and raw hex-key signing inputs are retired, not a verification fallback.
This update is not a package deployment or four-host acceptance record.

`novovm-product-evidence` produces and verifies a signed manifest over actual
runtime reports. It is a verifier, not a success generator: a report must first
meet its scope-specific security boundaries before it can be included.

Build an evidence manifest after a run:

```text
novovm-product-evidence build \
  <approved-report-root> \
  <approved-nonzero-chain-id> \
  <absolute-protected-transport-key-path> \
  <absolute-approved-AOEM-library-path> \
  <approved-library-sha256> \
  <output-manifest.json> \
  relay.json nat-punch.json node-overlay.json
```

Verify it independently:

```text
novovm-product-evidence verify \
  <approved-report-root> \
  <absolute-approved-AOEM-library-path> \
  <approved-library-sha256> \
  <manifest.json>
```

These placeholders are not approved deployment inputs. Build loads an existing
protected Transport purpose key (`NVPK0003`, version 3, suite 65) via the original
custody owner. Verify loads only the qualified **read-only verifier**, not a local
private key or custody file. Both pin the approved platform library SHA-256;
native signature verification uses the shared unified-semantic verifier owner.
Signing/keygen remain the existing shared primitives. No verification error
triggers an algorithm change. Use absolute CLI paths; these are not rebased
relative to a separate configuration file.

The verifier checks:

- manifest version 2, suite 65, complete 1,952-byte ML-DSA-65 public key,
  derived signer identity and 3,309-byte signature;
- SHA-256 for each report relative to the declared root;
- `accepted`, opaque-payload, and unchanged-wire boundaries;
- relay challenge-response and non-authority boundaries, daemon report version 2, positive and
  self-consistent resource limits, and runtime usage that does not contradict those limits;
- the current native endpoint observation scope: accepted authenticated exchange,
  handshake version 2/suite 65, session identity, local identity and role-specific
  request/reply/round-trip stages;
- node strategy receipt signature and decentralized-control-plane boundary.
- offline mainline topology full-mesh symmetry while requiring all external
  execution/proof flags to remain false.

The endpoint observation report does not automatically prove hole punching or
promote a route. Server-side reply submission is not proof of remote application
receipt. The verifier requires these distinct states:

- `mode="probe"`: `round_trip_confirmed=true`, one authenticated exchange,
  zero `authenticated_requests` and zero `local_replies_submitted`.
- `mode="observer"`: `round_trip_confirmed=false`, positive local reply count
  equal to `authenticated_exchanges`, and received `authenticated_requests` at
  least as large as the local reply count.

Missing or inconsistent stage fields are rejected; an observer report claiming
a confirmed round trip is rejected even when `accepted=true`. A valid observer
report can document local request handling, not remote receipt. Preserve and
correlate the probe report using reciprocal `local_peer_id` / `peer_id` and the
same `session_id`; the current manifest verifier does not perform that paired
topology acceptance automatically. See the
[current NAT tool boundary](novovm-product-nat-runtime-v1.md).

An offline topology preflight can be signed into the manifest:

```bash
novovm-product-topology /etc/novovm/topology-plan.json \
  > /var/lib/novovm/reports/topology-preflight.json
```

This records deployable configuration intent only. It is not a replacement for
the later signed reports from actual public/VPN/cellular runs.

It intentionally reports `real_public_topology_proven=false` and
`real_cross_nat_proven=false`: report integrity alone cannot prove an external
network topology. Those flags require a later verifier profile with signed,
multi-node public/VPN/cellular evidence and independently identifiable runs.

The current evidence profile does not yet cross-bind a signed directory endpoint's advertised capacity to the
daemon report's enforced capacity. Operators must deploy one homogeneous package checksum and
verify signed endpoint/config/report alignment separately; mixed old/new relay wire versions and
rolling upgrades are not claimed.
