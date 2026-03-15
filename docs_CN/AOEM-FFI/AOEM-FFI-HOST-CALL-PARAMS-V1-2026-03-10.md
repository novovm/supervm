<!--
Copyright (c) 2026 AOEM SYSTEM TECHNOLOGY
All rights reserved.
Author: AOEM SYSTEM TECHNOLOGY
-->

# AOEM FFI 宿主调用参数总表 v1（2026-03-10）

> 发布口径说明：本文件与 `docs/perf/AOEM-FFI-HOST-CALL-PARAMS-V1-2026-03-10.md` 保持逐项同步。  
> 头文件权威：`crates/ffi/aoem-ffi/include/aoem.h`

## 1. 推荐调用顺序（宿主主线）

1. `aoem_abi_version()` / `aoem_version_string()`
2. `aoem_global_init()`（进程级一次性预热，幂等）
3. `aoem_create()` 或 `aoem_create_with_options()`
4. 执行业务（优先 `aoem_execute_ops_wire_v1`）
5. 失败时读取 `aoem_last_error(handle)`
6. 释放资源：`aoem_free(...)`、`aoem_destroy(handle)`

## 2. 生命周期与基础 API

| API | 关键入参 | 关键出参 | 说明 |
| --- | --- | --- | --- |
| `aoem_abi_version` | 无 | `u32` | ABI 版本探针 |
| `aoem_version_string` | 无 | `const char*` | 版本字符串 |
| `aoem_global_init` | 无 | `i32` | sidecar/能力一次性预热 |
| `aoem_capabilities_json` | 无 | `const char*` | 能力快照 JSON |
| `aoem_create` | 无 | `void*` | 默认 handle |
| `aoem_create_with_options` | `aoem_create_options_v1*` | `void*` | `abi_version=1`, `struct_size=sizeof(...)` |
| `aoem_destroy` | `void* handle` | 无 | 释放 handle |
| `aoem_last_error` | `void* handle` | `const char*` | 最近错误文本 |
| `aoem_free` | `uint8_t* ptr, size_t len` | 无 | 释放 AOEM 分配内存 |

## 3. 执行入口（主路径）

| API | 关键入参 | 关键出参 | 说明 |
| --- | --- | --- | --- |
| `aoem_execute_ops_wire_v1` | `handle, input_ptr, input_len` | `aoem_exec_v2_result*` | 生产推荐入口 |
| `aoem_execute_ops_v2` | `handle, aoem_op_v2*, op_count` | `aoem_exec_v2_result*` | 结构体数组入口 |
| `aoem_execute_batch` | `handle, input_ptr, input_len` | `output_ptr/output_len` | 批量入口，支持 fast-discard |
| `aoem_execute` | `handle, input_ptr, input_len` | `output_ptr/output_len` | 单次入口，生产默认受控 |

### 3.1 `aoem_op_v2` 字段

- `opcode`: `1=read,2=write,3=add_i64,4=inc_i64`
- `key_ptr/key_len`, `value_ptr/value_len`
- `delta: i64`
- `expect_version: u64`（`UINT64_MAX` 表示 None）
- `plan_id: u64`（`0` 表示 auto）

### 3.2 `aoem_execute_ops_wire_v1` 二进制格式

- magic: `"AOV2\0"`（5 bytes）
- `version:u16`（当前 `1`）
- `flags:u16`（预留）
- `op_count:u32`
- repeated ops:
  - `opcode:u8, flags:u8, reserved:u16`
  - `key_len:u32, value_len:u32`
  - `delta:i64, expect_version:u64, plan_id:u64`
  - `key_bytes[key_len], value_bytes[value_len]`

## 4. GPU 通用算子 Primitive API

| API | 关键入参 | 关键出参 | 说明 |
| --- | --- | --- | --- |
| `aoem_execute_primitive_v1` | `primitive_kind, backend_request, vendor_id, values, indices` | `aoem_primitive_result_v1` + wire 输出 | 领域无关统一算子入口（GPU-first + CPU fallback） |

