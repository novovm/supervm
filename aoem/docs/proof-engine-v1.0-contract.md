# AOEM Compute Native Proof Engine v1.0 Contract Candidate

## Identity

```text
product:  AOEM Compute Native Proof Engine v1.0
stage:    production proof engine candidate
entry:    aoem_execute_ops_wire_v1
workload: compute.zk.resident_proof_v1
output:   aoem_state_read_v1
worker:   aoem_proof_worker
```

This contract candidate packages the v0.9 public and private Merkle membership
capabilities as a production-facing proof engine contract. It does not add a new
public FFI ABI, compute op, Runtime Canon path, Graph OS path, or dedicated LR
path.

## Profile Contract

### `merkle_membership_v1`

`merkle_membership_v1` is the public inclusion fast path.

```text
public_input:
  merkle_root: hex32
  leaf_hash:   hex32
  leaf_index:  u64
  tree_depth:  u32 <= 32

witness:
  sibling_path: hex32[tree_depth]
```

The external verifier recomputes the root from public `leaf_hash`,
`sibling_path`, and `leaf_index`, then checks it against `merkle_root`.

This profile is not a zero-knowledge privacy proof.

### `zk_merkle_membership_v1`

`zk_merkle_membership_v1` is the private membership path.

```text
public_input:
  merkle_root:      hex32
  leaf_commitment:  hex32
  nullifier:        hex32
  tree_depth:       u32 <= 32
  hash_profile:     zk_merkle_style_v1

private witness:
  leaf:         hex
  leaf_secret:  hex
  leaf_index:   u64
  sibling_path: hex32[tree_depth]
```

Worker outputs for this profile must not expose:

```text
leaf
sibling_path
leaf_index
raw_private_witness
```

The public verifier checks the proof envelope, public input binding,
public-output binding, commitment/nullifier binding, and tamper rejection without
reading the private witness.

`zk_merkle_membership_v1` does not replace `merkle_membership_v1`.

## Worker Job Contract

Worker input is JSONL. Each line is one job and must match:

```text
schemas/proof_job.schema.json
```

Common fields:

```text
request_id
resident_asset_id
profile_id
public_input
witness
```

The worker may batch adjacent jobs only when they share a compatible profile and
resident asset. Mixed profiles should be split by the caller or rejected/split by
the worker implementation; v1.0 does not require cross-profile batching in one
wire request.

## Worker Result Contract

Worker output is JSONL. Each line must match:

```text
schemas/proof_result.schema.json
```

Successful result:

```json
{
  "request_id": "job-001",
  "status": "ok",
  "profile_id": "zk_merkle_membership_v1",
  "proof": "hex...",
  "verify_status": "ok",
  "public_outputs": "{...}",
  "metadata": "{...}"
}
```

Current worker output encodes `public_outputs` and `metadata` as JSON strings
because they are copied from AOEM state readback responses.

Malformed result:

```json
{
  "request_id": "bad-001",
  "status": "error",
  "error": "malformed_payload",
  "proof_written": false
}
```

Malformed jobs must fail deterministically and must not write pseudo-success
proof output.

## Resident Asset Contract

The proof engine keeps the v0.7 resident asset lifecycle contract:

```text
setup
list
select
release
run proof with resident_asset_id
proof after release rejected
unknown asset rejected
```

The lifecycle workload remains:

```text
compute.zk.resident_asset_lifecycle_v1
```

Proof execution remains:

```text
compute.zk.resident_proof_v1
```

## Acceptance Contract

The v1.0 candidate release must pass the machine-readable acceptance record in:

```text
acceptance/worker-contract-acceptance.json
```

Minimum upstream v1.0 acceptance:

```text
Windows public Merkle worker PASS
Windows private ZK Merkle worker PASS
Linux public Merkle worker PASS
Linux private ZK Merkle worker PASS
standalone verifier PASS
tamper rejected
malformed rejected with proof_written=false
resident asset lifecycle PASS
```

No throughput, latency, or TPS claim is part of this contract.

Current SUPERVM host package note:

```text
Windows FULLMAX runtime = bundled
Linux FULLMAX runtime   = bundled
macOS FULLMAX runtime   = pending_rebuild_not_bundled
```

Do not use this contract to imply macOS runtime availability inside the current
SUPERVM `aoem/` package.

## Non-Claims

```text
not a generic arbitrary-circuit proof system
not a performance-ready claim
not a Graph OS path
not a dedicated LR path
no new public FFI ABI
no Runtime Canon change
no new compute op
```
