# NOVOVM Product Node Overlay Runtime v1

2026-11-03 source-candidate operator instructions. Native identity is ML-DSA-65;
the old raw Ed25519/hex-key invocation is retired. This is not evidence that a
new package has been installed or accepted on A/B/C/D.

`novovm-product-node-overlay` starts the node-side discovery/runtime boundary:
it merges signed bootstrap sources, restores a local cache, verifies every relay
record, selects a relay, and emits a signed strategy receipt. It does not expose
or request a global raw-IP directory.

The same signed bootstrap and relay-selection runtime is now also owned by the
`novovm-node` native execution pipeline when
`NOVOVM_PRODUCT_MAINLINE_OVERLAY_ENABLED=true`. See
`NOVOVM_PRODUCT_MAINLINE_OVERLAY_LIFECYCLE_V1.md`. The standalone binary
remains a discovery/receipt diagnostic and does not execute transactions.
The node-owned lifecycle supports a deterministic duplex E2E session, bounded
reconnect backoff, and rotation only among candidates verified from the signed
bootstrap/cache set.
For a mesh, one local relay session multiplexes independent E2E channels and
pending-delivery queues for up to 64 explicitly configured peers; duplicate
peer IDs and duplicate or zero metric IDs are rejected at startup.

## Configuration

```jsonc
{
  "cache_path": "/var/lib/novovm/bootstrap-cache.json",
  "trusted_signer_public_keys": [[ /* approved publisher public key: 1952 byte integers */ ]],
  "minimum_bootstrap_signatures": 1,
  "embedded_sources": [
    {
      "source_kind": "embedded_install",
      "priority": 10,
      "manifest": "<SignedBootstrapManifestV1 JSON object>"
    }
  ],
  "cooldown_base_ms": 2000,
  "cooldown_max_ms": 300000
}
```

These are deliberately non-runnable placeholders, not deployment public keys.
Each signer key is the complete 1,952-byte ML-DSA-65 public-key JSON array.
A real manifest is version 2 / suite 65 and
is an object, not the placeholder string above. Its signature, validity window,
record signatures, and `candidate_limit` are checked before anything enters the
candidate pool. Manifests that include a raw directory, require a single
official service, or exceed their minimal candidate disclosure limit are rejected.

Run one bootstrap/selection cycle with:

```text
novovm-product-node-overlay \
  <approved-overlay-config.json> \
  <approved-nonzero-chain-id> \
  <absolute-protected-transport-key-path> \
  <absolute-approved-AOEM-library-path> \
  <approved-library-sha256> \
  <authenticated-target-peer-id>
```

The identity is an existing protected Transport purpose envelope
(`NVPK0003`, version 3, suite 65), not a raw seed or hex secret. It is loaded by
the original platform custody owner and qualified AOEM facade with the explicit
chain and library hash. It signs the local version-2/suite-65 strategy receipt;
secret material is not sent to bootstrap sources or relays. Missing capability,
wrong purpose/domain, old envelope or verification failure does not select Ed25519.
Use absolute CLI paths; this entry does not rebase them to the config directory.
See [purpose preparation](NOVOVM_PURPOSE_IDENTITY_PREPARATION.md).

## Boundary

- A valid local cache is preferred as a source but cannot outlive its signed
  manifest expiry.
- Embedded, invite, community, and directory manifests merge only when each
  independently meets the configured signer policy.
- Relay records are self-signed by their relay node key and selected locally.
- Failure enters cooldown and selects the next valid candidate; no candidate
  produces `QueueFallback`.
- Receipts bind the target peer ID and bounded strategy inputs, never payloads.
- Incoming signatures use the existing unified-semantic verifier owner. Signing
  and key generation remain existing shared primitive operations, not newly
  claimed semantic-signing operations. Local selection/receipt success is not
  network delivery, consensus finality or an externally verified topology.
