# NOVOVM Product Mainline Overlay Lifecycle v1

Status: production candidate, opt-in until a real public topology is signed.

The Product Overlay can now be owned by the `novovm-node` native execution
pipeline instead of running only as the standalone `novovm-product-peer`
evidence tool.

The ownership boundary is:

```text
signed bootstrap manifests
  -> locally selected signed WSS relay
  -> relay node-key challenge response
  -> peer-to-peer signed handshake
  -> end-to-end encrypted NovoRUDP data frame
  -> NOVOVM native raw transaction ingress
  -> signature + identity + nonce + chain-domain checks
  -> pending runtime
  -> AOEM execution, persistence, receipt, and state owner
```

The relay sees only the secure envelope. It is not an execution authority,
identity authority, consensus authority, or AOEM policy owner.

## Node lifecycle

Enable the runtime only on the native execution pipeline entry:

```text
NOVOVM_NODE_MODE=native_execution_pipeline
NOVOVM_PRODUCT_MAINLINE_OVERLAY_ENABLED=true
NOVOVM_PRODUCT_MAINLINE_OVERLAY_CONFIG=<path-to-config.json>
```

The config path is supplied by the operator. No repository, drive-letter, or
workspace-parent absolute path is built into the runtime.

Example initiator config:

```json
{
  "chain_id": 1,
  "role": "initiator",
  "identity_key_path": "runtime/node-ed25519.hex",
  "target_peer_id": "novovm-ed25519:<peer-public-key>",
  "overlay": {
    "cache_path": "runtime/bootstrap-cache.json",
    "trusted_signer_public_keys": [[1, 2, 3]],
    "minimum_bootstrap_signatures": 1,
    "embedded_sources": []
  },
  "connect_timeout_ms": 10000,
  "read_timeout_ms": 250,
  "tls_trust": "native_web_pki",
  "channel_capacity": 1024,
  "metric_peer_id": 9990777
}
```

A responder uses:

```json
{
  "chain_id": 1,
  "role": "responder",
  "identity_key_path": "runtime/node-ed25519.hex",
  "expected_source_peer_id": "novovm-ed25519:<peer-public-key>",
  "overlay": {
    "cache_path": "runtime/bootstrap-cache.json",
    "trusted_signer_public_keys": [[1, 2, 3]],
    "minimum_bootstrap_signatures": 1,
    "embedded_sources": []
  }
}
```

The public key arrays above are abbreviated examples; each key must contain
exactly 32 bytes, and a trusted signed manifest must be available from the
embedded source or unexpired local cache.

The initiator owns outbound pending-transaction propagation. The responder
owns inbound delivery for the authenticated peer. This v1 role split keeps one
E2E session single-purpose and deterministic. The signed v1 gate covers the
initiator-to-responder direction; simultaneous bidirectional scheduling is not
claimed by this milestone.

## Fail-closed rules

- Enabling the runtime without a config is an error.
- Config chain ID must equal the native pipeline chain ID.
- No trusted or reachable signed relay candidate is an error.
- The selected live transport must be WSS.
- The configured remote peer must differ from the local node identity.
- Only E2E-authenticated NovoRUDP `Data` frames for the configured chain enter
  native transaction ingress.
- Invalid signature, identity, nonce, chain domain, empty payload, or wrong
  frame type is rejected before pending state.
- A transaction is counted as propagated only after the encrypted relay write
  succeeds.
- The worker stops and joins with the owning node runtime.

`NOVOVM_NATIVE_EXECUTION_PIPELINE_BROADCAST_ENABLED` defaults to disabled when
the Product Overlay runtime is enabled, preventing the old local observation
path from consuming the propagation budget. An explicit operator value still
wins.

## Signoff

Set these for a bounded topology gate:

```text
NOVOVM_PRODUCT_MAINLINE_OVERLAY_SIGNOFF_REQUIRED=true
NOVOVM_PRODUCT_MAINLINE_OVERLAY_EXPECTED_RECEIVED=<count>
NOVOVM_PRODUCT_MAINLINE_OVERLAY_EXPECTED_DELIVERED=<count>
```

The final pipeline summary includes `product_mainline_overlay` with lifecycle,
bootstrap, relay, E2E, ingress, delivery, and ownership evidence.

The mainline gate runs a local real-WSS relay test with two node-owned Overlay
runtimes. It verifies signed relay selection, relay identity authentication,
peer E2E establishment, opaque encrypted NovoRUDP delivery, and lifecycle
shutdown.

This does not claim a public VPS, cellular, VPN, NAT, or CGNAT result. Those
remain topology evidence work.
