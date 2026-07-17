# NOVOVM Product NAT Runtime v1

`novovm-product-nat` is a headless UDP candidate-observation and cooperative
punch tool. It uses the same Ed25519 node identity model as the overlay runtime.
Every probe and ACK is signed and binds a random nonce, peer identity, target,
and expiry. It does not send payloads, ledger data, or execution instructions.

## Modes

```text
observed_endpoint_observer  Receive a signed probe and return the sender's observed UDP address.
nat_punch_target            Receive a signed punch request and return a signed ACK.
observed_endpoint_probe     Ask a known observer for this socket's observed address.
nat_punch_probe             Try a signed UDP punch against a target candidate.
```

Run any mode with:

```bash
novovm-product-nat /etc/novovm/nat.json
```

The `identity_key_path` file contains exactly 64 hexadecimal characters for a
32-byte Ed25519 node key. The key is never written to the report.

## Observer Configuration

```json
{
  "mode": "observed_endpoint_observer",
  "bind_addr": "0.0.0.0:41031",
  "identity_key_path": "/etc/novovm/observer-ed25519.hex",
  "timeout_ms": 3000
}
```

An observer can be any reachable NOVOVM node. The prober must know its `peer_id`
from a signed record or another authenticated channel; an arbitrary address is
not an identity authority.

## Observed Candidate Probe

```json
{
  "mode": "observed_endpoint_probe",
  "bind_addr": "0.0.0.0:41020",
  "identity_key_path": "/etc/novovm/node-ed25519.hex",
  "peer_addr": "198.51.100.10:41031",
  "expected_peer_id": "novovm-ed25519:<observer-key-hash>",
  "timeout_ms": 3000,
  "report_path": "/var/lib/novovm/reports/observed-endpoint.json"
}
```

The report's `observed_endpoint` is a candidate, not a claim that inbound UDP
is reachable. Publish it only through the signed, minimal-disclosure directory
policy.

## Cooperative Punch

On the target node, run:

```json
{
  "mode": "nat_punch_target",
  "bind_addr": "0.0.0.0:41020",
  "identity_key_path": "/etc/novovm/node-b-ed25519.hex",
  "timeout_ms": 3000
}
```

On the source node, use the target's previously observed candidate and known
node identity:

```json
{
  "mode": "nat_punch_probe",
  "bind_addr": "0.0.0.0:41020",
  "identity_key_path": "/etc/novovm/node-a-ed25519.hex",
  "peer_addr": "203.0.113.20:41020",
  "expected_peer_id": "novovm-ed25519:<node-b-key-hash>",
  "timeout_ms": 3000,
  "relay_candidate_available": true,
  "report_path": "/var/lib/novovm/reports/nat-punch.json"
}
```

Only a valid target-signed ACK produces `PunchedDirect`. Timeout, malformed
packets, stale responses, nonce mismatch, and identity mismatch never promote
direct routing. They produce `RelayNovoRudp` when a valid relay candidate is
available, otherwise `QueueFallback`.

## Test Boundary

Loopback/LAN success proves protocol and local socket behavior only. It does
not prove traversal through home NAT, CGNAT, VPN/TUN, mobile carrier networks,
or a firewall. Those require mixed-topology evidence from separate networks.
