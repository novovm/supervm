# NOVOVM eth/71 BAL Design After a484a8506

Date: 2026-05-20

Status: design complete, implementation not started.

Type: design / assessment only.

Primary source:

- EIP-8159: https://eips.ethereum.org/EIPS/eip-8159

## Scope

This document defines the design path for full `eth/71` Block Access List support
after the already sealed NOVOVM patches:

- `fbbb5d3` - geth a484a8506 minimal external compatibility.
- `dc66a56a` - remove strategy-specific txpool surface.
- `7023c25d` - layered RLPx session canary diagnostics.
- `85807d0` - UA router RocksDB state isolation and diagnosis hardening.

This patch is intentionally documentation-only. It does not enable `eth/71`,
does not implement Block Access List message handling, and does not change the
current capability guard.

Project wording:

- External brand and technical wording should use `NOVOVM`.
- `SUPERVM` is treated only as the current repository/path/internal historical code name.
- EVM remains a NOVOVM plugin capability.
- The route remains externally standard Ethereum-compatible behavior, internally AOEM-oriented refactoring.

## Current State

Current NOVOVM behavior is the safe guard state:

- Native `novovm-network` has `EthWireVersion::{V66..V70}` and caps the native
  maximum at `eth/70`.
- The gateway RLPx path caps current support at `eth/69`.
- `eth/71` is not advertised by the native or gateway capability builders.
- BAL message codes `0x12` and `0x13` are classified as unsupported-safe.
- Receiving BAL message codes while not supporting `eth/71` logs
  `unsupported_eth71_bal_message` and does not panic.
- Block JSON has a `balHash` serializer hook, but no real BAL metadata source;
  `balHash` is omitted and no synthetic hash is produced.

This state is compatible by downgrade. It must not be described as full
`eth/71` support.

## EIP-8159 / BAL Requirements

EIP-8159 is Draft / Standards Track: Networking as of this design.

It defines `eth/71` as a Block Access List exchange extension and introduces two
new `eth` subprotocol messages:

- `GetBlockAccessLists` with message id `0x12`.
- `BlockAccessLists` with message id `0x13`.

The request form is:

```text
[request-id, [blockhash1, blockhash2, ...]]
```

The response form is:

```text
[request-id, [block-access-list1, block-access-list2, ...]]
```

Unavailable BAL entries are represented as the RLP empty string (`0x80`), and a
response soft limit of 2 MiB should be respected.

The design also has to account for post-Amsterdam block headers containing a
`block-access-list-hash` value. Received BAL payloads must be validated by
checking:

```text
keccak256(rlp.encode(block-access-list)) == header.block_access_list_hash
```

Therefore this is not only a message-id addition. Correct support requires wire
types, canonical RLP, header metadata, BAL source/storage, validation, retention,
rate limiting, block JSON behavior, and capability negotiation.

## EthWireVersion::V71 Placement

Native line:

- Candidate file: `crates/novovm-network/src/eth_fullnode.rs`.
- Candidate enum addition: `EthWireVersion::V71`.
- Candidate max constant: increase only when the full BAL gate is ready.
- Candidate negotiation path: include `71` in
  `eth_rlpx_select_shared_eth_version_v1` only behind the same gate.

Gateway RLPx line:

- Candidate file: `crates/gateways/evm-gateway/src/rpc_gateway_exec_cfg.rs`.
- Current gateway cap is `eth/69`.
- Design choice: either first unify gateway support to `eth/70`, or add `eth/71`
  in one later feature branch only after BAL prerequisites are complete.

Design rule:

```text
Do not advertise eth/71 until all required BAL prerequisites are true.
```

Feature gate shape:

```text
eth71_bal_enabled
&& bal_message_codec_ready
&& bal_metadata_source_ready
&& post_amsterdam_fork_gate_ready
&& bal_validation_ready
&& retention_policy_ready
&& parity_tests_passed
```

If any condition is false, the node must continue to negotiate the currently
supported lower protocol version and retain unsupported-safe handling for
`0x12` / `0x13`.

## Message Types

Future message enum shape:

```rust
enum EthMessage {
    // existing messages
    GetBlockAccessLists {
        request_id: u64,
        block_hashes: Vec<[u8; 32]>,
    },
    BlockAccessLists {
        request_id: u64,
        lists: Vec<BlockAccessListResponseEntry>,
    },
}

enum BlockAccessListResponseEntry {
    Available(BlockAccessList),
    Unavailable,
}
```

Wire ids are `eth` subprotocol ids. In NOVOVM RLPx transport code the final
transport code is the negotiated `eth` offset plus the subprotocol id:

```text
transport_code = eth_offset + 0x12
transport_code = eth_offset + 0x13
```

Message implementation phases:

- Phase 1: type definitions and parser tests only.
- Phase 2: decode-only handling with unsupported-safe retention.
- Phase 3: request/response handling backed by real storage.
- Phase 4: capability advertisement after validation and fixtures pass.

## BAL RLP Encoding / Decoding

