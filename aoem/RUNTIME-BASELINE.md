# AOEM FULLMAX Runtime Baseline 2026-07-31

## Status

```text
AOEM FULLMAX Runtime Baseline 2026-07-31
= source commit a951273c
= Windows included and verified
= Linux included and verified
= macOS pending, not bundled, not advertised as available
```

## Included Runtimes

```text
platform: windows-x86_64
library:  aoem/windows/core/bin/aoem_ffi.dll
sha256:   e84ee50a2b308559a52d9599a2200d16dac3a1de0e2c217c8a44675886e2c788
source:   aoem/windows/manifest.json

platform: linux-x86_64
library:  aoem/linux/core/bin/libaoem_ffi.so
sha256:   bd6f36f63f4194fe000b29ca2ee709c78bf384e83352757307aa9a3f7fe106ea
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

## Verified Acceptance

```text
Windows canonical FULLMAX build = PASS
Windows NOVOVM AOEM ownership tests = 17/17 PASS
Linux canonical FULLMAX build = PASS
Linux required symbol export check = PASS
Linux runtime capability JSON check = PASS
RISC0 guest methods embedded = PASS
JSON manifest parse = PASS
```

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
