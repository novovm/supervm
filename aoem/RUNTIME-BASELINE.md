# AOEM FULLMAX Runtime Baseline

## Current: 2026-10-04 private-profile containment

The previous `56e9da15` qualification below is historical, not a qualification of
this replacement binary. AOEM code source `1377cd1e7c1d43a4bf81cb16a169319772cf3e84`
withdraws the publicly forgeable op98 private profile 3. Later documentation-only
commits clarify this withdrawal without changing the built source. This is
**containment, not a restored private proof system**.

- Private profile 3: op98 wire versions 1-4 and op99 asset setup reject before
  GPU execution and state outputs; external verification rejects old AORF
  versions 1-3, including publicly recomputed digests and profile relabels.
- Profiles 1/2 remain public envelope/path diagnostics. Outputs explicitly have
  `verification_scope=envelope_integrity_only_not_zk`,
  `envelope_integrity_verified=true`, and proof/cryptographic acceptance false.
  Do not authorize assets, business execution or finality from these envelopes.
- The bundled worker, external verifier and embedded examples share corrected
  sources. Private fixtures are rejection cases, not successful proving demos.
- Original node/exec/bindings, CPU transaction/BFT/persistence paths and AOEM's
  other GPU and cryptographic capabilities are retained. No direct RISC0 product
  service was added; backend presence is not product integration or validation.

### Verified on Windows

Core SHA256: `01779fe4fc77535749d265f9ffc233d9c1486e526d9f5d0dc34fc6d49a056674`.
Canonical FULLMAX core and all sidecars rebuilt; no feature/verification skip.

- AOEM Rust security regression: 5 passed (all request/envelope versions,
  asset setup/output absence, relabel and public-diagnostic control).
- External C forged-envelope regression: 12 private variants rejected, with
  relabel/scope guards and public controls. The same C regression passes in WSL.
- Actual DLL C smoke, verifier, repeated service, worker and asset lifecycle
  pass; private worker returns the required nonzero rejection with no proof.
  Public GPU diagnostic preservation used NVIDIA RTX 5090 Laptop / Vulkan,
  explicitly not GPU business execution, private ZK or performance acceptance.
- Strict SUPERVM Rust/worker smoke passes; resident bindings 32/32 include real
  storage and compute; exec resident 3/3 are facade-contract tests, not another
  actual-backend qualification. Original node release builds.
- Two actual four-node RPC/restart regressions pass: mixed RPC 1.84s and single
  ingress 1.37s, each 7 successful and 1 business-failed finalized transaction.
  Two votes cannot publish a head. Legacy Host execution is not enabled.
- Packaged PQ regression: 3/3, ML-DSA 44/65/87 positives and negatives. This is
  not a rerun of the historical 90-vector/9-interop qualification below.

### Verified on Linux / WSL

Core SHA256: `4d06f36a67f41035661100ac0fd18c67dcae60bf2cdd5df28db33b555c538309`.
Canonical FULLMAX core, 14 core sidecar files and 6 KMS/HSM aliases rebuilt and
synchronized with source/destination hashes equal. Ubuntu 24.04 WSL2, Clang
18.1.3 and release optimization; no features skipped. Terminal-renderer failure
and build retries are preserved in the evidence, not presented as clean first runs.

- Actual new SO rejects op98 wire-v4 profile 3 and op99 setup with the exact
  retirement error, zero processed/success/write counters and all output keys
  absent before/after. External forged-envelope regression rejects 12/12.
- Linux worker rejects all 5 private jobs as unsupported, exit 1, no proof.
- Rust core regressions: bindings resident 32/32, exec facade 3/3, additional
  actual-core compute scope 2/2, ML-DSA 44/65/87 regression 3/3. Total 40 comprises
  13 actual-core and 27 pure/mock tests; this is not all-sidecar qualification.
- No new Linux node build/RPC test, physical Linux install or GPU test was run.
  WSL exposed only llvmpipe, not the target NVIDIA GPU. No CPU fallback is counted
  as GPU proof acceptance. GitHub CI has not yet qualified this package.

macOS remains absent. See platform manifests for exact hashes; never infer a
platform result from the other platform.

Evidence: [containment and host regressions](../artifacts/audit/aoem-private-proof-containment-20261004/verification.json).
No new TPS measurement, production sign-off, full feature requalification,
private asset delivery or NOV business validity proof is claimed. Restoring
private proving requires an independently verifiable, versioned cryptographic
relation; public digests or a renamed profile cannot substitute for it.

## Historical: 2026-10-01 qualification (superseded)

## Status

```text
AOEM FULLMAX Runtime Baseline 2026-10-01
= source commit 56e9da15010490ab54435ba6ab1c226f3d739176
= Windows locally qualified for the tests below
= Linux/WSL locally qualified for the tests below
= macOS pending, not bundled, not advertised as available
```

## Included Runtimes