The BAL codec must be canonical and deterministic. It cannot be a raw `Vec<u8>`
pass-through in code that claims full support.

Decode requirements:

- Validate the exact RLP shape defined by the active BAL EIP set.
- Validate address lengths and byte-array lengths.
- Validate account changes sorted by address.
- Validate storage slots, storage reads, and storage writes according to the
  protocol ordering requirements.
- Validate block-access-index ordering where present.
- Reject duplicate or malformed entries.
- Reject trailing bytes and non-canonical RLP encodings.

Encode requirements:

- Produce canonical RLP only.
- Preserve deterministic ordering.
- Hash the exact canonical payload used on the wire.
- Do not re-order after hashing.

Hash rule:

```text
bal_hash = keccak256(rlp.encode(block-access-list))
```

Full support requires a fixture suite proving that decoded, re-encoded, and
hashed BAL payloads match geth-compatible expectations.

## BAL Metadata Source

The core design question is where the real `block-access-list-hash` and BAL
payload come from.

Allowed sources:

- Native block header metadata once the header type carries
  `block_access_list_hash`.
- Execution result metadata if NOVOVM/AOEM execution can produce the canonical
  BAL for the block.
- Imported geth-compatible headers that already contain the post-Amsterdam BAL
  hash.
- A consensus/engine payload metadata path, if the active protocol surface
  exposes the field.
- Verified peer-supplied BAL payloads, only after hash validation against the
  header.

Disallowed sources:

- Synthetic `balHash`.
- Zero hash placeholder.
- Hash of an empty BAL unless the protocol rules define that exact value for the
  specific block.
- Gateway-local guesses derived from receipts or logs without canonical BAL
  construction.

Hard rule:

```text
balHash must only be emitted when backed by real block metadata or a verified BAL hash.
```

## balHash / block-access-list-hash Source

Current state:

```text
balHash = omit
```

Transition plan:

```text
Phase A:
  Keep the serializer hook.
  Do not output balHash without a real source.

Phase B:
  If header.block_access_list_hash exists for a post-Amsterdam block:
      output balHash from the header metadata.

Phase C:
  If BAL is fetched from a peer:
      validate keccak256(rlp.encode(bal)) against the header hash.
      store the verified BAL payload and metadata.
      output balHash from the header/source, not from an unverified payload.

Phase D:
  If local execution produces canonical BAL:
      validate it against the header before exposing it externally.
```

Pre-Amsterdam blocks must retain the appropriate pre-fork behavior. The
serializer must not produce post-fork fields before the fork gate.

## Block JSON balHash Transition Plan

Current files with hooks:

- `crates/gateways/evm-gateway/src/rpc_eth_query_helpers.rs`
- `crates/novovm-node/src/mainline_query.rs`

Current behavior:

- Hook exists.
- Real source does not exist.
- Field is omitted.

Future behavior:

- Keep omit semantics when metadata is unavailable.
- Output `balHash` only for blocks whose canonical metadata contains the field.
- Ensure gateway and mainline query use the same metadata source and fork gate.
- Add strict parity fixtures comparing geth-compatible block JSON around the
  Amsterdam boundary.

No design phase may output a synthetic hash to make a diff look better.

## Capability Negotiation

Advertise `eth/71` only if all of the following are complete:

- `EthWireVersion::V71` is implemented in the native line.
- Gateway RLPx has a defined `eth/71` path or deliberately delegates it to the
  native line.
- `GetBlockAccessLists` and `BlockAccessLists` decode and encode exist.
- Request and response handlers exist.
- BAL validation against the header hash exists.
- Real `block-access-list-hash` metadata source exists.
- Fork gating for Amsterdam or the active fork name exists.
- Unavailable/pruned BAL semantics are implemented.
- Response soft limit and rate limit are implemented.
- Strict geth parity fixtures pass.
- Rollback/downgrade behavior is verified.

Downgrade behavior:

```text
If remote supports eth/71 but local BAL feature is disabled:
  negotiate eth/70 or eth/69 according to the current implementation.

If BAL messages arrive while eth/71 is not negotiated:
  classify unsupported-safe.
  log unsupported_eth71_bal_message.
  do not panic.
```

The current guard remains the correct default until the Go conditions are met.

## Validation and Security

Validation:

- Verify BAL hash against the header before storing or serving the BAL.
- Reject malformed RLP and non-canonical encodings.
- Reject unsorted or duplicate BAL entries.
- Check request-id round trips.
- Check response item count matches request expectations.
- Treat unavailable entries as unavailable, not as empty valid BAL payloads.

Security and resource controls:

- Enforce request batch limits.
- Enforce the 2 MiB response soft limit.
- Rate-limit `GetBlockAccessLists` to prevent amplification.
- Bound memory while decoding large BAL responses.
- Bound storage writes for peer-supplied BAL data.
- Track malformed response counts per peer.
- Define disconnect policy separately from soft peer scoring.
- Do not serve pruned data as if it were available.

