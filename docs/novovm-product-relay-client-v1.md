# NOVOVM Product Relay Client v1

`ProductRelayClientV1` is the node-side WSS client used by the product overlay.
It connects to a selected signed relay record, verifies the relay node's
challenge-response identity, and then forwards peer handshake signalling and
opaque E2E frames.

```rust
let mut client = ProductRelayClientV1::connect(&node_identity, &config)?;
client.send_peer_handshake(target_peer_id, RelayPeerHandshakeV1::Offer(offer))?;
let event = client.recv_event()?;
```

The relay's expected `peer_id` must come from a validated
`PeerSignedRelayRecordV1`, never from a raw endpoint string.

## TLS Modes

- `native_web_pki`: normal platform certificate verification.
- `explicit_ca`: caller-supplied certificate bundle.
- `node_key_bound_encrypted`: transport encryption with certificate verification
  deferred to the mandatory signed NOVOVM relay handshake. Use only with an
  expected relay peer ID obtained from a verified relay record.

In all three modes, TLS is not the NOVOVM identity root. The client rejects a
relay that cannot prove ownership of the expected node key. After A/B peer
handshake completes through the relay, all NOVORUDP frames use the independent
E2E AEAD session and remain opaque to the relay.

`ProductRelayConnectorV1` records bounded exponential reconnect delay and
resets it only after a successful authenticated connection. A caller must use
the node overlay's signed candidate pool to rotate after a relay failure; the
connector does not invent endpoints or bypass cooldown policy.
