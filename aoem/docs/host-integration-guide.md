# AOEM Proof Engine Host Integration Guide

AOEM Compute Native Proof is an engine library intended to be embedded into a
host system. The host owns its API, queues, storage, authentication, scheduling,
and deployment. AOEM provides the proof execution kernel and stable host-callable
wire path.

```text
host system
  -> load aoem_ffi.dll / libaoem_ffi.so
  -> aoem_execute_ops_wire_v1
  -> compute.zk.resident_proof_v1
  -> aoem_state_read_v1
```

The current SUPERVM package bundles freshly rebuilt Windows and Linux FULLMAX
runtimes. macOS runtime artifacts are pending fresh FULLMAX rebuild and are not
included.

## Primary Mode: Embedded Host

Use embedded mode when the host already has a service, chain worker, rollup
prover, enterprise backend, or application runtime.

```text
host process
  -> business logic
  -> job queue / database / network API owned by host
  -> AOEM dynamic library
  -> proof output returned to host state
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

Use worker adapter mode when a team wants a file-based proof job adapter before
embedding AOEM directly.

```text
jobs.jsonl
  -> aoem-proof-worker
  -> AOEM dynamic library
  -> proofs.jsonl
```

Reference files:

```text
worker-adapter/aoem_proof_worker.c
worker-adapter/examples/jobs.merkle.jsonl
worker-adapter/examples/jobs.zk_merkle.jsonl
worker-adapter/examples/jobs.mixed.jsonl
```

The worker adapter is a host sample. It is not the AOEM runtime itself and not a
required standalone deployment.

## Supported Profiles

```text
merkle_membership_v1
  public inclusion proof
  fast path
  not a privacy proof

zk_merkle_membership_v1
  private membership proof
  hides leaf, sibling_path, and leaf_index from worker outputs
```

Both profiles use:

```text
compute.zk.resident_proof_v1
aoem_resident_proof_contract_v1_le_hex
```

## Boundary

```text
no new public FFI ABI
no Runtime Canon change
no new compute op
no Graph OS path
no dedicated LR path
not a generic arbitrary-circuit proof system
not a performance-ready claim
```
