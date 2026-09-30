# AOEM FULLMAX Runtime Baseline 2026-10-01

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
