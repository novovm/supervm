# NOVOVM Product Mainline Overlay Lifecycle v1

Status: implementation candidate and opt-in only. It is not production-
activation-ready until relay admission/resource bounds, durable recipient
acknowledgement, and real public-topology evidence are closed.

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
relay sessions. Every configured peer has its own state machine, session keys,
replay window, in-memory pending delivery queue, and propagation metric ID:

```text
Idle -> Handshaking -> Active
  ^          |           |
  |          +-----------+
  +------ Cooldown <------  attributable peer fault
```

`Cooldown` applies bounded exponential retry delay to that peer only. It does
not close the shared relay or reset another peer's active channel. The current
`pending_by_peer` queue is nevertheless not resource-bounded: there is no
per-peer count limit, byte limit, TTL, durable journal, or restart recovery for
queued obligations. Session independence must not be interpreted as bounded or
durable delivery.

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

## Peer error-domain containment and signed relay rotation

While the worker process remains alive, the node keeps not-yet-written outbound
transactions in separate in-memory queues when a relay session or individual
peer channel is unavailable. One peer succeeding does not clear another peer's
not-yet-written queue entry. These entries are recipient-specific but are not
bounded or durable obligations.

Errors attributable to a configured peer are contained in that peer's state
machine. Invalid handshake material, handshake expiry, an invalid authenticated
envelope or NovoRUDP/classified frame, and pre-authentication buffer overflow
move only that peer to `Cooldown`. The peer's E2E channel and pre-authentication
buffer are discarded, its session-failure counter drives independent
exponential retry, and healthy peers continue over the same authenticated relay
connection. Unknown, unconfigured sources are dropped rather than being allowed
to rotate the shared relay. Envelopes and late Responses from a stale E2E
session generation are also dropped; during `Handshaking`, only the session ID
bound to the current local Offer can consume pre-authentication buffer capacity
or complete the current initiator.

An authenticated and correctly framed native transaction can still fail the
mandatory signature, identity, nonce, or chain-domain ingress checks. That
transaction is rejected before pending state and recorded as a peer rejection;
it does not set the relay worker's global error, stop healthy-peer broadcast,
or rotate the relay. The typed ingress outcome keeps local durable-store,
nonce-registry, configuration, and unknown verifier faults fail-closed as
worker-global errors; unknown failures are never downgraded to hostile input.
Per-peer compute and byte budgets for repeated semantic rejections remain part
of the open resource-governance gate.

Only faults in the shared carrier remain relay-scoped: WSS read/write/close,
relay-wire failure for which no trustworthy peer can be attributed, or relay
authentication/lifecycle failure. Those faults may close the shared connection
and invoke signed relay rotation for every multiplexed peer.

Shared relay failure causes the node to:

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
at most 64 opaque envelopes from each configured remote peer. It cannot decrypt
or ingest them until the signed handshake succeeds. Overflow or a
post-handshake authentication failure isolates that peer and enters its
cooldown; it does not close the shared relay. This 64-envelope pre-authentication
limit does not bound `pending_by_peer`, total bytes, authenticated traffic, or
the relay's aggregate resource use.

## Delivery semantics and remaining resource gates

The current `Delivery { delivered: true }` event means only that the encrypted
envelope was accepted by the local relay socket write. At that point the
in-memory queue entry is removed. It does not prove that the recipient read the
envelope, opened the E2E frame, persisted quarantine state, or accepted a native
transaction. There is no per-recipient durable ACK or delivery journal, so a
disconnect after socket write can lose a sent-but-unacknowledged obligation and
a process restart cannot reconstruct in-memory pending entries.

The relay manifest exposes session and byte-rate policy fields, but end-to-end
enforcement is not yet complete. In particular, concurrent relay admission,
per-identity/per-source session accounting, aggregate bytes, and pending queue
count/byte/TTL limits remain open. Consequently this slice is accurately named
**Mesh Peer Error-Domain Containment v1**. It is not full Peer-Local Fault
Isolation, a complete delivery guarantee, or an activation-ready production
boundary.

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
- A transaction is counted as locally written only after the encrypted relay
  socket write succeeds. Failure reported before that write completes leaves
  the not-yet-written in-memory queue entry available for retry; success removes
  it without waiting for recipient acknowledgement.
- `Delivery=true` is telemetry for that socket write, not a durable recipient
  ACK, quarantine admission, execution acceptance, vote, QC, or finality.
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
peer-attributable error containment without rotating the healthy shared relay,
failed-candidate cooldown, signed relay rotation, reconnect, and lifecycle
shutdown.

This does not claim a public VPS, cellular, VPN, NAT, or CGNAT result. Those
remain topology evidence work. It also does not claim relay admission/resource
bounds, a durable per-recipient ACK/journal, restart-safe pending delivery, or
production activation readiness.

The exact containment boundary and its negative claims are frozen in
[`NOVOVM_PRODUCT_OVERLAY_MESH_PEER_ERROR_DOMAIN_V1.md`](NOVOVM_PRODUCT_OVERLAY_MESH_PEER_ERROR_DOMAIN_V1.md).