`primitive_kind`：
- `0=sort, 1=scan, 2=scatter, 3=fft, 4=merkle, 5=ntt, 6=gemm`

`backend_request`：
- `0=auto, 1=spirv-vulkan, 2=cuda`

### 4.3 Transformer 子图契约（FFI）

- 当前没有独立 `transformer_*` FFI 符号。
- Transformer 子图通过 `aoem_execute_primitive_v1` 路由。
- 宿主应把它视为 primitive graph profile 配置，而不是单独 ABI 族。

### 4.4 MSM 契约定位（FFI）

- MSM 不作为 `primitive_kind` 独立枚举暴露。
- MSM 走证明/隐私链路的后端契约（例如 `AOEM_STAGE42_MSM_BACKEND`、`AOEM_STAGE42_MSM_BACKEND_STRICT`）。
- 这属于“统一入口 + 专用路由参数”设计，不是能力缺失。

## 5. ZKVM / Crypto / Privacy API

### 5.1 zkVM

| API | 关键入参 | 关键出参 |
| --- | --- | --- |
| `aoem_zkvm_supported` | 无 | `u32` |
| `aoem_zkvm_prove_verify_v1` | `backend, program_ptr/len, witness_ptr/len` | `out_verified` |
| `aoem_zkvm_trace_fib_prove_verify` | `rounds,a,b` | `i32` |

`backend` 枚举：
- `0=auto, 1=trace, 2=risc0, 3=sp1, 4=halo2`

说明（边界口径）：
- `trace/risc0/sp1`：zkVM 路径（VM 语义）。
- `halo2`：AOEM 原生电路后端（Native Circuit），不等同于 zkVM 字节码虚拟机。

### 5.2 ML-DSA

| API | 关键入参 | 关键出参 |
| --- | --- | --- |
| `aoem_mldsa_supported` | 无 | `u32` |
| `aoem_mldsa_pubkey_size/signature_size/secret_key_size` | `level` | `u32` |
| `aoem_mldsa_keygen_v1` | `level` | `pubkey/sk`（AOEM 分配） |
| `aoem_mldsa_sign_v1` | `level, sk, msg` | `signature`（AOEM 分配） |
| `aoem_mldsa_verify` / `aoem_mldsa_verify_auto` | `pk,msg,sig` | `out_valid` |
| `aoem_mldsa_verify_batch_v1` | `items_ptr,item_count` | `out_results,out_valid_count` |

`level`：`44/65/87`（兼容 `2/3/5`）。

### 5.3 经典密码学 / 哈希

| 能力 | API |
| --- | --- |
| Hash | `aoem_sha256_v1` / `aoem_keccak256_v1` / `aoem_blake3_256_v1` |
| Ed25519 | `aoem_ed25519_verify_v1` / `aoem_ed25519_verify_batch_v1` |
| secp256k1 | `aoem_secp256k1_verify_v1` / `aoem_secp256k1_recover_pubkey_v1` |

说明：
- `aoem_ed25519_verify_batch_v1` 返回 bitmap `out_results` + `out_valid_count`。
- `aoem_secp256k1_recover_pubkey_v1` 返回未压缩 SEC1（65 字节）。
- secp 签名格式：65 字节 `[r||s||v]`，`v` 支持 `0/1/27/28`。

### 5.4 隐私与证明

| 能力 | API |
| --- | --- |
| Ring Signature | `aoem_ring_signature_keygen_v1` / `aoem_ring_signature_sign_web30_v1` / `aoem_ring_signature_verify_web30_v1` |
| Ring Signature Batch Verify | `aoem_ring_signature_verify_batch_web30_v1` |
| Groth16 | `aoem_groth16_prove_v1` / `aoem_groth16_prove_batch_v1` / `aoem_groth16_verify_v1` / `aoem_groth16_verify_batch_v1` |
| Bulletproof | `aoem_bulletproof_prove_v1` / `aoem_bulletproof_verify_v1` / `aoem_bulletproof_prove_batch_v1` / `aoem_bulletproof_verify_batch_v1` |
| RingCT | `aoem_ringct_prove_v1` / `aoem_ringct_verify_v1` / `aoem_ringct_prove_batch_v1` / `aoem_ringct_verify_batch_v1` |

