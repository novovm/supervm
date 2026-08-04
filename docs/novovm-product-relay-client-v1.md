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

`send_peer_handshake` and encrypted data sends complete only after a correlated relay
`ForwardOutcome` is received. Accepted dispositions mean the relay forwarded the message or
took bounded in-memory ownership of it. They do **not** prove recipient receipt, decryption,
ingress acceptance, durable journaling, execution, or proof sealing. While waiting for an
outcome, interleaved public events are buffered under both count and byte limits.
The temporary bound is 64 events and 16 MiB; exceeding either fails the current relay
lifecycle instead of allocating without limit. Callers must continue to consume
`recv_event`: data already written into the single TCP stream can still precede a later
outcome, and this buffer is not an unlimited reliable-delivery queue.
The full wait for one correlated outcome has one absolute deadline that is not reset by
interleaved events. A single protocol-item read also has an absolute deadline and accepts at most
64 Ping/Pong control frames; authenticated writes have their own absolute lower-TCP deadline.
Read buffering uses a cursor with bounded compaction rather than shifting the whole buffer for
every small frame.

Rate/byte admission happens on raw authenticated wire bytes before JSON decode. If that predecode
gate rejects, or the frame is malformed, no trustworthy correlation fields exist and the daemon
closes the connection instead of returning an invented outcome. The overlay retains its pending
obligation and treats this as a relay lifecycle failure. Queue/resource rejection after decode can
return a correlated `ForwardOutcome`.

The relay's expected `peer_id` must come from a validated
`PeerSignedRelayRecordV1`, never from a raw endpoint string.

## TLS Modes

- `native_web_pki`: normal platform certificate verification.
- `explicit_ca`: caller-supplied certificate bundle.
- `node_key_bound_encrypted`: compatibility mode restricted in code to a resolved loopback
  endpoint for local tests. It is not safe on an untrusted network because the signed relay
  handshake is not yet channel-bound to subsequent relay wire messages.

In all three modes, TLS is not the NOVOVM identity root. The client rejects a
relay that cannot prove ownership of the expected node key. After A/B peer
handshake completes through the relay, all NOVORUDP frames use the independent
E2E AEAD session and remain opaque to the relay.

Public and production endpoints must use `native_web_pki` or `explicit_ca`; the signed relay
identity challenge remains mandatory in addition to certificate validation.

`ProductRelayConnectorV1` records bounded exponential reconnect delay and
resets it only after a successful authenticated connection. A caller must use
the node overlay's signed candidate pool to rotate after a relay failure; the
connector does not invent endpoints or bypass cooldown policy.

Client WebSocket writes are capped at 1 MiB, control frames at 125 bytes, and every client frame
uses a fresh unpredictable mask. Every connection also generates a fresh random 16-byte
`Sec-WebSocket-Key`. The client rejects fragmented, RSV-marked, masked server, or
oversized frames before exposing them to the overlay.

This release adds `ForwardOutcome` without a relay-protocol capability negotiation. All relay
clients and daemons in one Product Overlay deployment must therefore run the same release; mixed
old/new processes are not rolling-upgrade compatible.
