# AOEM FULLMAX SUPERVM Host Package

## Identity

```text
package: AOEM FULLMAX SUPERVM Host Package
layout:  single-layer fullmax host package
entry:   aoem_execute_ops_wire_v1
output:  aoem_state_read_v1
host:    SUPERVM
stage:   scoped private-profile containment; see per-platform baseline
```

## Platform Status

Current source, hashes, build availability and executed tests are authoritative
in [RUNTIME-BASELINE.md](RUNTIME-BASELINE.md) and the
[Windows](windows/manifest.json) / [Linux](linux/manifest.json) manifests.
Do not reuse older artifact qualifications or infer Linux success from Windows.
Pending platform work is not passed. macOS remains unbundled.

This release withdraws the publicly forgeable resident private profile 3;
profiles 1/2 retain public diagnostics only. It does not restore private ZK or
qualify the whole product. Other AOEM backends remain capabilities, not a newly
enabled backend-specific NOVOVM proof service.

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
Semantic Graph V3
semantic atomic writer v1
storage provider wire v1
state read / write / snapshot
tensor compute
primitive operator graph: sort / scan / scatter / fft / merkle / ntt / gemm
GPU-adaptive primitive route
ZK MSM primitives and resident pipeline
resident proof v1: public diagnostics only; private profile 3 withdrawn
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
That legacy sample requires same-process prove-cache admission; it does not
deliver independently held/respendable assets or main-chain privacy. The current
SDK manifest and baseline separately identify later classical privacy APIs.

## ML-DSA Regression Scope

This update ran 3 Windows packaged tests for ML-DSA-44/65/87 positives and
negatives. The previous 90-vector NIST subset and 9 interoperability cases are
historical qualification, not tests rerun for this release. Neither those records
nor this scoped regression imply FIPS certification, new per-platform acceptance
or main-chain PQ integration. See [RUNTIME-BASELINE.md](RUNTIME-BASELINE.md).

## Proof Engine Capability

```text
compute.zk.resident_proof_v1
compute.zk.resident_asset_lifecycle_v1
aoem_resident_proof_contract_v1_le_hex
```

Current profile status:

```text
fixed_profile_v1
  fixed-pipeline envelope diagnostic; not ZK

merkle_membership_v1
  public path/envelope diagnostic; not ZK

zk_merkle_membership_v1
  withdrawn; unsupported_private_membership_proof
  no successful private proof output; negative fixtures only
```

## Expected Diagnostic / Rejection Result

```text
SUPERVM_AOEM_PROOF_ENGINE_HOST_SMOKE|profile=fixed_profile_v1|scope=not_zk|envelope_integrity_verified=true|cryptographic_proof_verified=false|proof_verified=false|accepted=false|retired_profile3=rejected|retired_profile3_outputs=absent|state_read=ok|metadata=ok|failures=0
```

Default worker examples use public jobs. Their successful rows must report
`verification_scope=envelope_integrity_only_not_zk`, integrity true and
`accepted/proof_verified/cryptographic_proof_verified=false`.
Private fixtures must return `unsupported_private_membership_proof` and
`proof_written=false`, with nonzero worker exit. The worker remains an optional
reference adapter, not a required service; actual platform results are recorded
in the baseline. Historical private-positive acceptance is withdrawn.

## Non-Claims

```text
not a standalone AOEM platform service
not a generic arbitrary-circuit proof system
not a performance-ready claim
not a Graph OS path
not a dedicated LR path
additive public FFI ABI update for Semantic Graph V3
compatibility changes and unqualified capabilities are listed in the baseline
macOS runtime availability is not claimed by this package
```
