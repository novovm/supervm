# NOVOVM Product Relay Daemon v1

`novovm-product-relay` is a headless WSS relay runtime. It does not execute
payloads, interpret business semantics, or act as a NOVOVM trust authority.

The relay requires a PEM TLS certificate/key for the WebSocket transport and a
separate Ed25519 relay identity key. The TLS certificate encrypts the transport;
the signed NOVOVM node challenge-response remains the protocol identity check.

## Configuration

```json
{
  "bind_addr": "0.0.0.0:443",
  "tls_cert_path": "/etc/novovm/tls/fullchain.pem",
  "tls_key_path": "/etc/novovm/tls/privkey.pem",
  "relay_identity_key_path": "/etc/novovm/relay-ed25519.hex",
  "report_path": "/var/lib/novovm/reports/relay.json",
  "report_interval_ms": 5000,
  "session_queue_capacity": 256,
  "offline_queue_per_peer": 512,
  "offline_queue_total": 16384,
  "session_ttl_ms": 45000,
  "rate_limit_frames": 4096,
  "rate_limit_window_ms": 1000
}
```

`relay_identity_key_path` contains exactly 64 hexadecimal characters: a single
32-byte Ed25519 secret. It must be readable only by the relay service account
and must never be placed in the JSON configuration, report, manifest, or relay
record.

Run the daemon with:

```bash
novovm-product-relay /etc/novovm/relay.json
```

For a bounded smoke run only, add `"run_for_ms": 60000` to the config. Normal
deployments omit it and manage process lifetime through the operating system.

## Runtime Boundary

- Client WebSocket frames must be masked and are limited to 1 MiB.
- A client first sends a signed NOVOVM handshake offer; no arbitrary `peer_id`
  registration is accepted.
- After relay-session authentication, the relay can forward signed peer handshake
  offers and responses by `target_peer_id`. This establishes an A/B E2E session
  without requiring either NAT node to expose an inbound listener. The relay only
  checks that the signed message route matches its authenticated source session.
- Relay sessions are authenticated, expiring, rate-limited, bounded, replaceable
  on reconnect, and closed during graceful shutdown.
- Only `SecureNovoRudpEnvelopeV1` ciphertext is forwarded. The relay cannot
  decrypt its NOVORUDP frame or infer execution semantics.
- `reports/relay.json` is atomically replaced with session and queue counters.
- WebPKI/CA trust is not a NOVOVM identity root. A client must bind the relay
  node key through a signed relay record and verify the node handshake.
