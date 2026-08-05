# NOVOVM Product Mainline Overlay Lifecycle v1

Status: resource-bounded implementation candidate and opt-in only. Relay and
Overlay process-memory admission is closed by
[`Product Relay Admission & Resource Bounds v1`](NOVOVM_PRODUCT_RELAY_ADMISSION_RESOURCE_BOUNDS_V1.md),
and the Host-owned durable obligation/recipient-ACK boundary is defined by
[`Product Durable Recipient ACK & Delivery Journal v1`](NOVOVM_PRODUCT_DURABLE_RECIPIENT_ACK_DELIVERY_JOURNAL_V1.md).
Production activation still requires complete main-runtime signoff and real
public-topology evidence.

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
delivery-journal, bootstrap-cache, and explicit-CA paths inside the config are
resolved from the config file's own directory, not from the process working
directory.

Recommended duplex config:

```json
{
  "chain_id": 1,
  "role": "duplex",
  "identity_key_path": "runtime/node-ed25519.hex",
  "delivery_journal_path": "runtime/novovm-product-delivery-journal.rocksdb",
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
  "resource_limits": {
    "pending_per_peer_count": 1024,
    "pending_per_peer_bytes": 67108864,
    "pending_total_count": 16384,
    "pending_total_bytes": 268435456,
    "pending_ttl_ms": 60000,
    "event_total_bytes": 268435456,
    "preauth_per_peer_count": 64,
    "preauth_per_peer_bytes": 4194304,
    "preauth_total_count": 1024,
    "preauth_total_bytes": 67108864,
    "preauth_ttl_ms": 30000,
    "journal_max_entries": 65536,
    "journal_max_bytes": 536870912,
    "journal_obligation_ttl_ms": 86400000,
    "journal_terminal_retention_ms": 604800000,
    "journal_retry_interval_ms": 1000
  },
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
not close the shared relay or reset another peer's active channel. Every
`pending_by_peer` obligation is admitted atomically against per-peer and global
count/byte limits and a local TTL. Logical payload bytes are shared across
recipients, while each recipient keeps an independent accounting permit. These
worker queues remain in-memory only; the separate Host delivery journal retains
the restart-safe per-recipient obligation. Bounded worker admission must not be
interpreted as durable completion, and relay admission must not remove the Host
journal obligation.

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
  "delivery_journal_path": "runtime/novovm-product-delivery-journal.rocksdb",
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

While the worker process remains alive, the node keeps not-yet-admitted outbound
transactions in separate bounded in-memory queues when a relay session or
individual peer channel is unavailable. One peer succeeding does not clear
another peer's obligation. Each entry is recipient-specific and subject to
per-peer/global count, byte and TTL policy, but is not durable across restart.

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
Relay ingress frame/byte budgets now bound transport abuse. A separate
application-level compute budget for repeated but well-framed semantic
rejections remains open.

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
at most 64 opaque envelopes from each configured remote peer, additionally
bounded by per-peer/global bytes, global count, and local age. It cannot decrypt
or ingest them until the signed handshake succeeds. Overflow, expiry, or a
post-handshake authentication failure isolates that peer and enters its
cooldown; it does not close the shared relay. Authenticated traffic and relay
queues have their own independent limits.

## Relay admission and durable recipient ACK semantics

The `RelayAdmission { admitted: true }` event means the relay returned a
strictly correlated `ForwardOutcome` saying it forwarded the encrypted envelope
to an active target session or accepted it into a bounded in-memory offline/
backpressure queue. At that point the Overlay worker releases its in-memory
queue permit, but the Host journal retains the recipient-specific durable
obligation. Relay admission does not prove that the recipient read the envelope,
opened the E2E frame, persisted journal state, or accepted native ingress.

A distinct signed recipient ACK binds the chain, payload class, object and
payload hashes, stable delivery ID, original sender, recipient and a
journal-evidence timestamp. The wire field is named `accepted_at_ms`, but a
completed duplicate currently reuses its tombstone terminal time; it is not a
universal proof of the original ingress time. The ACK permits only that
matching sender-side outbound recipient obligation to transition to an
outbound-recipient tombstone. It
proves durable Host journal acceptance plus pending-only native ingress
acceptance; it does not prove AOEM execution or persistence, an AOEM receipt, a
candidate block, proof seal, validator vote, QC, fork choice, safe or finalized
status.

The worker queue and the durable journal therefore have different release
points:

```text
accepted ForwardOutcome  -> release worker in-memory permit
verified recipient ACK   -> terminalize Host recipient obligation
AOEM / QC / finality     -> independent lifecycle, never inferred here
```

On the recipient inbound side, ACK relay admission and AOEM execution are
independent branches. The ACK may be emitted before AOEM executes and remains
only pending-ingress evidence. The active inbound record becomes a terminal
tombstone only after both `ack_pending=false` and an AOEM execution receipt is
observable; neither branch alone may compact it.

Physical connections, absolute handshake time, authenticated sessions,
identity/aggregate ingress, active/offline relay queues, node pending queues,
pre-auth buffers, and event channels now have explicit count/byte/time bounds.
Data and control traffic share the declared queue accounts. These are
process-memory safety properties, not a complete delivery guarantee or an
activation-ready production boundary.
Event byte permits cover only runtime-owned channel backlog and are released when ownership moves
to the caller. One `drain_events` call transfers at most 256 events; returned values are caller-owned
and are not falsely counted as still resident in the runtime channel.

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
- A decoded transaction is counted as relay-admitted only after a correlated accepted
  `ForwardOutcome`. A rejected outcome leaves the bounded in-memory obligation
  available for retry; accepted relay ownership removes only the worker-owned
  in-memory item. The Host journal obligation remains until a verified recipient
  ACK or its explicit terminal expiry policy.
- A raw predecode rate/byte rejection or malformed wire closes that relay lifecycle because no
  trustworthy correlation fields exist; the pending obligation remains available for retry.
- `RelayAdmission(admitted=true)` is telemetry for bounded relay admission, not a durable recipient
  ACK, quarantine admission, execution acceptance, vote, QC, or finality.
- A signed `journal_persisted` recipient ACK proves only Host journal/ingress
  acceptance into the pending-only lifecycle. It is not an AOEM execution
  receipt and cannot set proof-sealed, safe or finalized state.
- Journal paths must be operator-configurable and support resolution relative to
  their owning config file. Repository roots, drive letters and workspace-parent
  names are not protocol inputs.
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

`EXPECTED_RECEIVED` and `EXPECTED_DELIVERED` are thresholds for the current
`novovm-node` process counters only. They reset on process restart and must not
be presented as cumulative cross-restart totals. Restart recovery is reported
separately by `journal_recovered_outbound_total` and
`journal_recovered_inbound_total`, together with the recovered journal records.

Required signoff also fails closed while any of the following remains:

```text
journal startup recovery not yet driven
in-memory transaction or recipient-ACK work in flight
active durable outbound recipient obligation
inbound Prepared record
inbound Accepted record with ack_pending=true
all-ACKed fanout whose completion projection is still unobserved
outbound or inbound-Prepared journal expiry observed during the process
```

Consequently, passing the expected per-process counters alone is never enough
to sign off durable delivery or restart recovery.

The final pipeline summary includes `product_mainline_overlay` with lifecycle,
bootstrap, relay, E2E peer count, per-peer in-flight delivery count, ingress,
relay-admission, verified-recipient-ACK delivery, reconnect/rotation, and ownership evidence.

The mainline gate runs local real-WSS tests with node-owned Overlay runtimes.
It verifies signed relay selection, relay identity authentication,
deterministic peer E2E establishment, three-node multiplexing over one relay
session per node, simultaneous bidirectional encrypted NovoRUDP delivery,
single-peer restart recovery without reconnecting the other nodes,
peer-attributable error containment without rotating the healthy shared relay,
failed-candidate cooldown, signed relay rotation, reconnect, and lifecycle
shutdown.

This does not claim a public VPS, cellular, VPN, NAT, CGNAT, Linux package or
self-hosted nightly result. Those remain `NOT EXECUTED` until their own evidence
exists. Local durable recipient ACK/journal coverage also does not claim AOEM
execution, QC/finality, mixed-version rolling upgrade support, or production
activation readiness. LAN nodes must run the same verified release/package;
local drive and data paths may differ.

The exact containment boundary and its negative claims are frozen in
[`NOVOVM_PRODUCT_OVERLAY_MESH_PEER_ERROR_DOMAIN_V1.md`](NOVOVM_PRODUCT_OVERLAY_MESH_PEER_ERROR_DOMAIN_V1.md).
