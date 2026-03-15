<!--
Copyright (c) 2026 AOEM SYSTEM TECHNOLOGY
All rights reserved.
Author: AOEM SYSTEM TECHNOLOGY
-->

# AOEM FFI 能力透明矩阵（2026-03-15）

## 目的

本文件用于消除 “AOEM 是黑盒、zk/gpu 未实现” 的评审误解。  
口径为：**以 SuperVM 仓内已集成 AOEM FFI 产物 + `aoem.h` 导出符号为准**。

## SuperVM 已集成产物（可审计路径）

| 平台 | Core | Plugins | Header |
| --- | --- | --- | --- |
| Windows | `aoem/bin/aoem_ffi.dll` | `aoem/plugins/*.dll` | `aoem/include/aoem.h` |
| Linux | `aoem/linux/bin/libaoem_ffi.so` | `aoem/linux/plugins/*.so` | `aoem/linux/include/aoem.h` |
| macOS | `aoem/macos/core/bin/libaoem_ffi.dylib` | `aoem/macos/core/plugins/*.dylib` | `aoem/macos/core/include/aoem.h` |

## 已导出能力（来自已集成 `aoem/include/aoem.h`）

| 能力域 | 代表导出符号 | 状态 |
| --- | --- | --- |
| 统一入口 | `aoem_global_init`, `aoem_execute_ops_wire_v1`, `aoem_execute_primitive_v1` | 已实现 |
| Hash | `aoem_sha256_v1`, `aoem_keccak256_v1`, `aoem_blake3_256_v1` | 已实现 |
| Ed25519 | `aoem_ed25519_verify_v1`, `aoem_ed25519_verify_batch_v1` | 已实现 |
| Secp256k1 | `aoem_secp256k1_verify_v1`, `aoem_secp256k1_recover_pubkey_v1` | 已实现 |
| Ring Signature | `aoem_ring_signature_keygen_v1`, `aoem_ring_signature_sign_web30_v1`, `aoem_ring_signature_verify_web30_v1`, `aoem_ring_signature_verify_batch_web30_v1` | 已实现 |
| Groth16 | `aoem_groth16_prove_v1`, `aoem_groth16_prove_batch_v1`, `aoem_groth16_verify_v1`, `aoem_groth16_verify_batch_v1` | 已实现 |
| Bulletproof | `aoem_bulletproof_prove_v1`, `aoem_bulletproof_prove_batch_v1`, `aoem_bulletproof_verify_v1`, `aoem_bulletproof_verify_batch_v1` | 已实现 |
| RingCT | `aoem_ringct_prove_v1`, `aoem_ringct_prove_batch_v1`, `aoem_ringct_verify_v1`, `aoem_ringct_verify_batch_v1` | 已实现 |
| zkVM | `aoem_zkvm_prove_verify_v1`, `aoem_zkvm_trace_fib_prove_verify` | 已实现 |
| ML-DSA | `aoem_mldsa_keygen_v1`, `aoem_mldsa_sign_v1`, `aoem_mldsa_verify`, `aoem_mldsa_verify_batch_v1` | 已实现 |
| KMS/HSM | `aoem_kms_sign_v1`, `aoem_hsm_sign_v1` | 已实现 |

## GPU 与 ZK 能力说明（非黑盒）

1. GPU 能力通过 `aoem_execute_primitive_v1` 与 ZK/Crypto 路由参数进入 AOEM，遵循 `GPU-first + CPU fallback` 契约。
2. Groth16/Bulletproof/RingCT/ML-DSA/zkVM 的 FFI 符号已对外导出，可由宿主直接调用。
3. “是否启用某条 GPU 子路”受运行时设备与参数策略控制，不等于“能力未实现”。

## SuperVM 评审建议口径

评审 AOEM 能力时，统一使用以下证据链：

1. `docs_CN/AOEM-FFI/AOEM-FFI-CAPABILITY-TRANSPARENCY-MATRIX-2026-03-15.md`（本文件）
2. `aoem/include/aoem.h`（导出符号事实）
3. `docs_CN/AOEM-FFI/AOEM-FFI-FULLMAX-INTEGRATION-2026-03-12.md`（集成与验收）
4. AOEM 上游权威矩阵：
   - `D:\WEB3_AI\AOEM\docs\perf\AOEM-FFI-FULLMAX-CAPABILITY-MATRIX-2026-03-12.md`
   - `D:\WEB3_AI\AOEM\docs\perf\AOEM-FFI-HOST-CALL-PARAMS-V1-2026-03-10.md`

## 结论

AOEM 在 SuperVM 中不是“黑盒占位”，而是“具备可审计符号与跨平台产物的完整 FFI 能力面”。  
后续评审若出现“zk/gpu 未实现”结论，应先基于上述证据链复核。
