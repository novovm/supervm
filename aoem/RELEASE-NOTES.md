# AOEM FULLMAX SUPERVM Host Package

## Identity

```text
package: AOEM FULLMAX SUPERVM Host Package
layout:  single-layer fullmax host package
entry:   aoem_execute_ops_wire_v1
output:  aoem_state_read_v1
host:    SUPERVM
stage:   Windows + Linux FULLMAX included, macOS pending rebuild
```

## Platform Status

```text
Runtime baseline:
  AOEM FULLMAX Runtime Baseline 2026-06-06
  Windows included and verified
  Linux included and verified
  macOS pending, not bundled, not advertised as available

Windows:
  AOEM SUPERVM v1.2 FULLMAX freshly generated and installed
  artifact: D:\WorksArea\AOEM\artifacts\ffi-bundles\fullmax\windows\20260606-161254
  core: aoem/windows/core/bin/aoem_ffi.dll

Linux:
  AOEM SUPERVM v1.2 FULLMAX freshly generated and installed
  artifact: D:\WorksArea\AOEM\artifacts\ffi-bundles\fullmax\linux\20260524-103416
  core: aoem/linux/core/bin/libaoem_ffi.so
  RISC0 recursion artifact: verified

macOS:
  pending_rebuild_not_bundled
  old SUPERVM macOS runtime artifacts removed from this package
```

## Positioning

AOEM is an embeddable execution/proof/crypto engine for SUPERVM host systems.
The host normally loads the AOEM dynamic library and calls the existing FFI ABI
directly. `aoem-proof-worker` is included only as an optional reference adapter.

This package is not a Proof-only package. The Compute Native Proof Engine is
included as one capability domain inside the same FULLMAX package.

## Included Runtime Surface

```text
windows/core/bin/aoem_ffi.dll
windows/core/plugins/*.dll
windows/kms-hsm-plugin/*.dll
windows/include/aoem.h

linux/core/bin/libaoem_ffi.so
linux/core/plugins/*.so
linux/kms-hsm-plugin/*.so
linux/include/aoem.h

host-integration/*.c
examples/*.c
worker-adapter/aoem_proof_worker.c
worker-adapter/examples/*.jsonl
schemas/*.json
acceptance/*.json
docs/*.md
bin/windows-x86_64/aoem-proof-worker.exe
bin/linux-x86_64/aoem-proof-worker
```

macOS dynamic libraries are not included in this package. They must be
materialized from a fresh AOEM FULLMAX platform build before being reintroduced.

## FULLMAX Capability Domains

```text
typed execution v2
wire execution v1
state read / write / snapshot
tensor compute
primitive operator graph: sort / scan / scatter / fft / merkle / ntt / gemm
GPU-adaptive primitive route
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

## Confidential Transfer Host Profile

This package includes `confidential_transfer_v1` as a SUPERVM host-facing
profile over existing AOEM RingCT:

```text
host -> aoem_ringct_prove_v1 -> aoem_privacy_execute_v1
```

Host references:

```text
host-integration/embedded_confidential_transfer_host.c
examples/hosted_confidential_transfer_smoke.c
docs/confidential-transfer-v1.md
```

This is an SDK/host semantic wrapper for RingCT confidential transfers. It does
not add a public FFI ABI, does not change Runtime Canon, does not change the
proof worker default task, and does not add a new ZK circuit.

The confidential-transfer host example defaults to a fast wiring probe. Pass
`--run-prove` to execute the full RingCT generation/verification path.

## Proof Engine Capability

```text
compute.zk.resident_proof_v1
compute.zk.resident_asset_lifecycle_v1
aoem_resident_proof_contract_v1_le_hex
```

Included profiles:

```text
merkle_membership_v1
  public inclusion fast path

zk_merkle_membership_v1
  private membership profile
  hides leaf, sibling_path, and leaf_index from public outputs
```

## Expected Acceptance

```text
SUPERVM_AOEM_PROOF_ENGINE_HOST_SMOKE|profile=fixed_profile_v1|proof=ok|verify=ok|state_read=ok|metadata=ok|failures=0
```

Optional worker adapter acceptance is available for Windows and Linux. The
worker remains a reference host adapter, not a required AOEM service.

## Non-Claims

```text
not a standalone AOEM platform service
not a generic arbitrary-circuit proof system
not a performance-ready claim
not a Graph OS path
not a dedicated LR path
no new public FFI ABI
no Runtime Canon change
macOS runtime availability is not claimed by this package
```