## 6. KMS/HSM API

| API | 关键入参 | 关键出参 | 说明 |
| --- | --- | --- | --- |
| `aoem_kms_sign_v1` | `level,key_material,msg` | `signature` | provider 路由 |
| `aoem_hsm_sign_v1` | `level,key_material,msg` | `signature` | provider 路由 |

provider 路由参数：
- `AOEM_FFI_KMS_MODE=local|plugin|none`（默认 `local`）
- `AOEM_FFI_HSM_MODE=local|plugin|none`（默认 `local`）
- 插件发现：
  - `AOEM_FFI_KMS_PLUGIN` / `AOEM_FFI_KMS_PLUGIN_DIR`
  - `AOEM_FFI_HSM_PLUGIN` / `AOEM_FFI_HSM_PLUGIN_DIR`

## 7. 常用宿主环境变量（代码口径）

| 环境变量 | 用途 |
| --- | --- |
| `AOEM_FFI_PERSIST_BACKEND=rocksdb|none` | 持久化后端选择 |
| `AOEM_FFI_PERSIST_PLUGIN{,_DIR}` | persist sidecar 发现 |
| `AOEM_FFI_WASM_RUNTIME=wasmtime` | wasm sidecar 路由 |
| `AOEM_FFI_WASM_PLUGIN{,_DIR}` | wasm sidecar 发现 |
| `AOEM_FFI_ZKVM_MODE=executor` | zkvm sidecar 路由 |
| `AOEM_FFI_ZKVM_PLUGIN{,_DIR}` | zkvm sidecar 发现 |
| `AOEM_HALO2_BACKEND=auto|gpu|cpu` | Halo2 后端请求 |
| `AOEM_HALO2_GPU_MIN_ITEMS=<N>` | Halo2 GPU 最小工作量阈值（默认 `64`） |
| `AOEM_HALO2_GPU_STRICT=0|1` | Halo2 严格模式（GPU 不满足时是否直接失败） |
| `AOEM_HALO2_GPU_PREPROCESS_MODE=auto|identity|fft|cpu` | Halo2 预处理模式；`auto` 常规批次优先 identity，达到 FFT 阈值再试 fft |
| `AOEM_HALO2_GPU_FFT_CHAIN_MIN_ITEMS=<N>` | Halo2 auto 模式触发 FFT 链路的最小批次（默认 `2048`） |
| `AOEM_HALO2_DIGEST_PAR_MIN=<N>` | Halo2 prove digest 并行阈值（默认 `1024`） |
| `AOEM_FFI_MLDSA_MODE=enabled` | ML-DSA sidecar 路由 |
| `AOEM_FFI_MLDSA_PLUGIN{,_DIR}` | ML-DSA sidecar 发现 |
| `AOEM_MLDSA_VERIFY_BATCH_PAR_MIN=<N>` | ML-DSA 批验并行阈值（默认 `128`） |
| `AOEM_ED25519_VERIFY_BATCH_PAR_MIN=<N>` | Ed25519 批验并行阈值（自适应默认：`>=16核:32, >=8核:64, 其他:128`） |
| `AOEM_MLDSA_GPU_NTT_PREPASS=0|1` | ML-DSA 批验 GPU NTT prepass 开关（默认 `1`） |
| `AOEM_MLDSA_GPU_NTT_PREPASS_MIN_ITEMS=<N>` | ML-DSA NTT prepass 最小批次（自适应默认：NVIDIA/AMD=`16`，其他=`32`） |
| `AOEM_MLDSA_GPU_NTT_PREPASS_BUILD_PAR_MIN=<N>` | ML-DSA prepass 主机打包并行阈值（默认 `256`） |
| `AOEM_MLDSA_GPU_NTT_PREPASS_CACHE_CAP=<N>` | ML-DSA prepass 缓存容量（`0` 关闭，默认 `8`） |
| `AOEM_MLDSA_GPU_NTT_PREPASS_STRICT=0|1` | ML-DSA prepass 严格模式 |
| `AOEM_GPU_AUTO_THRESHOLD_ENFORCE=0|1` | 全局自动阈值守卫：`0`=GPU-first，`1`=恢复阈值短路 |
| `AOEM_FFI_KMS_MODE` / `AOEM_FFI_HSM_MODE` | KMS/HSM provider 路由 |
| `AOEM_FFI_RESPONSE_JSON=1` | 兼容 JSON 输出 |
| `AOEM_FFI_ENABLE_SINGLE_EXEC=1` | debug 启用 `aoem_execute` |
| `AOEM_ED25519_VERIFY_BATCH_DEDICATED_MIN=<N>` | Ed25519 dedicated 批验入口阈值（未设置时走常规 serial/parallel） |
| `AOEM_ED25519_VERIFY_BATCH_DEDICATED_AGGREGATION=<N>` | Ed25519 dedicated 分片粒度（默认 `256`） |
| `AOEM_RING_BATCH_BACKEND=auto|gpu|cpu` | Ring batch 后端请求 |
| `AOEM_RING_BATCH_ADAPTIVE=0|1` | Ring batch 自适应策略 |
| `AOEM_STAGE42_VERIFY_MANY_GPU_ASSIST=0|1` | Bulletproof verify-many GPU-assist 开关（默认 `1`） |
| `AOEM_STAGE42_VERIFY_MANY_GPU_ASSIST_THRESHOLD=<N>` | Bulletproof verify-many assist 阈值（默认 `256`） |
| `AOEM_STAGE42_VERIFY_MANY_AGG_PARSE_PAR_THRESHOLD=<N>` | verify-many 聚合 commitment 并行解析阈值（默认 `512`） |
| `AOEM_STAGE42_VERIFY_MANY_PROOF_PARSE_PAR_THRESHOLD=<N>` | verify-many proof 并行解析阈值（默认 `256`） |
| `AOEM_STAGE42_VERIFY_MANY_PROOF_PARSE_CACHE_CAP=<N>` | verify-many proof 解析缓存容量（`0` 关闭，默认 `4096`） |
| `AOEM_STAGE42_VERIFY_MANY_PROOF_BATCH_PARSE_CACHE_CAP=<N>` | verify-many proof-batch 缓存容量（`0` 关闭，默认 `64`） |
| `AOEM_STAGE42_VERIFY_MANY_PROOF_BATCH_PARSE_CACHE_MIN_ITEMS=<N>` | proof-batch 缓存最小批次（默认 `512`） |
| `AOEM_STAGE42_VERIFY_MANY_COMMITMENT_BATCH_PARSE_CACHE_CAP=<N>` | verify-many commitment-batch 缓存容量（`0` 关闭，默认 `64`） |
| `AOEM_STAGE42_VERIFY_MANY_COMMITMENT_BATCH_PARSE_CACHE_MIN_ITEMS=<N>` | commitment-batch 缓存最小批次（默认 `512`） |
| `AOEM_BULLETPROOF_VERIFY_GPU_REQUIRE=0|1` | Bulletproof 批验严格 GPU 契约（默认 `0`） |
| `AOEM_BULLETPROOF_PROVE_BATCH_PARSE_CACHE_CAP=<N>` | Bulletproof prove-batch 解析缓存（`0` 关闭，默认 `32`） |
| `AOEM_BULLETPROOF_VERIFY_BATCH_PARSE_CACHE_CAP=<N>` | Bulletproof verify-batch 解析缓存（`0` 关闭，默认 `32`） |
| `AOEM_BULLETPROOF_VERIFY_BATCH_PARSE_PAR_MIN=<N>` | Bulletproof verify-batch JSON 解码并行阈值（默认 `256`） |
| `AOEM_BULLETPROOF_VERIFY_FALLBACK_PAR_MIN=<N>` | Bulletproof 逐条回退并行阈值（自适应默认：NVIDIA=`128`, AMD=`160`, 其他=`256`） |
| `AOEM_BULLETPROOF_PROVE_BATCH_PAR_MIN=<N>` | Bulletproof prove-batch 并行阈值（默认 `8`） |
| `AOEM_RINGCT_BALANCE_PAR_MIN_TXS=<N>` | RingCT balance 并行阈值（默认 `8`） |
| `AOEM_RINGCT_PROVE_BATCH_PARSE_CACHE_CAP=<N>` | RingCT prove-batch 解析缓存（`0` 关闭，默认 `16`） |
| `AOEM_RINGCT_VERIFY_BATCH_PARSE_CACHE_CAP=<N>` | RingCT verify-batch 解析缓存（`0` 关闭，默认 `16`） |
| `AOEM_RINGCT_PROVE_BATCH_PAR_MIN=<N>` | RingCT prove-batch 并行阈值（默认 `4`） |
| `AOEM_RING_SIGNATURE_VERIFY_BATCH_PARSE_PAR_MIN=<N>` | Ring Signature 批验并行解析阈值（自适应默认：NVIDIA=`32`, AMD=`48`, 其他=`64`） |
| `AOEM_RING_SIGNATURE_VERIFY_BATCH_PARSE_CACHE_CAP=<N>` | Ring Signature 批验解析缓存（`0` 关闭，默认 `32`） |
| `AOEM_RINGCT_PARSE_POINTS_PAR_MIN=<N>` | RingCT commitment 点并行解码阈值（默认 `512`） |
| `AOEM_RINGCT_PARSE_POINTS_CACHE_CAP=<N>` | RingCT 解码点批缓存容量（`0` 关闭，默认 `16`） |
| `AOEM_RINGCT_PARSE_POINTS_CACHE_MIN_POINTS=<N>` | RingCT 点批缓存最小规模（默认 `2048`） |
| `AOEM_RINGCT_COMMITMENT_GPU_BACKEND=auto|gpu|cpu` | RingCT commitment offload 后端请求 |
| `AOEM_GPU_RINGCT_COMMITMENT_MIN_POINTS=<N>` | RingCT commitment dispatch 首选阈值键（默认 `4096`） |
| `AOEM_RINGCT_COMMITMENT_GPU_MIN_POINTS=<N>` | RingCT commitment 历史兼容阈值键 |
| `AOEM_RINGCT_COMMITMENT_GPU_ENCODE_PAR_MIN=<N>` | RingCT commitment 预编码并行阈值（默认 `8192`） |
| `AOEM_RINGCT_COMMITMENT_GPU_ENCODE_CACHE_CAP=<N>` | RingCT commitment 预编码缓存容量（`0` 关闭，默认 `8`） |
| `AOEM_RINGCT_COMMITMENT_GPU_ENCODE_CACHE_MIN_POINTS=<N>` | RingCT commitment 预编码缓存最小规模（默认 `8192`） |
| `AOEM_RINGCT_COMMITMENT_GPU_MODE=auto|hybrid|full` | RingCT commitment 模式：`auto` 默认优先 full，失败退 hybrid；`full` 需要严格 dispatch 证据 |
| `AOEM_RINGCT_COMMITMENT_FULL_GPU_KERNEL=auto|bridge|dedicated` | RingCT full-route 核选择 |
| `AOEM_RINGCT_COMMITMENT_FULL_GPU_DEDICATED_AUTO_MIN_POINTS=<N>` | auto 下切 dedicated 的阈值（默认 `65536`） |
| `AOEM_RINGCT_COMMITMENT_DEDICATED_PAR_MIN_BUCKETS=<N>` | RingCT dedicated bucket 并行阈值（默认 `4096`） |
| `AOEM_RINGCT_COMMITMENT_DEDICATED_PAR_THREADS=<N>` | RingCT dedicated worker 线程数（默认=系统并行度） |
| `AOEM_RINGCT_COMMITMENT_DEDICATED_GPU_READBACK_ONLY=0|1` | RingCT dedicated 只用 GPU 回读证据，跳过 CPU bucket 累加（默认 `1`） |
| `AOEM_RINGCT_COMMITMENT_DEDICATED_FAILS_BEFORE_CACHE=<N>` | RingCT dedicated 连续失败后进入缓存回退阈值（默认 `1`） |
| `AOEM_RINGCT_COMMITMENT_DEDICATED_REPROBE_EVERY=<N>` | RingCT dedicated 缓存回退激活后每 N 次重探（默认 `512`） |
| `AOEM_RINGCT_COMMITMENT_DEDICATED_SHADER_MODE=stable|dedicated` | RingCT dedicated shader 选择 |
| `AOEM_RINGCT_COMMITMENT_DEDICATED_CLASSIC_LIST_MODE=0|1` | RingCT dedicated classic list 模式守卫（默认 `1`） |
| `AOEM_RINGCT_COMMITMENT_DEDICATED_RADIX_DEBUG=0|1` | RingCT dedicated radix debug 开关（默认 `0`） |
| `AOEM_RINGCT_COMMITMENT_DEDICATED_STRICT=0|1` | RingCT dedicated 严格失败策略 |
| `AOEM_RINGCT_COMMITMENT_GPU_STRICT=0|1` | RingCT commitment offload 严格模式 |
| `AOEM_RINGCT_COMMITMENT_GPU_REQUIRE=0|1` | RingCT 批验中要求真实 GPU offload 证据 |
| `AOEM_RINGCT_COMMITMENT_GPU_TRUST_FULL=0|1` | RingCT full-GPU 信任模式控制（`1` 可跳过 CPU `verify_sum_preparsed`，默认性能模式） |
| `AOEM_GROTH16_VERIFY_GPU=auto|gpu|cpu` | Groth16 verify 后端请求 |
| `AOEM_GROTH16_VERIFY_GPU_ASSIST=0|1` | Groth16 verify GPU-assist 开关 |
| `AOEM_GROTH16_VERIFY_GPU_REQUIRE=0|1` | Groth16 verify 严格 GPU 契约 |
| `AOEM_GROTH16_VERIFY_FULL_GPU_REQUIRE=0|1` | Groth16 full-GPU verify 严格契约 |
| `AOEM_GROTH16_VERIFY_FULL_GPU_KERNEL=auto|bridge|dedicated` | Groth16 full-kernel 选择 |
| `AOEM_GROTH16_VERIFY_FULL_GPU_DEDICATED_AUTO_MIN_INPUTS=<N>` | Groth16 dedicated auto 下限（默认 NVIDIA/AMD=`1024`，其他=`16384`） |
| `AOEM_GROTH16_VERIFY_FULL_GPU_DEDICATED_AUTO_MAX_INPUTS=<N>` | Groth16 dedicated auto 上限（默认 NVIDIA/AMD=`65536`，其他=`32768`） |
| `AOEM_GROTH16_VERIFY_FULL_GPU_DEDICATED_VARIANT=auto|small_subgroup|large_nosubgroup|large_subgroup` | Groth16 dedicated 变体选择 |
| `AOEM_GROTH16_VERIFY_FULL_GPU_DEDICATED_LARGE_MIN_INPUTS=<N>` | Groth16 auto 切大输入阈值（默认 NVIDIA/AMD=`2048`，其他=`4096`） |
| `AOEM_GROTH16_VERIFY_FULL_GPU_DEDICATED_LARGE_SUBGROUP_MIN_INPUTS=<N>` | Groth16 auto 切 large subgroup 阈值（默认 NVIDIA/AMD=`8192`，其他=`32768`） |
| `AOEM_GROTH16_VERIFY_GPU_MIN_INPUTS=<N>` | Groth16 assist 最小公开输入数（默认 `8`） |
| `AOEM_GROTH16_VERIFY_VK_CACHE_CAP=<N>` | Groth16 PVK 缓存容量（默认 `64`） |
| `AOEM_GROTH16_VERIFY_GPU_PROBE_CACHE_CAP=<N>` | Groth16 GPU probe 缓存容量（默认 `128`） |
| `AOEM_GROTH16_VERIFY_INPUTS_CACHE_CAP=<N>` | Groth16 public inputs 缓存容量（默认 `256`） |
| `AOEM_GROTH16_VERIFY_PROOFS_CACHE_CAP=<N>` | Groth16 proof 解析缓存容量（默认 `64`） |
| `AOEM_GROTH16_VERIFY_PROOF_PARSE_PAR_MIN=<N>` | Groth16 proof 并行反序列化阈值（默认 `128`） |
| `AOEM_GROTH16_VERIFY_BATCH_PAR_MIN=<N>` | Groth16 verify-batch 并行窗口阈值（默认 `128`） |
| `AOEM_GROTH16_PROVE_CTX_FILE=<path>` | Groth16 proving-context 文件路径（跨进程复用） |
| `AOEM_GROTH16_PROVE_CTX_PERSIST=0|1` | Groth16 proving-context 持久化/加载开关（默认 `0`） |
| `AOEM_GROTH16_PROVE_SELF_VERIFY=0|1` | Groth16 prove 内部自验开关（默认 `0`） |
| `AOEM_GROTH16_PROVE_WITNESS_CACHE_CAP=<N>` | Groth16 witness 解析缓存容量（`0` 关闭，默认 `128`） |
| `AOEM_GROTH16_PROVE_BATCH_PAR_MIN=<N>` | Groth16 prove-batch 并行阈值（默认 `8`） |
| `AOEM_GPU_VENDOR_ID=<hex|dec>` | GPU vendor hint（后端策略选择） |
| `AOEM_STAGE42_MSM_BACKEND=auto|gpu|cpu` | 共享 MSM 后端请求（隐私/证明路径） |
| `AOEM_STAGE42_MSM_BACKEND_STRICT=0|1` | 共享 MSM 严格模式 |
| `AOEM_PRIMITIVE_SORT_PROFILE=<profile>` | Primitive 图谱 profile 选择（含 `graph_transformer_mini_v1`） |
| `AOEM_PRIMITIVE_TRANSFORMER_MINI_AUTO=0|1` | Transformer-mini 自动 profile 选择开关 |
| `AOEM_PRIMITIVE_TRANSFORMER_MINI_MIN_LEN=<N>` | Transformer-mini profile 提升阈值 |

