# NOVOVM Product Evidence v1

`novovm-product-evidence` produces and verifies a signed manifest over actual
runtime reports. It is a verifier, not a success generator: a report must first
meet its scope-specific security boundaries before it can be included.

Build an evidence manifest after a run:

```bash
novovm-product-evidence build \
  /var/lib/novovm/reports \
  /etc/novovm/evidence-ed25519.hex \
  /var/lib/novovm/reports/evidence.json \
  relay.json nat-punch.json node-overlay.json
```

Verify it independently:

```bash
novovm-product-evidence verify \
  /var/lib/novovm/reports \
  /var/lib/novovm/reports/evidence.json
```

The verifier checks:

- manifest signer identity and Ed25519 signature;
- SHA-256 for each report relative to the declared root;
- `accepted`, opaque-payload, and unchanged-wire boundaries;
- relay challenge-response and non-authority boundaries, daemon report version 2, positive and
  self-consistent resource limits, and runtime usage that does not contradict those limits;
- NAT direct routing only when `ack_valid=true`;
- node strategy receipt signature and decentralized-control-plane boundary.
- offline mainline topology full-mesh symmetry while requiring all external
  execution/proof flags to remain false.

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

Evidence v1 does not yet cross-bind a signed directory endpoint's advertised capacity to the
daemon report's enforced capacity. Operators must deploy one homogeneous package checksum and
verify signed endpoint/config/report alignment separately; mixed old/new relay wire versions and
rolling upgrades are not claimed.
