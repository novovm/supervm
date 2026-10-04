# AOEM FULLMAX SUPERVM Host Package

This directory is a single AOEM FULLMAX package for SUPERVM. It is not a
Proof-only sub-package and it is not a separate platform service.

Security correction 2026-10-04: private resident profile 3 is withdrawn.
Its old AORF envelope was publicly forgeable, not a ZK proof. Corrected
producers and verifiers reject it; profiles 1/2 are public diagnostics only.
See [the affected contract](docs/proof-engine-v1.0-contract.md#security-correction-2026-10-04)
and [the exact packaged build and test scope](RUNTIME-BASELINE.md).

```text
SUPERVM host process
  -> original novovm-node / novovm-exec / aoem-bindings
  -> AOEM dynamic library and FFI ABI
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
current FULLMAX output.

## Current Platform State

The current source, hashes, build availability and **actual per-platform test
results** are recorded in [RUNTIME-BASELINE.md](RUNTIME-BASELINE.md),
[the Windows manifest](windows/manifest.json) and
[the Linux manifest](linux/manifest.json). This update is scoped private-profile
containment, not whole-product or performance requalification. A bundled file
or a Windows pass does not establish Linux validation; pending results must not
be counted as passed. macOS remains unbundled.

## FULLMAX Capability Domains

The FULLMAX package preserves these capability domains. Presence is not a new
per-platform qualification or proof of NOVOVM business integration:

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
ZK MSM primitives and resident pipeline
resident proof v1: profiles 1/2 diagnostics only; profile 3 withdrawn
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
the FULLMAX confidential-transfer capability; `compute.zk.resident_proof_v1`
now exposes only public diagnostics for profiles 1/2 and rejects profile 3.

This legacy profile requires same-process prove-cache admission. It does not
authorize independently held assets, wallet respend, or historical double-spend
protection. Availability of later classical JSON v2 and its actual product
integration are separate entries in [the SDK manifest](aoem-sdk-manifest.json)
and [runtime baseline](RUNTIME-BASELINE.md), not implied by this legacy example.
ML-DSA support does not make the privacy proofs post-quantum.

## ML-DSA Qualification

The retained ML-DSA implementation is pinned to `mldsa-native 2.0.0`.
This update ran **3 Windows packaged regression tests** covering ML-DSA-44/65/87
positive and negative cases. The 90-vector applicable NIST subset and 9-case
independent interoperability results belong to the historical qualification;
they were not rerun or transferred to the new artifacts by this update.
This is not FIPS certification or a new main-chain authentication/seal sign-off.
Raw/internal ABI framing is unchanged; the Host adds external-pure context
framing exactly once. See [RUNTIME-BASELINE.md](RUNTIME-BASELINE.md) for current
platform results, exclusions and compatibility; notices are in `licenses/mldsa-native/`.

## Proof Engine Capability

The existing unified workload and host integration stay inside the same FULLMAX
package. Their current scope is public diagnostics and private-profile rejection,
not a restored private or complete NOV business proof:

```text
aoem_execute_ops_wire_v1
  -> compute.zk.resident_proof_v1
  -> aoem_state_read_v1
```

Current profile status:

```text
fixed_profile_v1 (profile 1)
  fixed-pipeline envelope diagnostic; not ZK

merkle_membership_v1
  public Merkle path/envelope diagnostic; not ZK

zk_merkle_membership_v1
  withdrawn; unsupported_private_membership_proof
  rejection fixtures only; no successful private proof output
```

Public results must report `verification_scope=envelope_integrity_only_not_zk`,
`envelope_integrity_verified=true`, and `proof_verified=false`,
`cryptographic_proof_verified=false`, `verify_status.accepted=false`.
Neither GPU work nor a valid public envelope authorizes assets or finality.

`aoem-proof-worker` is only an optional reference host / worker adapter. Hosts
can embed AOEM directly and do not need to deploy the worker.

## Boundaries

```text
additive public FFI ABI update for Semantic Graph V3
APFL AI contract now requires versioned sealed model-session assets
not a standalone AOEM platform service
not a generic arbitrary-circuit proof system
not a performance-ready claim
not a Graph OS path
not a dedicated LR path
macOS runtime artifacts are not bundled until a fresh build is verified
```

Use [RUNTIME-BASELINE.md](RUNTIME-BASELINE.md),
[aoem-sdk-manifest.json](aoem-sdk-manifest.json) and
[manifest/aoem-manifest.json](manifest/aoem-manifest.json) for the current scope.
The 2026-05-23 capability audit is historical, not current private-proof acceptance.
