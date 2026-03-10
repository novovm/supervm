<!--
Copyright (c) 2026 AOEM SYSTEM TECHNOLOGY
All rights reserved.
Author: AOEM SYSTEM TECHNOLOGY
-->

# AOEM FFI D1 宿主调用参数总表 (V1, 2026-03-10)

## 1. 适用范围
- 适用于 `novovm-node` 生产路径 `ffi_v2`。
- D1 仅负责 ingress 规范化与最薄封送，不做执行语义重建。
- 本表为宿主参数与 FFI 入口契约，性能口径以生产链路为准。

## 2. D1 入口参数（宿主环境变量）

| 参数 | 说明 | 推荐值 |
| --- | --- | --- |
| `NOVOVM_EXEC_PATH` | 执行路径选择 | `ffi_v2` |
| `NOVOVM_D1_INGRESS_MODE` | D1 ingress 模式（auto/ops_wire_v1/ops_v2） | `auto` |
| `NOVOVM_D1_CODEC` | 指定 codec（留空=自动） | 空 |
| `NOVOVM_TX_WIRE_FILE` | tx wire 输入文件（与 `NOVOVM_OPS_WIRE_FILE` 二选一） | 按场景 |
| `NOVOVM_OPS_WIRE_FILE` | ops wire 输入文件（与 `NOVOVM_TX_WIRE_FILE` 二选一） | 按场景 |
| `NOVOVM_ENABLE_HOST_ADMISSION` | 宿主 admission（生产建议关闭） | `0` |
| `NOVOVM_AOEM_DLL` | AOEM 动态库路径 | 指向当前生产 DLL |
| `NOVOVM_AOEM_ROOT` | AOEM 根目录 | `repo/aoem` |
| `NOVOVM_AOEM_MANIFEST` | AOEM manifest 路径 | `aoem/manifest/aoem-manifest.json` |
| `NOVOVM_AOEM_RUNTIME_PROFILE` | AOEM runtime profile 路径 | `aoem/config/aoem-runtime-profile.json` |
| `NOVOVM_AOEM_PLUGIN_DIR` | sidecar 插件目录（可选） | `aoem/plugins` |

## 3. 隐私/证明 FFI 入口（含 batch）

| 能力 | 单条入口 | 批量入口 |
| --- | --- | --- |
| Ring Signature (Web30) | `aoem_ring_signature_verify_web30_v1` | `aoem_ring_signature_verify_batch_web30_v1` |
| Bulletproof | `aoem_bulletproof_prove_v1` / `aoem_bulletproof_verify_v1` | `aoem_bulletproof_prove_batch_v1` / `aoem_bulletproof_verify_batch_v1` |
| RingCT | `aoem_ringct_prove_v1` / `aoem_ringct_verify_v1` | `aoem_ringct_prove_batch_v1` / `aoem_ringct_verify_batch_v1` |

## 4. 宿主 smoke（D1 接线证据）

### 4.1 入口 smoke（生产 ingress）
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/migration/run_ffi_v2_tx_wire_ingress_smoke.ps1
```

### 4.2 隐私 batch FFI smoke（符号 + capability）
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/migration/run_aoem_ffi_privacy_batch_smoke.ps1
```

输出摘要：
- `artifacts/migration/aoem-ffi-privacy-batch-smoke/aoem-ffi-privacy-batch-smoke-summary.json`
- `artifacts/migration/aoem-ffi-privacy-batch-smoke/aoem-ffi-privacy-batch-smoke-summary.md`

## 5. 设计约束
- 不向主链路引入额外日志与诊断分支。
- 仅在加载阶段做符号可用性探测；执行热路径不增加探测开销。
- 生产性能指标以 E2E 与 steady 路径封盘脚本为准，不与诊断口径混用。

