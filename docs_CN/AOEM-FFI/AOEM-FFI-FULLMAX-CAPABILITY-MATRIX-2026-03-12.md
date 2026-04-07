<!--
Copyright (c) 2026 AOEM SYSTEM TECHNOLOGY
All rights reserved.
Author: AOEM SYSTEM TECHNOLOGY
-->

# AOEM FFI Fullmax 能力矩阵（2026-03-12）

> 发布口径说明：本文件是 `docs/AOEM-FFI/AOEM-FFI-FULLMAX-CAPABILITY-MATRIX-2026-03-12.md` 的中文权威版本，内容逐项对齐。

## 产物快照

- Release version: `Beta 0.8`
- Windows fullmax: `artifacts/ffi-bundles/fullmax/windows/20260312-070556`
- Linux fullmax: `artifacts/ffi-bundles/fullmax/linux/20260312-070556`
- macOS fullmax: `scripts/build_macos_fullmax_bundle.sh <stamp> "<release_version>"`
- FFI 覆盖审计:
  - `artifacts/aoem-audits/zk-crypto-ffi-coverage/20260312-094222/aoem-zk-crypto-ffi-coverage-summary.json`

## 总结

- FFI 导出符号覆盖审计：`overall_pass=true`，`missing_blocker_count=0`。
- Windows fullmax 特性：
  - `rocksdb-persistence,wasmtime-runtime,privacy-verify,mldsa,zkvm-executor,risc0,sp1,halo2`
- Linux fullmax 特性：
  - `rocksdb-persistence,wasmtime-runtime,privacy-verify,mldsa,zkvm-executor,risc0,sp1,halo2`
- macOS fullmax 特性：
  - `rocksdb-persistence,wasmtime-runtime,privacy-verify,mldsa,zkvm-executor,risc0,sp1,halo2`
- 主线边界说明（NOVOVM）：
  - 本矩阵仅覆盖 AOEM FFI 执行能力。
  - 覆盖层路由治理（`overlay_route_mode/region/relay_*`）归属 NOVOVM 宿主层，不在 AOEM 能力矩阵范围内。

## FFI 能力矩阵

