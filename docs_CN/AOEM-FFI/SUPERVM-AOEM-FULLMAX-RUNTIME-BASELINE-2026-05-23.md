# SUPERVM AOEM FULLMAX Runtime Baseline 2026-05-23

## 冻结结论

```text
AOEM FULLMAX Runtime Baseline 2026-05-23:
Windows included and verified.
Linux included and verified.
macOS pending, not bundled, not advertised as available.
```

这条 baseline 只冻结当前 SUPERVM 可发布 runtime。它不是性能宣传，也不是扩大 AOEM 能力声明。

## 当前可发布 Runtime

```text
platform = windows-x86_64
library  = aoem/windows/core/bin/aoem_ffi.dll
sha256   = 0879941e58f6879ecfcd7f1c49105fe6ba45ddad588550922307b808adf0a49d
source   = D:\WorksArea\AOEM\artifacts\ffi-bundles\fullmax\windows\20260523-181655

platform = linux-x86_64
library  = aoem/linux/core/bin/libaoem_ffi.so
sha256   = d72ea8704b13681b91f23ac6714411046b5965ee5aeda5b6a249bcb0574a05ba
source   = D:\WorksArea\AOEM\artifacts\ffi-bundles\fullmax\linux\20260524-103416
```

Windows 和 Linux runtime 均来自重新生成的 AOEM `SUPERVM v1.2 FULLMAX` bundle，并已确认包含当前 AOEM Proof Engine core。

Linux RISC0 recursion artifact 已单独校验：

```text
file   = C:\Users\leadb\Downloads\ffc503386276f809137161f18d2f3ddcba3bb4b2d8b5d893b2c5d94b35afaf47.zip
sha256 = ffc503386276f809137161f18d2f3ddcba3bb4b2d8b5d893b2c5d94b35afaf47
log    = D:\WorksArea\AOEM\tmp\linux_fullmax_20260524-103416.log
```

## FULLMAX 能力面

该 baseline 保留 AOEM FULLMAX 能力面，并对 Windows/Linux runtime 做发布冻结：

```text
typed execution v2
wire execution v1
state read / write / snapshot
tensor compute
tensor graph / graph runtime internals
primitive operator graph
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

Proof Engine 只是其中一个能力域，不代表 AOEM 只有 proof 能力。

## Pending 平台

```text
macOS:
  status = pending_rebuild_not_bundled
  old runtime removed from aoem/
```

macOS 不在当前可发布 runtime baseline 内。这不是能力否定，而是未进入当前 SUPERVM host package 的发布状态。

## 验收输出

```text
SUPERVM_AOEM_PROOF_ENGINE_HOST_SMOKE|profile=fixed_profile_v1|proof=ok|verify=ok|state_read=ok|metadata=ok|failures=0
cargo check -p aoem-bindings = PASS
cargo check -p novovm-exec = PASS
cargo check -p novovm-node = PASS
JSON manifest parse = PASS
git diff --check = PASS
```

## 不可宣称项

```text
Linux runtime available = true
macOS runtime available = false
generic arbitrary-circuit proof = false
performance-ready claim = false
Graph OS path = false
dedicated LR path = false
public FFI ABI changed = false
Runtime Canon changed = false
```

## 审计入口

联合阅读：

```text
aoem/RUNTIME-BASELINE.md
aoem/manifest/aoem-manifest.json
aoem/aoem-sdk-manifest.json
aoem/CHECKSUMS.sha256
docs_CN/AOEM-FFI/SUPERVM-AOEM-CAPABILITY-AUDIT-V1-2026-05-23.md
docs_CN/AOEM-FFI/AOEM-FFI-FULLMAX-CAPABILITY-MATRIX-2026-03-12.md
```
