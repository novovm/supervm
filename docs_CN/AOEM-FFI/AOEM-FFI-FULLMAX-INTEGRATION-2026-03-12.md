<!--
Copyright (c) 2026 AOEM SYSTEM TECHNOLOGY
All rights reserved.
Author: AOEM SYSTEM TECHNOLOGY
-->

# AOEM FFI Fullmax Integration (2026-03-12)

## 当前唯一版本
- AOEM release version: `Beta 0.8`
- AOEM fullmax stamp: `20260312-070556`

## 产物来源
- AOEM Windows fullmax:
  - `D:\WEB3_AI\AOEM\artifacts\ffi-bundles\fullmax\windows\20260312-070556`
- AOEM Linux fullmax:
  - `D:\WEB3_AI\AOEM\artifacts\ffi-bundles\fullmax\linux\20260312-070556`

## SuperVM 接入位置
- Windows core:
  - `D:\WEB3_AI\SUPERVM\aoem\bin\aoem_ffi.dll`
- Linux core:
  - `D:\WEB3_AI\SUPERVM\aoem\linux\bin\libaoem_ffi.so`
- Windows sidecars:
  - `D:\WEB3_AI\SUPERVM\aoem\plugins\*.dll`
- Linux sidecars:
  - `D:\WEB3_AI\SUPERVM\aoem\linux\plugins\*.so`
- Header:
  - `D:\WEB3_AI\SUPERVM\aoem\include\aoem.h`
- Manifest:
  - `D:\WEB3_AI\SUPERVM\aoem\manifest\aoem-manifest.json`

## 接线验收
- 隐私批能力 smoke:
  - `D:\WEB3_AI\SUPERVM\artifacts\migration\aoem-ffi-privacy-batch-smoke\aoem-ffi-privacy-batch-smoke-summary.json`
  - `overall_pass=true`
- 能力契约快照:
  - `D:\WEB3_AI\SUPERVM\artifacts\migration\capabilities-ffi-bundles-2026-03-12\capability-contract-persist.json`
  - `execute_ops_v2=true`, `zkvm_prove=true`, `zkvm_verify=true`, `mldsa_verify=true`, `ringct_batch_verify=true`

## 清理策略
- 历史 AOEM fullmax bundle 仅保留 `20260312-070556`。
- SuperVM 迁移记录仅保留：
  - `aoem-ffi-privacy-batch-smoke`
  - `capabilities-ffi-bundles-2026-03-12`