```text
platform: windows-x86_64
library:  aoem/windows/core/bin/aoem_ffi.dll
sha256:   4de9c21853b4bebf1527f2b7d8461a3f393fcf83263e040408a0f7745b0ed463
source:   aoem/windows/manifest.json

platform: linux-x86_64
library:  aoem/linux/core/bin/libaoem_ffi.so
sha256:   88c3e7888256c6c024b0bd2aa013a75e5b51463b41b314e412a66dc5b8043675
source:   aoem/linux/manifest.json
```

The Windows and Linux runtimes were produced from the same canonical AOEM
FULLMAX source commit. Each platform package contains the generic core runtime
plus persistence, Wasmtime, zkVM, ML-DSA, and KMS/HSM sidecars.

All committed paths are repository-relative. A checkout may live on any drive
or under any workspace directory name.

## NOVOVM Integration Boundary

The current NOVOVM integration requires AOEM's generic Semantic Graph V3 and
RocksDB storage-provider capabilities:

```text
aoem_submit_semantic_graph_v3
aoem_bind_semantic_atomic_writer_v1
aoem_storage_provider_wire_v1
```

AOEM owns domain-neutral scheduling, atomic persistence, completion, and
evidence. NOVOVM remains the owner of authentication, nonce and chain-domain
validation, transaction semantics, balances, and product policy. No
NOVOVM-specific business logic is included in AOEM.

## Verified Acceptance and Scope

```text
Windows/Linux core: applicable official ML-DSA sigVer = 90/90 each
  ML-DSA-44/65/87, external pure + internal raw, 18 positive / 72 negative
  90 prehash/external-mu cases excluded, not counted as passed
Windows/Linux core: independent bidirectional/key-import interop = 9 each
Windows/Linux ML-DSA sidecar: same 90/90 subset and 9 interop cases each
Windows/Linux complete FULLMAX build = PASS (core + 14 plugins + 6 KMS/HSM aliases)
Windows/Linux packaged Host ML-DSA strict tests = 2/2 each
Windows/Linux actual AOEM compute/graph/persistence regression = 3/3 each
Windows actual signed NOV Transfer candidate regression = 5/5
```

The old DLL's rejection of valid ML-DSA-65/87 signatures is no longer an
acceptable test outcome. Host tests require all three official positives and
retain message/context/key/signature tampering and raw/pure framing negatives.
This is not FIPS certification, main-chain PQ authentication, finality TPS,
physical Linux installation acceptance, or requalification of every FULLMAX
feature. The user accepted the independently measured Linux verification
tradeoff; concurrent package builds were not used for new performance claims.

Builds used an archived committed source, not the concurrently modified AOEM
checkout. Empty workspace markers in the isolated optional zkVM/KMS manifests
only prevent Cargo from discovering the outer workspace; all five relevant
workspace/optional/guest lockfiles match the source commit. No RISC0 skip flag
or replacement guest was used. Windows KMS used identical committed source.
The later AOEM classical privacy JSON v2 commit `38f602ec` is NOT in this SDK.

## Compatibility and Known Boundaries

The Host-used ABI remains available, with required ABI=1, semantic graph V3 and
storage-provider checks. This is not a claim that every historical export is
unchanged: upstream removed the unbound `aoem_apfl_native_transfer_execute_v1`
symbol. The separate opcode 114 capability remains present. The existing
network-only opcode 114 fixture fails on both old and new DLLs because its
declared signature length does not match its appended payload; network delivery
alone is not execution success, and this test is not counted as passed.
Separately, the engine's own tiny opcode 114 single/bulk template and two
negative cases pass through both new platform C ABIs (4/4 each). This only checks that
synthetic template contract: its 32-byte token is NOT a cryptographic signature
and is never accepted as NOV authentication or finality/TPS evidence.

APFL AI now uses sealed model-session assets through `compute.ai.sgm_infer_v1`.
Legacy GPU-resident/AOAI-v0 metadata is not carried forward as a current claim;
no AI model or GPU performance was newly qualified by this crypto update.
ML-DSA notices are in `licenses/mldsa-native/`.
The legacy `validate_fullmax_host_package.ps1` still asserts that public ABI
changes are false, which already conflicted with the previous V3 manifest.
It is not reported as passing; this update uses the explicit tests above and
package checksum validation without falsifying the ABI/canon flags.

Legacy `confidential_transfer_v1` remains a same-process prove-cache profile,
not independent wallet spend authorization. The RISC0 1.2.6 guest dependency
still has the previously recorded GHSA-jqq4-c7wq-36h7 risk and is not a qualified
PQ-privacy proof baseline. This update does not claim to fix that issue.

## Pending Platforms

```text
macos-universal:
  status: pending_rebuild_not_bundled
  runtime: not included
```

## Non-Claims

```text
AOEM standalone platform service = false
NOVOVM business semantics inside AOEM = false
generic arbitrary-circuit proof = false
performance-ready claim = false
macOS runtime available = false
```
