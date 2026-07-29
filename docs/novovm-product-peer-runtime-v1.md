# NOVOVM Product Peer Runtime v1

`novovm-product-peer` is the headless A/B endpoint used for a real
relay-first delivery run. It uses a NOVOVM node key to authenticate to the
relay, exchanges the peer handshake through the relay, and forwards only
end-to-end encrypted NOVORUDP frames.

It is not the `novovm-node` mainline, does not invoke AOEM, and does not
interpret payloads. A successful local report is not evidence of a public
VPS, NAT, cellular, or CGNAT result.

## Sender Configuration

The sender must provide existing opaque payload files. The runtime never
manufactures test payloads. Each path creates one encrypted NOVORUDP frame.

```json
{
  "role": "sender",
  "identity_key_path": "/etc/novovm/peer-a-ed25519.hex",
  "relay": {
    "endpoint": "wss://relay.example/novovm",
    "expected_relay_peer_id": "<verified-peer-id>",
    "tls_trust": "native_web_pki",
    "connect_timeout_ms": 10000
  },
  "target_peer_id": "<peer-b-id>",
  "payload_paths": [
    "/var/lib/novovm/outbound/frame-001.bin"
  ],
  "report_path": "/var/lib/novovm/reports/peer-a.json"
}
```

## Receiver Configuration

The receiver must declare the expected frame count and may restrict the
accepted source peer. A receiver must not configure `payload_paths`.

```json
{
  "role": "receiver",
  "identity_key_path": "/etc/novovm/peer-b-ed25519.hex",
  "relay": {
    "endpoint": "wss://relay.example/novovm",
    "expected_relay_peer_id": "<verified-peer-id>",
    "tls_trust": "native_web_pki",
    "connect_timeout_ms": 10000
  },
  "expected_source_peer_id": "<peer-a-id>",
  "expected_frame_count": 1,
  "report_path": "/var/lib/novovm/reports/peer-b.json"
}
```

Run the receiver before the sender:

```bash
./novovm-product-peer /etc/novovm/peer-b.json
./novovm-product-peer /etc/novovm/peer-a.json
```

The reports may be included in a signed evidence manifest with
`novovm-product-evidence`. Evidence verification requires an authenticated
relay path, peer handshake through the relay, an established E2E session, the
opaque NOVORUDP boundary, and role-consistent nonzero frame counts.