Failure classes:

- `bal_unavailable`
- `bal_pruned`
- `bal_malformed_rlp`
- `bal_hash_mismatch`
- `bal_response_too_large`
- `bal_rate_limited`
- `bal_unsupported_before_eth71`

## Storage / Retention / Pruning

Storage has to distinguish:

- Header hash source.
- Verified BAL payload.
- Payload availability.
- Payload retention/pruning status.
- Source peer and validation timestamp for diagnostics.

Suggested storage record:

```rust
struct VerifiedBlockAccessListRecord {
    block_hash: [u8; 32],
    block_number: u64,
    header_bal_hash: [u8; 32],
    canonical_rlp: Vec<u8>,
    payload_hash: [u8; 32],
    source: BalSource,
    verified_at_unix_ms: u64,
}
```

Retention policy must be explicit. If a block is known but its BAL was never
available or has been pruned, the response entry should use the EIP unavailable
encoding rather than a fabricated payload.

## Test and Fixture Plan

Unit tests:

- RLP decode accepts valid BAL fixtures.
- RLP decode rejects malformed shape.
- RLP decode rejects non-canonical encodings.
- Sorting validation rejects unsorted account/storage changes.
- `keccak256(rlp.encode(bal))` matches fixture header hash.
- `GetBlockAccessLists` request decode/encode round trips.
- `BlockAccessLists` response decode/encode round trips.
- Unavailable response entry maps to RLP empty string.
- Response soft limit is enforced.
- Rate limit counters are exercised.

Capability tests:

- Disabled feature does not advertise `eth/71`.
- Remote `eth/71` plus local disabled feature negotiates lower version.
- Enabled feature advertises `eth/71` only when all gates are true.
- BAL messages remain unsupported-safe before negotiation.

Integration tests:

- Local controlled geth peer with post-Amsterdam fixtures.
- Request BAL for available block.
- Request BAL for unavailable/pruned block.
- Reject BAL whose hash does not match the header.
- Block JSON `balHash` matches header metadata.
- Strict parity test across pre/post-Amsterdam blocks.

Canary tests:

- Public session canary must report selected capability.
- A run with `eth/71` disabled must show downgrade.
- A run with `eth/71` enabled must show readiness only after local parity passes.

## Implementation Phases

### Phase 0: Current Guard

- Do not advertise `eth/71`.
- Keep BAL `0x12` / `0x13` unsupported-safe.
- Keep `balHash` omitted without real metadata.

### Phase 1: Types and Codecs

- Add draft `EthWireVersion::V71` placement behind a disabled gate.
- Add BAL message data model.
- Add RLP codec and canonical validation.
- Add fixture tests.
- Do not advertise `eth/71`.

### Phase 2: Metadata Source

- Add `block_access_list_hash` to the canonical header/metadata path.
- Add post-Amsterdam fork gate.
- Keep `balHash` omitted unless metadata exists.
- Add block JSON parity tests.

### Phase 3: BAL Storage and Handlers

- Add verified BAL storage.
- Add `GetBlockAccessLists` handler.
- Add `BlockAccessLists` handler.
- Add unavailable/pruned semantics.
- Add response size limit and rate limiting.
- Add hash validation before storing or serving.

### Phase 4: Capability Enablement

- Enable feature flag in controlled environments only.
- Advertise `eth/71` only when all gates pass.
- Run strict geth parity fixtures.
- Run local controlled geth session.
- Run public session canary.
- Keep rollback to `eth/70` / `eth/69`.

## Not Claimed

- No `eth/71` implementation in this patch.
- No BAL wire support enabled.
- No capability advertisement change.
- No real `balHash` source implemented.
- No block JSON `balHash` behavior change.
- No RLPx handshake semantic change.
- No UA RocksDB change.
- No strategy-specific txpool surface.
- No EVM plugin architecture rewrite.

## Diff / Worktree Boundary

This design patch should include only:

- `artifacts/migration/eth71-bal-design-after-a484a8506.md`

It must not stage or modify:

- `crates/gateways/evm-gateway/src/main.rs`
- `crates/novovm-adapter-novovm/src/lib.rs`
- `crates/plugins/evm/core/src/lib.rs`
- any sealed commit content from `fbbb5d3`, `dc66a56a`, `7023c25d`, or `85807d0`

## Go / No-Go Conditions

Go for implementation planning:

- Design review accepts the phased path.
- BAL metadata source owner is identified.
- Fork gate source is identified.
- Test fixture source is identified.
- Storage retention policy is accepted.

No-Go for advertising `eth/71`:

- Missing real `block-access-list-hash` source.
- Missing BAL hash validation.
- Missing RLP canonical validation.
- Missing response size/rate limiting.
- Missing unavailable/pruned semantics.
- Missing strict geth parity fixtures.
- Any remaining need to synthesize `balHash`.

Final rule:

```text
Current guard is the safe state. Move from guard to eth/71 support only after
types, metadata source, validation, storage, tests, and rollback are closed.
```