| 能力域（Capability Area） | 代表符号 | Windows fullmax | Linux fullmax | macOS fullmax | 说明 |
| --- | --- | --- | --- | --- | --- |
| 统一入口（Unified Ingress） | `aoem_global_init` | ready | ready | ready | 进程级一次性初始化（幂等），完成能力注册与插件探测预热。 |
| 通用执行（Generic Execution） | `aoem_execute_ops_wire_v1` | ready | ready | ready | 宿主主线二进制执行入口，领域无关（区块链/AI/OS/机器人均可用）。 |
| GPU 通用算子（GPU Generic Primitives） | `aoem_execute_primitive_v1` | ready | ready | ready | 统一 primitive 入口：`sort/scan/scatter/fft/merkle/ntt/gemm`，默认 GPU-first，自适应不可用时回退 CPU。 |
| 持久化可选组件插件（Persistence Optional Plugin Component） | `aoem_ffi_persist(_rocksdb)` | ready | ready | ready | 持久化后端插件（RocksDB）；支持插件化加载/替换/禁用，缺失时按契约降级。 |
| WASM 可选组件插件（WASM Optional Plugin Component） | `aoem_ffi_wasm` | ready | ready | ready | Wasmtime 运行时插件；支持插件化加载与独立关闭，不改变主 ABI。 |
| zkVM 可选组件插件（zkVM Optional Plugin Component） | `aoem_ffi_zkvm(_executor)` | ready | ready | ready | 零知识虚拟机执行后端（Trace/RISC0/SP1）统一路由入口。 |
| 原生电路证明引擎（Native Circuit Proving Engine） | `aoem_zkvm_prove_verify_v1`（`backend=halo2`） | ready | ready | ready | AOEM 原生 Halo2 **零知识证明（Zero-Knowledge Proof, ZKP）**电路 prove/verify 引擎（非 zkVM 字节码虚拟机）。 |
| ML-DSA 可选组件插件（ML-DSA Optional Plugin Component） | `aoem_ffi_mldsa` | ready | ready | ready | 抗量子签名（Post-Quantum Signature）能力，支持 44/65/87 级别与批验。 |
| KMS/HSM 提供方（KMS/HSM Providers） | `aoem_kms_sign_v1` / `aoem_hsm_sign_v1` | ready | ready | ready | 密钥托管/硬件签名提供方路由（`local|plugin|none`），用于密钥隔离与合规签名。 |
| 经典哈希（Classic Hash） | `aoem_sha256_v1` / `aoem_keccak256_v1` / `aoem_blake3_256_v1` | ready | ready | ready | 经典哈希能力，宿主/业务可直接调用，适配链上与通用数据完整性场景。 |
| 经典非对称签名（Classic Asymmetric Signature） | `aoem_ed25519_verify_v1` / `aoem_ed25519_verify_batch_v1` / `aoem_secp256k1_verify_v1` / `aoem_secp256k1_recover_pubkey_v1` | ready | ready | ready | 经典签名验签与公钥恢复能力，含批验路径（Ed25519 batch）。 |
| 环签名（Ring Signature） | `aoem_ring_signature_keygen/sign/verify` + `aoem_ring_signature_verify_batch_web30_v1` | ready | ready | ready | 隐私签名能力（成员匿名证明），提供 Web30 兼容载荷与批验入口。 |
| Groth16（Groth16） | `aoem_groth16_prove_v1/verify_v1/verify_batch_v1` | ready | ready | ready | zk-SNARK（Groth16）证明/验真与批验能力，支持 GPU 路由优化策略。 |
| Bulletproof（Bulletproof） | `aoem_bulletproof_prove_v1/verify_v1` + `aoem_bulletproof_prove_batch_v1/verify_batch_v1` | ready | ready | ready | 范围证明（Range Proof）能力，提供单条与批量 prove/verify。 |
| RingCT（RingCT） | `aoem_ringct_prove_v1/verify_v1` + `aoem_ringct_prove_batch_v1/verify_batch_v1` | ready | ready | ready | 隐私交易证明能力（金额隐藏 + 一致性验证），提供单条与批量路径。 |

## 可选组件插件热插拔级别（Optional Plugin Hot-Plug Level）

| 能力域（Capability Area） | 热插拔级别 | 说明 |
| --- | --- | --- |
| 持久化可选组件插件（Persistence Optional Plugin Component） | 进程级可替换（Process-Level Replaceable） | 替换插件后重建 handle 或重启进程生效；缺失可降级。 |
| WASM 可选组件插件（WASM Optional Plugin Component） | 进程级可替换（Process-Level Replaceable） | 按配置开启/关闭，不改 ABI。 |
| zkVM 可选组件插件（zkVM Optional Plugin Component） | 进程级可替换（Process-Level Replaceable） | 后端插件可更换，能力通过参数和探测路由。 |
| ML-DSA 可选组件插件（ML-DSA Optional Plugin Component） | 进程级可替换（Process-Level Replaceable） | 算法实现可替换，调用 ABI 保持稳定。 |
| KMS/HSM 提供方（KMS/HSM Providers） | 进程级可替换（Process-Level Replaceable） | provider 可在 `local|plugin|none` 间切换，插件缺失时显式失败或禁用。 |

## zkVM 后端矩阵（FFI）

| 后端 | Windows fullmax | Linux fullmax | macOS fullmax | 说明 |
| --- | --- | --- | --- | --- |
| Trace | ready | ready | ready | `aoem_zkvm_trace_fib_prove_verify` |
| Halo2（Native Circuit） | ready | ready | ready | 原生电路 **零知识证明（ZKP）** 后端，非 zkVM 字节码虚拟机；用于 AOEM 自有电路证明路径。 |
| RISC0 | feature-on | ready | feature-on | Windows/macOS 为“已编译能力”，运行期依赖本机后端环境 |
| SP1 | feature-on | ready | feature-on | Windows/macOS 为“已编译能力”，运行期依赖本机后端环境 |

