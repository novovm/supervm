# NOVOVM Product Node Overlay Runtime v1

`novovm-product-node-overlay` starts the node-side discovery/runtime boundary:
it merges signed bootstrap sources, restores a local cache, verifies every relay
record, selects a relay, and emits a signed strategy receipt. It does not expose
or request a global raw-IP directory.

## Configuration

```json
{
  "cache_path": "/var/lib/novovm/bootstrap-cache.json",
  "trusted_signer_public_keys": [[1, 2, 3]],
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

The signer key value is a 32-byte Ed25519 public-key JSON array. A real manifest
is an object, not the placeholder string above. Its signature, validity window,
record signatures, and `candidate_limit` are checked before anything enters the
candidate pool. Manifests that include a raw directory, require a single
official service, or exceed their minimal candidate disclosure limit are rejected.

Run one bootstrap/selection cycle with:

```bash
novovm-product-node-overlay \
  /etc/novovm/node-overlay.json \
  /etc/novovm/node-ed25519.hex \
  novovm-ed25519:target-peer-id
```

The identity file contains 64 hexadecimal characters representing a 32-byte
Ed25519 secret. It signs the local strategy receipt; it is not sent to bootstrap
sources or relays.

## Boundary

- A valid local cache is preferred as a source but cannot outlive its signed
  manifest expiry.
- Embedded, invite, community, and directory manifests merge only when each
  independently meets the configured signer policy.
- Relay records are self-signed by their relay node key and selected locally.
- Failure enters cooldown and selects the next valid candidate; no candidate
  produces `QueueFallback`.
- Receipts bind the target peer ID and bounded strategy inputs, never payloads.
