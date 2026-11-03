# NOVOVM Product Peer Runtime v1

2026-11-03 source-candidate operator update, not a deployed-platform sign-off.

`novovm-product-peer` is the headless A/B endpoint used for a real
relay-first delivery run. It uses a NOVOVM node key to authenticate to the
relay, exchanges the peer handshake through the relay, and forwards only
end-to-end encrypted NOVORUDP frames.

It is not the `novovm-node` transaction mainline. It invokes the original AOEM
identity/signature owner, but does not execute transactions or interpret opaque
payloads. A successful local report is not evidence of a public
VPS, NAT, cellular, or CGNAT result.

## Sender Configuration

The sender must provide existing opaque payload files. The runtime never
manufactures test payloads. Each path creates one encrypted NOVORUDP frame.

```jsonc
{
  "role": "sender",
  "identity_key_path": "<ABSOLUTE_PROTECTED_TRANSPORT_KEY_A>",
  "chain_id": "<APPROVED_NONZERO_CHAIN_ID_INTEGER>",
  "library": "<ABSOLUTE_APPROVED_PLATFORM_AOEM_LIBRARY>",
  "library_sha256": "<APPROVED_64_LOWERCASE_HEX_SHA256>",
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

```jsonc
{
  "role": "receiver",
  "identity_key_path": "<ABSOLUTE_PROTECTED_TRANSPORT_KEY_B>",
  "chain_id": "<SAME_APPROVED_NONZERO_CHAIN_ID_INTEGER>",
  "library": "<ABSOLUTE_APPROVED_PLATFORM_AOEM_LIBRARY>",
  "library_sha256": "<APPROVED_64_LOWERCASE_HEX_SHA256>",
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

These templates contain no usable chain/key/hash; replace placeholders with
reviewed inputs before execution. Each peer needs its own already-protected
Transport purpose key (`NVPK0003`, version 3, suite 65), not the former raw
Ed25519 hex file. Both config/CLI identity loading and incoming signature
verification use the original qualified owner; verification uses the shared
unified semantics. No downgrade is selected after failure. The existing
signing/keygen primitives and packet AEAD remain their original operations.
Use absolute identity, library, payload and report paths: this config loader
does not rebase relative paths against the config directory.
The relay/target/source labels must come from authenticated ML-DSA-65 public
materials; arbitrary hostnames or old Ed peer labels are not identity pins.

The reports may be included in a signed evidence manifest with
`novovm-product-evidence`. Evidence verification requires an authenticated
relay path, peer handshake through the relay, an established E2E session, the
opaque NOVORUDP boundary, and role-consistent nonzero frame counts.
New reports also bind `transport_signature_suite: 65` and
`transport_handshake_version: 2`. Sender submission is not proof that a remote
application consumed the payload; retain and correlate receiver evidence.
