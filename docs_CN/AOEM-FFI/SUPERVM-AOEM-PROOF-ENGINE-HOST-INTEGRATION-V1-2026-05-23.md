# SUPERVM AOEM Proof Engine Host Integration V1

## 定位

本文只说明 AOEM Proof Engine 在 SUPERVM 中的宿主接入方式。AOEM 在 SUPERVM 中是单层 FULLMAX 包，不是 Proof-only 包。全能力总览请先看：

```text
docs_CN/AOEM-FFI/SUPERVM-AOEM-CAPABILITY-AUDIT-V1-2026-05-23.md
```

SUPERVM 使用 AOEM 的方式是嵌入宿主进程，而不是把 AOEM 部署成独立平台服务。

```text
SUPERVM host
  -> aoem_ffi.dll / libaoem_ffi.so
  -> aoem_execute_ops_wire_v1
  -> compute.zk.resident_proof_v1
  -> aoem_state_read_v1
  -> proof / verify_status / metadata
```

`aoem-proof-worker` 只是可选 adapter / reference host，用于 JSONL 批处理试用或集成前过渡。

## 当前集成包

```text
aoem/
```

当前已包含的 SUPERVM 加载路径：

```text
aoem/windows/core/bin/aoem_ffi.dll
aoem/windows/core/plugins/*.dll
aoem/windows/kms-hsm-plugin/*.dll

aoem/linux/core/bin/libaoem_ffi.so
aoem/linux/core/plugins/*.so
aoem/linux/kms-hsm-plugin/*.so
```

当前平台状态：

```text
Windows:
  included
  installed from AOEM SUPERVM v1.2 FULLMAX bundle
  artifact = D:\WorksArea\AOEM\artifacts\ffi-bundles\fullmax\windows\20260523-181655

Linux:
  included
  installed from AOEM SUPERVM v1.2 FULLMAX bundle
  artifact = D:\WorksArea\AOEM\artifacts\ffi-bundles\fullmax\linux\20260524-103416

macOS:
  pending_rebuild_not_bundled
  old SUPERVM macOS runtime artifacts have been removed from aoem/
```

本地 manifest：

```text
aoem/manifest/aoem-manifest.json
aoem/aoem-sdk-manifest.json
```

## 已接线能力

```text
aoem-bindings:
  aoem_execute_ops_wire_v1 = supported
  aoem_state_read_v1 = supported
  supports_proof_engine_v1 = supported

Proof profiles:
  merkle_membership_v1
  zk_merkle_membership_v1

Resident asset lifecycle:
  setup / list / select / release
```

## 验收

嵌入式宿主 smoke：

```powershell
scripts/aoem/run_proof_engine_host_smoke.ps1
```

期望输出包含：

```text
SUPERVM_AOEM_PROOF_ENGINE_HOST_SMOKE|profile=fixed_profile_v1|proof=ok|verify=ok|state_read=ok|metadata=ok|failures=0
```

如果需要只验证嵌入式宿主而不运行 optional worker adapter：

```powershell
scripts/aoem/run_proof_engine_host_smoke.ps1 -SkipWorkerAdapter
```

## 边界

```text
no new public FFI ABI
Runtime Canon unchanged
Graph OS untouched
dedicated LR untouched
not generic arbitrary-circuit proof
not performance-ready claim
worker adapter is optional, not the AOEM product boundary
macOS runtime artifacts are pending rebuild and not bundled
```