### zkVM 与 Halo2 边界（Boundary）

1. `Trace/RISC0/SP1` 归类为 zkVM 路径（VM 语义）。
2. `Halo2` 在 AOEM 中归类为**原生电路路径（Native Circuit）**，不等同于 zkVM 虚拟机语义。
3. 对外评审时，应将“zkVM 能力”与“原生电路能力”分开统计，避免能力误判。

## GPU 契约口径

- 对外统一算子入口：`aoem_execute_primitive_v1`
- 能力字段：`backend_gpu_path`、`msm_accel`、`msm_backend`、`gpu_adaptive_scope`
- 路由原则：默认自适应，GPU 优先，不可用自动回退 CPU（不改变 ABI）

关键参数（验签/证明路径）：
- `AOEM_RING_BATCH_BACKEND`、`AOEM_RING_BATCH_ADAPTIVE`
- `AOEM_STAGE42_VERIFY_MANY_GPU_ASSIST`、`AOEM_STAGE42_VERIFY_MANY_GPU_ASSIST_THRESHOLD`
- `AOEM_RINGCT_COMMITMENT_GPU_MODE`、`AOEM_RINGCT_COMMITMENT_FULL_GPU_KERNEL`、`AOEM_RINGCT_COMMITMENT_GPU_REQUIRE`
- `AOEM_GROTH16_VERIFY_GPU`、`AOEM_GROTH16_VERIFY_GPU_ASSIST`、`AOEM_GROTH16_VERIFY_GPU_MIN_INPUTS`
- `AOEM_STAGE42_MSM_BACKEND`、`AOEM_STAGE42_MSM_BACKEND_STRICT`

### 通用算子与 MSM/Transformer 关系（FFI）

1. `aoem_execute_primitive_v1` 是**统一算子入口**，主用于 `sort/scan/scatter/fft/merkle/ntt/gemm` 这 7 类计算图语义。
2. 该入口是 **GPU-first + CPU fallback** 路由，不是“只跑 CPU”。
3. MSM 不作为 `primitive_kind` 枚举单独暴露，它走 ZK/隐私证明链路的专用后端参数（例如 `AOEM_STAGE42_MSM_BACKEND`）。
4. Transformer 目前也不单独新增 `transformer_*` FFI 符号，而是通过 primitive graph profile 路由（`AOEM_PRIMITIVE_SORT_PROFILE` 等）。
5. 因此，“未看到独立 MSM/Transformer FFI 符号”不等于未实现，而是统一入口契约设计。

### RingCT full-route kernel 契约

- `AOEM_RINGCT_COMMITMENT_FULL_GPU_KERNEL=auto|bridge|dedicated`
- `auto/bridge`：
  - 稳定桥接核 `ringct_ristretto_commitment_balance_msm_bridge_v1`
- `dedicated`：
  - 专用核 `ringct_ristretto_commitment_balance_dedicated_v1`
  - 配套阶段证据与严格契约
- dedicated 并行参数：
  - `AOEM_RINGCT_COMMITMENT_DEDICATED_PAR_MIN_BUCKETS`
  - `AOEM_RINGCT_COMMITMENT_DEDICATED_PAR_THREADS`

### Transformer 子图（FFI）

- 通过 `aoem_execute_primitive_v1` + primitive profile 路由；
- 不新增独立 `transformer_*` FFI 符号；
- 相关参数：
  - `AOEM_PRIMITIVE_SORT_PROFILE`
  - `AOEM_PRIMITIVE_TRANSFORMER_MINI_AUTO`
  - `AOEM_PRIMITIVE_TRANSFORMER_MINI_MIN_LEN`

## 平台提示

- Linux 上 `librocksdb-sys` 在部分构建路径可能出现 `libclang` 线程加载抖动。
- 当前 fullmax 产物已包含 persist sidecar；如本机构建遇到抖动，建议单独命令先构建 persist，再回填到 bundle。
- `scripts/export_aoem_beta08_bundle.ps1` 为历史兼容脚本，当前标准打包口径是 fullmax core+sidecar。
