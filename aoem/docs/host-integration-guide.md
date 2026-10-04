# AOEM Proof Engine Host Integration Guide

AOEM is an engine library intended to be embedded into a host system. These
resident-workload references exercise public diagnostics through the existing
wire path; they do not deliver a private or complete host-business ZK proof.
The host owns its API, admission, application integration and deployment. AOEM
retains its domain-neutral execution and persistence responsibilities.

```text
host system
  -> load aoem_ffi.dll / libaoem_ffi.so
  -> aoem_execute_ops_wire_v1
  -> compute.zk.resident_proof_v1
  -> aoem_state_read_v1
```

Current source, hashes, platform availability and executed checks are recorded
in [RUNTIME-BASELINE.md](../RUNTIME-BASELINE.md) and the
[Windows](../windows/manifest.json) / [Linux](../linux/manifest.json) manifests.
Bundled files do not imply completed validation; do not infer Linux acceptance
from Windows. macOS is not included.

## Primary Mode: Embedded Host

Use embedded mode for these SDK diagnostics when the host already has an
application runtime. NOVOVM remains on the original
`novovm-node -> novovm-exec -> aoem-bindings -> AOEM` product path. These samples
must not become another standalone node, proof service or authoritative state.

```text
host process
  -> diagnostic job admission
  -> AOEM dynamic library
  -> public diagnostic output returned to caller
```

Reference sources:

```text
host-integration/embedded_proof_host.c
host-integration/embedded_batch_proof_host.c
host-integration/embedded_asset_lifecycle_host.c
```

These examples use the same exported C ABI as production hosts:

```text
aoem_execute_ops_wire_v1
aoem_state_read_v1
```

## Optional Mode: Worker Adapter

Use worker adapter mode when a team wants a file-based diagnostic adapter before
embedding AOEM directly.

```text
public jobs.merkle.jsonl
  -> aoem-proof-worker
  -> AOEM dynamic library
  -> public-diagnostics.jsonl
```

Reference files:

```text
worker-adapter/aoem_proof_worker.c
worker-adapter/examples/jobs.merkle.jsonl
worker-adapter/examples/jobs.zk_merkle.jsonl
  retired profile 3 rejection fixtures only
worker-adapter/examples/jobs.mixed.jsonl
  public diagnostics plus private-profile rejection
```

The worker adapter is a host sample. It is not the AOEM runtime itself and not a
required standalone deployment.

## Current Profile Status

```text
fixed_profile_v1
  fixed-pipeline envelope diagnostic; not ZK

merkle_membership_v1
  public path/envelope diagnostic; not ZK

zk_merkle_membership_v1
  withdrawn; unsupported_private_membership_proof
  no successful output; no witness-disclosure fallback
```

Profiles 1/2 retain the existing workload and envelope names:

```text
compute.zk.resident_proof_v1
aoem_resident_proof_contract_v1_le_hex
```

Require `verification_scope=envelope_integrity_only_not_zk`,
`envelope_integrity_verified=true`, `proof_verified=false`,
`cryptographic_proof_verified=false` and `verify_status.accepted=false`.
Private fixtures must fail with `proof_written=false` and a nonzero worker exit.
Neither a public checksum, GPU execution nor successful SDK smoke is a private
proof. See the [security correction](proof-engine-v1.0-contract.md#security-correction-2026-10-04).

The current packaged ML-DSA check is a 3-test Windows regression for 44/65/87
positives/negatives. Historical 90-vector/9-interoperability qualifications are
not this update's results; actual platform scope belongs in the runtime baseline.

## Boundary

```text
this guide introduces no new proof entry or separate product service
existing compatibility changes are listed in the runtime baseline
no new compute op
no Graph OS path
no dedicated LR path
not a generic arbitrary-circuit proof system
not a performance-ready claim
```
