# AOEM FULLMAX SUPERVM Host Package

This directory is a single AOEM FULLMAX package for SUPERVM. It is not a
Proof-only sub-package and it is not a separate platform service.

```text
SUPERVM host process
  -> aoem_ffi.dll
  -> AOEM FFI ABI
  -> aoem_execute_ops_wire_v1 and typed AOEM symbols
  -> AOEM state / proof / crypto / primitive outputs
```

## Package Layout

```text
windows/
  core/bin/aoem_ffi.dll
  core/plugins/*.dll
  kms-hsm-plugin/*.dll
  include/aoem.h

linux/
  core/bin/libaoem_ffi.so
  core/plugins/*.so
  kms-hsm-plugin/*.so
  include/aoem.h

host-integration/
examples/
worker-adapter/
schemas/
acceptance/
docs/
bin/windows-x86_64/
bin/linux-x86_64/
manifest/
```

macOS runtime directories are intentionally not bundled in this SUPERVM package
until fresh AOEM FULLMAX artifacts are rebuilt and verified. Old `.dylib`
artifacts were removed so users do not mistake stale platform binaries for
current v1.2 FULLMAX output.

## Current Platform State

```text
Runtime baseline:
  AOEM FULLMAX Runtime Baseline 2026-07-31
  source commit: a951273c
  Windows included and verified
  Linux included and verified
  macOS pending, not bundled, not advertised as available

Windows:
  included
  canonical AOEM FULLMAX bundle
  source: aoem/windows/manifest.json

Linux:
  included
  canonical AOEM FULLMAX bundle
  source: aoem/linux/manifest.json
  core: aoem/linux/core/bin/libaoem_ffi.so

macOS:
  pending_rebuild_not_bundled
  no macOS runtime files are shipped in this package
```

## FULLMAX Capability Domains

The Windows and Linux FULLMAX runtimes keep the AOEM FULLMAX capability surface
together:

```text
ABI lifecycle
capability discovery
typed execution v2
wire execution v1
state read / write / snapshot
tensor compute
tensor graph / graph runtime internals
primitive u32 graph: sort / scan / scatter / fft / merkle / ntt / gemm
GPU generic primitive route
ZK MSM and resident proof pipeline
resident proof v1
resident asset lifecycle
classic hashes
classic signature verification
ring signature
Groth16
Bulletproof
RingCT
RocksDB persistence sidecar
WASM / Wasmtime sidecar
zkVM executor sidecar
native circuit / Halo2 path
ML-DSA sidecar
KMS / HSM sidecar
```

## Semantic Graph V3 Host Boundary

NOVOVM uses the generic AOEM Semantic Graph V3 surface:

```text
aoem_submit_semantic_graph_v3
aoem_bind_semantic_atomic_writer_v1
aoem_storage_provider_wire_v1
```

AOEM owns domain-neutral scheduling, atomic persistence, completion, and
evidence. The SUPERVM/NOVOVM host owns authentication, nonce and chain-domain
validation, transaction semantics, balances, and all product policy. The AOEM
runtime contains no NOVOVM-specific business logic.

## Confidential Transfer Capability

`confidential_transfer_v1` is the SUPERVM host-facing product profile for the
existing AOEM RingCT capability. It is not a new proof system and it does not
change the public FFI ABI.

```text
SUPERVM host
  -> aoem_ringct_prove_v1
  -> aoem_privacy_execute_v1
  -> RingCT transaction payload / verification status
```

Host references:

```text
host-integration/embedded_confidential_transfer_host.c
examples/hosted_confidential_transfer_smoke.c
docs/confidential-transfer-v1.md
```

This profile is separate from the Proof Engine worker profiles. RingCT remains
the FULLMAX confidential-transfer path; `compute.zk.resident_proof_v1` remains
the proof engine path for membership/state proof profiles.

## Proof Engine Capability

The current Windows and Linux FULLMAX core dynamic libraries include the AOEM
Compute Native Proof Engine host integration. The proof path is part of the same
FULLMAX package:

```text
aoem_execute_ops_wire_v1
  -> compute.zk.resident_proof_v1
  -> aoem_state_read_v1
```

Included proof profiles:

```text
merkle_membership_v1
  public Merkle inclusion proof
  fast public path
  not a privacy proof

zk_merkle_membership_v1
  private Merkle membership profile
  hides leaf, sibling_path and leaf_index in public outputs
  does not replace merkle_membership_v1
```

`aoem-proof-worker` is only an optional reference host / worker adapter. Hosts
can embed AOEM directly and do not need to deploy the worker.

## Boundaries

```text
additive public FFI ABI update for Semantic Graph V3
Runtime Canon unchanged
not a standalone AOEM platform service
not a generic arbitrary-circuit proof system
not a performance-ready claim
not a Graph OS path
not a dedicated LR path
macOS runtime artifacts are not bundled until a fresh build is verified
```

Use `aoem-sdk-manifest.json`, `manifest/aoem-manifest.json` and
`docs_CN/AOEM-FFI/SUPERVM-AOEM-CAPABILITY-AUDIT-V1-2026-05-23.md` for audit.