运行时说明（无额外 env）：
- Halo2：AOEM 内部按 `(k,max_proofs)` 维持进程级 setup 缓存，减少重复 keygen/setup。
- Bulletproof：AOEM 内部缓存 `CommitmentGenerator/RangeProofGenerator`，减少每次 prove 构造开销。

release 静默契约：
- `release` 构建下，生产热路径性能/诊断输出编译期关闭；
- 诊断类 env 主要用于 `debug` 性能分析构建，不污染生产快路。

### 7.1 Groth16 Prove Context 推荐配置

- 生产高性能配置：
  - `AOEM_GROTH16_PROVE_CTX_PERSIST=1`
  - `AOEM_GROTH16_PROVE_CTX_FILE=<稳定可写路径>`
  - `AOEM_GROTH16_PROVE_SELF_VERIFY=0`
- 诊断/安全配置：
  - 保持 `AOEM_GROTH16_PROVE_CTX_PERSIST=1`
  - 仅在必要窗口启用 `AOEM_GROTH16_PROVE_SELF_VERIFY=1`

契约说明：
- proving context 持久化仅用于 proving key/context 缓存复用；
- 不改变证明语义与验签规则，只降低冷启动开销。

## 8. 返回码总则

以 `aoem.h` 各函数注释为准，常见语义：
- `0`：调用成功
- `-1`：无效 handle（执行 API 常见）
- `-2`：参数非法 / 输入格式错误
- `-4`：执行/编码/验证错误
- `-5`：能力未构建或后端不可用

失败时建议立即读取 `aoem_last_error(handle)`。

## 9. 相关文档

- `docs-CN/perf/AOEM-FFI-FULLMAX-CAPABILITY-MATRIX-2026-03-12.md`
- `docs/AOEM/OPTIONAL-COMPONENTS-MATRIX-V1-2026-03-09.md`
- `docs/AOEM/OUT-OF-TREE-ADAPTER-INTEGRATION-GUIDE-V1-2026-03-09.md`

