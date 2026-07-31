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
workspace-parent absolute path is built into the runtime. Relative identity,
bootstrap-cache, and explicit-CA paths inside the config are resolved from the
config file's own directory, not from the process working directory.

Recommended duplex config:

```json
{
  "chain_id": 1,
  "role": "duplex",
  "identity_key_path": "runtime/node-ed25519.hex",
  "peers": [
    {
      "peer_id": "novovm-ed25519:<peer-b-public-key>",
      "metric_peer_id": 9991002
    },
    {
      "peer_id": "novovm-ed25519:<peer-c-public-key>",
      "metric_peer_id": 9991003
    },
    {
      "peer_id": "novovm-ed25519:<peer-d-public-key>",
      "metric_peer_id": 9991004
    }
  ],
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
  "metric_peer_id": 9990777,
  "reconnect_base_delay_ms": 250,
  "reconnect_max_delay_ms": 30000
}
```

The `duplex` role uses one authenticated relay connection for the local node
identity and multiplexes one independent E2E secure channel per configured
peer. Simultaneous offers are resolved deterministically; a peer that reconnects
may initiate a fresh offer without forcing the other nodes to reconnect their
relay sessions. Every peer has its own session keys, replay window, pending
delivery queue, and propagation metric ID.

Both ends may propagate pending transactions and both return decrypted
payloads to the same native ingress boundary. For a compatibility two-node
deployment, `target_peer_id` plus the top-level `metric_peer_id` remains
accepted when `peers` is omitted.

The original one-way compatibility roles remain available. A responder uses:

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

An `initiator` owns outbound propagation and a `responder` owns inbound
delivery. They remain useful for constrained or staged deployments, but
`duplex` is the product-mainline role.

## Recovery and signed relay rotation

The node keeps ownership of queued outbound transactions while a relay session
or individual peer channel is unavailable. Each submitted transaction is
expanded into a bounded per-peer delivery obligation. One peer succeeding does
not clear another peer's pending delivery. A failed encrypted write does not
produce a successful delivery receipt and does not discard the queued
transaction.

Connection or session failure causes the node to:

1. mark the active signed relay candidate failed and enter its configured
   cooldown;
2. select another candidate only from the already verified signed pool;
3. authenticate the replacement relay by its node key;
4. establish a fresh peer E2E session; and
5. resume queued delivery with fresh session keys and replay windows.

Reconnect delay uses bounded exponential backoff from
`reconnect_base_delay_ms` through `reconnect_max_delay_ms`. Rotation never
introduces an unsigned endpoint or turns bootstrap/relay infrastructure into a
transaction authority.

Relay concurrency may deliver queued ciphertext immediately beside the peer
handshake response. Before E2E authentication completes, the node may retain
at most 64 opaque envelopes from the configured remote peer. It cannot decrypt
or ingest them until the signed handshake succeeds; an unexpected source,
overflow, or post-handshake authentication failure closes that session.

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
  succeeds. A write interrupted by disconnect remains queued for the next
  authenticated session.
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
bootstrap, relay, E2E peer count, per-peer in-flight delivery count, ingress,
delivery, reconnect/rotation, and ownership evidence.

The mainline gate runs local real-WSS tests with node-owned Overlay runtimes.
It verifies signed relay selection, relay identity authentication,
deterministic peer E2E establishment, three-node multiplexing over one relay
session per node, simultaneous bidirectional encrypted NovoRUDP delivery,
single-peer restart recovery without reconnecting the other nodes,
failed-candidate cooldown, signed relay rotation, reconnect, and lifecycle
shutdown.

This does not claim a public VPS, cellular, VPN, NAT, or CGNAT result. Those
remain topology evidence work.
