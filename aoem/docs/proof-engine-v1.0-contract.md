# AOEM Compute Native Proof Engine v1.0 Contract Candidate

## Security correction (2026-10-04)

`zk_merkle_membership_v1` (profile 3) is withdrawn. The previous AORF
envelope only authenticated its own publicly recomputable digests; it did not
prove knowledge of a private membership witness. An external party could
construct an accepted envelope even for depth zero with root != commitment.
Producer-side witness checks and GPU execution did not fix this verifier gap.

Current op 98 rejects profile 3 in every wire version, before GPU work or state
writes, with rc=-4 and zero successful operations/writes. Op 99 refuses setup
of profile 3 assets. The C verifier returns unsupported with or without a
witness; the worker returns `unsupported_private_membership_proof` and
`proof_written=false`. No witness-disclosure fallback is permitted.

Profiles 1/2 retain diagnostic/public-path checks, **not** a ZK claim. The
historical `proof_verified` field is now false; status explicitly includes
`envelope_integrity_verified=true`, `verification_scope=envelope_integrity_only_not_zk`
and `cryptographic_proof_verified=false`. Consumers requiring genuine ZK must
reject these diagnostic results. AORF v1/v2 must not be used as a
downgrade route for profile 3. Old outputs must not be reused after a failed
call; callers must check the return code and request identity before readback.

`verify_status.accepted` and `fixed_profile_verifier_accepted` are also false;
batch `all_verify_status` and capability status explicitly identify the public
diagnostic scope. Public Merkle `membership_verifier_accepted` still reports the
public path relation, not hidden-witness knowledge.

The profile-3 shape and old acceptance examples below are historical, not an
active security contract. Previously packaged `dist/aoem-compute-native-proof-*`
workers/libraries are not patched in place and remain unsafe for private-proof
acceptance; rebuild from corrected sources. A containment test passing does not
mean genuine private proving is restored.

A replacement must version the relation, hash and envelope, use a real circuit
with independently trusted parameters, and constrain commitment, nullifier,
private index/path and root. The existing multiplication/accumulator circuits
are not that relation. No backend, trusted setup or new hash is silently chosen
by this containment change. Existing GPU primitives, other cryptographic
verifiers and the unified semantic entry are unchanged.

## Identity

```text
product:  AOEM Compute Native Proof Engine v1.0
stage:    public diagnostics only; private proof withdrawn
entry:    aoem_execute_ops_wire_v1
workload: compute.zk.resident_proof_v1
output:   aoem_state_read_v1
worker:   aoem_proof_worker
```

This historical contract packaged v0.9 public and private Merkle membership.
The private-proof acceptance claim is withdrawn; only public diagnostics remain.
The correction does not add a new
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

### `zk_merkle_membership_v1` (withdrawn historical shape)

`zk_merkle_membership_v1` was presented as a private membership path. Its
cryptographic validity/privacy acceptance is withdrawn; the following bytes
are retained only to identify and reject old requests.

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

The old public verifier checked only the envelope and public digests, not the
private relation. Those checks were insufficient against a malicious prover.

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

Historical result (no longer emitted for profile 3):

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

The old private-positive acceptance record is withdrawn. Current security
regression must reject false envelopes with recomputed public digests, with/without witness,
all old envelope/wire versions and asset setup, without successful proof writes.

The following record is historical only, not an active release gate or evidence
of cryptographic soundness:

```text
acceptance/worker-contract-acceptance.json
```

Current containment requirements, checked separately for each shipped platform:

```text
profiles 1/2: public diagnostics, scope=envelope_integrity_only_not_zk
proof_verified=false; cryptographic_proof_verified=false; accepted=false
profile 3 generation: unsupported, no successful operations or writes
profile 3 asset setup: rejected, no registered asset or output
profile 3 external verification: rejected with and without witness
profile 3 worker jobs: unsupported_private_membership_proof; proof_written=false
recomputed-checksum private envelopes and profile relabels rejected
malformed rejected with proof_written=false
public resident asset lifecycle and existing host integration preserved
```

Record unexecuted platform checks explicitly. A passing containment test does
not restore private proving and must not be reported as `privacy=ok` or ZK
success. Default worker examples use `jobs.merkle.jsonl`; private job fixtures
remain negative tests only.

No throughput, latency, or TPS claim is part of this contract.

SUPERVM packages Windows and Linux runtimes; macOS remains unbundled.
The actual source, hashes and test scope are in [RUNTIME-BASELINE.md](../RUNTIME-BASELINE.md).

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
