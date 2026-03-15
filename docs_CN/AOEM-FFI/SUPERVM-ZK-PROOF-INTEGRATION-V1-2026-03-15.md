<!--
Copyright (c) 2026 AOEM SYSTEM TECHNOLOGY
All rights reserved.
Author: AOEM SYSTEM TECHNOLOGY
-->

# SuperVM ZK 与 Halo2 集成说明（V1, 2026-03-15）

## 1. 本文目的

本文是 **SuperVM 视角** 的零知识与证明能力说明，回答两个问题：

1. SuperVM 是否真正接入了 Halo2 与零知识证明能力；
2. SuperVM 主线如何调用，不把 AOEM 误判为黑盒。

> 结论先行：**已接入**。SuperVM 通过 D1 接线调用 AOEM FFI，Halo2、zkVM、Groth16、Bulletproof、RingCT 均可在 SuperVM 主线使用。

## 2. SuperVM 调用链（非黑盒）

SuperVM 的证明能力调用链：

`SuperVM Host (D1)`  
`-> novovm-exec / aoem-bindings`  
`-> aoem_ffi (D0)`  
`-> AOEM 后端（Halo2 / Trace / RISC0 / SP1 / Groth16 / Bulletproof / RingCT）`

这条链路是显式可审计的：

- 头文件契约：`supervm/aoem/include/aoem.h`
- 能力清单：`supervm/aoem/manifest/aoem-manifest.json`
- 能力矩阵：`docs_CN/AOEM-FFI/AOEM-FFI-FULLMAX-CAPABILITY-MATRIX-2026-03-12.md`
- 宿主参数表：`docs_CN/AOEM-FFI/AOEM-FFI-HOST-CALL-PARAMS-V1-2026-03-10.md`

## 3. SuperVM 已接入的 ZK/证明能力

| 能力 | SuperVM 状态 | 代表 FFI |
| --- | --- | --- |
| zkVM（Trace/RISC0/SP1） | 已接入 | `aoem_zkvm_prove_verify_v1` |
| Halo2 原生电路（Native Circuit） | 已接入 | `aoem_zkvm_prove_verify_v1`（`backend=halo2`） |
| Groth16 | 已接入 | `aoem_groth16_prove_v1` / `aoem_groth16_verify_v1` / `aoem_groth16_verify_batch_v1` |
| Bulletproof | 已接入 | `aoem_bulletproof_prove_v1` / `aoem_bulletproof_verify_v1` / `aoem_bulletproof_verify_batch_v1` |
| RingCT | 已接入 | `aoem_ringct_prove_v1` / `aoem_ringct_verify_v1` / `aoem_ringct_verify_batch_v1` |
| Ring Signature | 已接入 | `aoem_ring_signature_sign_web30_v1` / `aoem_ring_signature_verify_web30_v1` |

## 4. Halo2 与 zkVM 的边界（必须分开统计）

- `Trace/RISC0/SP1`：zkVM 路径（VM 语义）；
- `Halo2`：原生电路路径（Native Circuit），**不是** zkVM 字节码虚拟机。

这点对外评估必须分开，否则会出现“没写 Halo2”或“只写 zkVM”的误判。

## 5. SuperVM 宿主最常用参数（ZK相关）

### 5.1 选择后端

- `backend=0|1|2|3|4`
- `0=auto, 1=trace, 2=risc0, 3=sp1, 4=halo2`

### 5.2 Halo2

- `AOEM_HALO2_BACKEND=auto|gpu|cpu`
- `AOEM_HALO2_GPU_MIN_ITEMS=<N>`
- `AOEM_HALO2_GPU_STRICT=0|1`
- `AOEM_HALO2_GPU_PREPROCESS_MODE=auto|identity|fft|cpu`
- `AOEM_HALO2_GPU_FFT_CHAIN_MIN_ITEMS=<N>`

### 5.3 Groth16

- `AOEM_GROTH16_VERIFY_GPU=auto|gpu|cpu`
- `AOEM_GROTH16_VERIFY_FULL_GPU_KERNEL=auto|bridge|dedicated`
- `AOEM_GROTH16_VERIFY_GPU_REQUIRE=0|1`
- `AOEM_GROTH16_PROVE_CTX_FILE=<path>`
- `AOEM_GROTH16_PROVE_CTX_PERSIST=0|1`

### 5.4 Bulletproof / RingCT

- `AOEM_STAGE42_VERIFY_MANY_GPU_ASSIST=0|1`
- `AOEM_BULLETPROOF_VERIFY_GPU_REQUIRE=0|1`
- `AOEM_RINGCT_COMMITMENT_GPU_BACKEND=auto|gpu|cpu`
- `AOEM_RINGCT_COMMITMENT_FULL_GPU_KERNEL=auto|bridge|dedicated`
- `AOEM_RINGCT_COMMITMENT_GPU_REQUIRE=0|1`

> 完整参数表见：`AOEM-FFI-HOST-CALL-PARAMS-V1-2026-03-10.md`

## 6. 为什么会被误判“未实现”

常见原因：

1. 只看 SuperVM 旧文档，没有看 AOEM FFI 权威矩阵；
2. 把 Halo2 误当 zkVM 子集；
3. 看到统一入口（如 `aoem_zkvm_prove_verify_v1`）就误以为能力缺失。

正确做法：

先看能力矩阵和宿主参数表，再看 `aoem.h` 的导出符号，三者交叉即可判定。

## 7. 对外发布口径（SuperVM）

对外只引用以下 4 份：

1. `AOEM-INTRODUCTION-V1-2026-03-15.md`
2. `SUPERVM-ZK-PROOF-INTEGRATION-V1-2026-03-15.md`
3. `AOEM-FFI-FULLMAX-CAPABILITY-MATRIX-2026-03-12.md`
4. `AOEM-FFI-HOST-CALL-PARAMS-V1-2026-03-10.md`

避免使用旧过程文档，防止出现“AOEM 黑盒/未实现”的错误结论。
