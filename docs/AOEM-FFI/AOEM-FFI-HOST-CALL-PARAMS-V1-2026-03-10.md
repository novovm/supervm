<!--
Copyright (c) 2026 AOEM SYSTEM TECHNOLOGY
All rights reserved.
Author: AOEM SYSTEM TECHNOLOGY
-->

# AOEM FFI Host Call Parameters v1 (2026-03-10)

> Release note: this file is the English counterpart of  
> `docs_CN/AOEM-FFI/AOEM-FFI-HOST-CALL-PARAMS-V1-2026-03-10.md`.  
> Header authority: `crates/ffi/aoem-ffi/include/aoem.h`.

## 1. Recommended Host Call Sequence (Mainline)

1. `aoem_abi_version()` / `aoem_version_string()`
2. `aoem_global_init()` (process-level one-time prewarm, idempotent)
3. `aoem_create()` or `aoem_create_with_options()`
4. Run business execution (prefer `aoem_execute_ops_wire_v1`)
5. On failure, read `aoem_last_error(handle)`
6. Release resources: `aoem_free(...)`, `aoem_destroy(handle)`

## 2. Lifecycle and Base APIs

| API | Key Input | Key Output | Notes |
| --- | --- | --- | --- |
| `aoem_abi_version` | none | `u32` | ABI version probe |
| `aoem_version_string` | none | `const char*` | version string |
| `aoem_global_init` | none | `i32` | one-time sidecar/capability prewarm |
| `aoem_capabilities_json` | none | `const char*` | capability snapshot JSON |
| `aoem_create` | none | `void*` | default handle |
| `aoem_create_with_options` | `aoem_create_options_v1*` | `void*` | `abi_version=1`, `struct_size=sizeof(...)` |
| `aoem_destroy` | `void* handle` | none | destroy handle |
| `aoem_last_error` | `void* handle` | `const char*` | last error text |
| `aoem_free` | `uint8_t* ptr, size_t len` | none | free AOEM-owned memory |

## 3. Execution Ingress (Production Path)

| API | Key Input | Key Output | Notes |
| --- | --- | --- | --- |
| `aoem_execute_ops_wire_v1` | `handle, input_ptr, input_len` | `aoem_exec_v2_result*` | recommended production ingress |
| `aoem_execute_ops_v2` | `handle, aoem_op_v2*, op_count` | `aoem_exec_v2_result*` | struct-array ingress |
| `aoem_execute_batch` | `handle, input_ptr, input_len` | `output_ptr/output_len` | batch ingress, supports fast-discard |
| `aoem_execute` | `handle, input_ptr, input_len` | `output_ptr/output_len` | single ingress, controlled in production |

### 3.1 `aoem_op_v2` fields

- `opcode`: `1=read,2=write,3=add_i64,4=inc_i64`
- `key_ptr/key_len`, `value_ptr/value_len`
- `delta: i64`
- `expect_version: u64` (`UINT64_MAX` means None)
- `plan_id: u64` (`0` means auto)

### 3.2 `aoem_execute_ops_wire_v1` binary format

- magic: `"AOV2\0"` (5 bytes)
- `version:u16` (current `1`)
- `flags:u16` (reserved)
- `op_count:u32`
- repeated ops:
  - `opcode:u8, flags:u8, reserved:u16`
  - `key_len:u32, value_len:u32`
  - `delta:i64, expect_version:u64, plan_id:u64`
  - `key_bytes[key_len], value_bytes[value_len]`

## 4. GPU Generic Primitive API

| API | Key Input | Key Output | Notes |
| --- | --- | --- | --- |
| `aoem_execute_primitive_v1` | `primitive_kind, backend_request, vendor_id, values, indices` | `aoem_primitive_result_v1` + wire output | unified domain-agnostic primitive ingress (GPU-first + CPU fallback) |

`primitive_kind`:
- `0=sort, 1=scan, 2=scatter, 3=fft, 4=merkle, 5=ntt, 6=gemm`

`backend_request`:
- `0=auto, 1=spirv-vulkan, 2=cuda`

### 4.1 Transformer Subgraph Contract (FFI)

- No standalone `transformer_*` FFI symbol is exported.
- Transformer subgraph is routed via `aoem_execute_primitive_v1`.
- Host should treat it as primitive graph profile configuration.

### 4.2 MSM Contract Positioning (FFI)

- MSM is not exported as standalone `primitive_kind`.
- MSM is routed through proving/privacy backend parameters (for example `AOEM_STAGE42_MSM_BACKEND` and strict mode).
- This is a unified-ingress contract design, not a capability gap.

## 5. ZKVM / Crypto / Privacy APIs

### 5.1 zkVM

| API | Key Input | Key Output |
| --- | --- | --- |
| `aoem_zkvm_supported` | none | `u32` |
| `aoem_zkvm_prove_verify_v1` | `backend, program_ptr/len, witness_ptr/len` | `out_verified` |
| `aoem_zkvm_trace_fib_prove_verify` | `rounds,a,b` | `i32` |

`backend` enum:
- `0=auto, 1=trace, 2=risc0, 3=sp1, 4=halo2`

Boundary:
- `trace/risc0/sp1`: zkVM path (VM semantics).
- `halo2`: AOEM native circuit backend (Native Circuit), not zkVM bytecode VM semantics.

### 5.2 ML-DSA

| API | Key Input | Key Output |
| --- | --- | --- |
| `aoem_mldsa_supported` | none | `u32` |
| `aoem_mldsa_pubkey_size/signature_size/secret_key_size` | `level` | `u32` |
| `aoem_mldsa_keygen_v1` | `level` | `pubkey/sk` (AOEM alloc) |
| `aoem_mldsa_sign_v1` | `level, sk, msg` | `signature` (AOEM alloc) |
| `aoem_mldsa_verify` / `aoem_mldsa_verify_auto` | `pk,msg,sig` | `out_valid` |
| `aoem_mldsa_verify_batch_v1` | `items_ptr,item_count` | `out_results,out_valid_count` |

`level`: `44/65/87` (compatible with `2/3/5`).

### 5.3 Classic Crypto / Hash

| Capability | API |
| --- | --- |
| Hash | `aoem_sha256_v1` / `aoem_keccak256_v1` / `aoem_blake3_256_v1` |
| Ed25519 | `aoem_ed25519_verify_v1` / `aoem_ed25519_verify_batch_v1` |
| secp256k1 | `aoem_secp256k1_verify_v1` / `aoem_secp256k1_recover_pubkey_v1` |

Notes:
- `aoem_ed25519_verify_batch_v1` returns bitmap `out_results` + `out_valid_count`.
- `aoem_secp256k1_recover_pubkey_v1` returns uncompressed SEC1 (65 bytes).
- secp signature layout: 65 bytes `[r||s||v]`, where `v` supports `0/1/27/28`.

### 5.4 Privacy and Proof

| Capability | API |
| --- | --- |
| Ring Signature | `aoem_ring_signature_keygen_v1` / `aoem_ring_signature_sign_web30_v1` / `aoem_ring_signature_verify_web30_v1` |
| Ring Signature Batch Verify | `aoem_ring_signature_verify_batch_web30_v1` |
| Groth16 | `aoem_groth16_prove_v1` / `aoem_groth16_prove_batch_v1` / `aoem_groth16_verify_v1` / `aoem_groth16_verify_batch_v1` |
| Bulletproof | `aoem_bulletproof_prove_v1` / `aoem_bulletproof_verify_v1` / `aoem_bulletproof_prove_batch_v1` / `aoem_bulletproof_verify_batch_v1` |
| RingCT | `aoem_ringct_prove_v1` / `aoem_ringct_verify_v1` / `aoem_ringct_prove_batch_v1` / `aoem_ringct_verify_batch_v1` |

## 6. KMS/HSM APIs

| API | Key Input | Key Output | Notes |
| --- | --- | --- | --- |
| `aoem_kms_sign_v1` | `level,key_material,msg` | `signature` | provider-routed |
| `aoem_hsm_sign_v1` | `level,key_material,msg` | `signature` | provider-routed |

Provider routing parameters:
- `AOEM_FFI_KMS_MODE=local|plugin|none` (default `local`)
- `AOEM_FFI_HSM_MODE=local|plugin|none` (default `local`)
- Plugin discovery:
  - `AOEM_FFI_KMS_PLUGIN` / `AOEM_FFI_KMS_PLUGIN_DIR`
  - `AOEM_FFI_HSM_PLUGIN` / `AOEM_FFI_HSM_PLUGIN_DIR`

## 7. Common Host Environment Variables (Code-Level Contract)

| Environment Variable | Purpose |
| --- | --- |
| `AOEM_FFI_PERSIST_BACKEND=rocksdb|none` | persistence backend selection |
| `AOEM_FFI_PERSIST_PLUGIN{,_DIR}` | persist sidecar discovery |
| `AOEM_FFI_WASM_RUNTIME=wasmtime` | wasm sidecar runtime routing |
| `AOEM_FFI_WASM_PLUGIN{,_DIR}` | wasm sidecar discovery |
| `AOEM_FFI_ZKVM_MODE=executor` | zkvm sidecar routing |
| `AOEM_FFI_ZKVM_PLUGIN{,_DIR}` | zkvm sidecar discovery |
| `AOEM_HALO2_BACKEND=auto|gpu|cpu` | Halo2 backend request |
| `AOEM_HALO2_GPU_MIN_ITEMS=<N>` | Halo2 GPU minimum workload threshold (default `64`) |
| `AOEM_HALO2_GPU_STRICT=0|1` | Halo2 strict mode |
| `AOEM_HALO2_GPU_PREPROCESS_MODE=auto|identity|fft|cpu` | Halo2 preprocess mode |
| `AOEM_HALO2_GPU_FFT_CHAIN_MIN_ITEMS=<N>` | Halo2 auto-trigger min batch for FFT chain (default `2048`) |
| `AOEM_HALO2_DIGEST_PAR_MIN=<N>` | Halo2 prove digest parallel threshold (default `1024`) |
| `AOEM_FFI_MLDSA_MODE=enabled` | ML-DSA sidecar routing |
| `AOEM_FFI_MLDSA_PLUGIN{,_DIR}` | ML-DSA sidecar discovery |
| `AOEM_MLDSA_VERIFY_BATCH_PAR_MIN=<N>` | ML-DSA batch verify parallel threshold (default `128`) |
| `AOEM_ED25519_VERIFY_BATCH_PAR_MIN=<N>` | Ed25519 batch verify parallel threshold (adaptive defaults by core count) |
| `AOEM_MLDSA_GPU_NTT_PREPASS=0|1` | ML-DSA GPU NTT prepass switch |
| `AOEM_MLDSA_GPU_NTT_PREPASS_MIN_ITEMS=<N>` | ML-DSA prepass min batch |
| `AOEM_MLDSA_GPU_NTT_PREPASS_BUILD_PAR_MIN=<N>` | ML-DSA prepass host packing parallel threshold |
| `AOEM_MLDSA_GPU_NTT_PREPASS_CACHE_CAP=<N>` | ML-DSA prepass cache capacity |
| `AOEM_MLDSA_GPU_NTT_PREPASS_STRICT=0|1` | ML-DSA prepass strict mode |
| `AOEM_GPU_AUTO_THRESHOLD_ENFORCE=0|1` | global adaptive threshold guard |
| `AOEM_FFI_KMS_MODE` / `AOEM_FFI_HSM_MODE` | KMS/HSM provider routing |
| `AOEM_FFI_RESPONSE_JSON=1` | compatibility JSON response |
| `AOEM_FFI_ENABLE_SINGLE_EXEC=1` | debug-enable `aoem_execute` |
| `AOEM_ED25519_VERIFY_BATCH_DEDICATED_MIN=<N>` | Ed25519 dedicated path threshold |
| `AOEM_ED25519_VERIFY_BATCH_DEDICATED_AGGREGATION=<N>` | Ed25519 dedicated shard granularity |
| `AOEM_RING_BATCH_BACKEND=auto|gpu|cpu` | ring batch backend request |
| `AOEM_RING_BATCH_ADAPTIVE=0|1` | ring batch adaptive switch |
| `AOEM_STAGE42_VERIFY_MANY_GPU_ASSIST=0|1` | Bulletproof verify-many GPU assist switch |
| `AOEM_STAGE42_VERIFY_MANY_GPU_ASSIST_THRESHOLD=<N>` | Bulletproof verify-many assist threshold |
| `AOEM_BULLETPROOF_VERIFY_GPU_REQUIRE=0|1` | Bulletproof strict GPU verify contract |
| `AOEM_RINGCT_COMMITMENT_GPU_MODE=auto|hybrid|full` | RingCT commitment mode |
| `AOEM_RINGCT_COMMITMENT_FULL_GPU_KERNEL=auto|bridge|dedicated` | RingCT full-kernel selection |
| `AOEM_RINGCT_COMMITMENT_GPU_REQUIRE=0|1` | RingCT strict GPU evidence requirement |
| `AOEM_GROTH16_VERIFY_GPU=auto|gpu|cpu` | Groth16 verify backend request |
| `AOEM_GROTH16_VERIFY_GPU_ASSIST=0|1` | Groth16 verify GPU-assist switch |
| `AOEM_GROTH16_VERIFY_GPU_REQUIRE=0|1` | Groth16 strict GPU contract |
| `AOEM_GROTH16_VERIFY_FULL_GPU_KERNEL=auto|bridge|dedicated` | Groth16 full-kernel selection |
| `AOEM_GROTH16_VERIFY_GPU_MIN_INPUTS=<N>` | Groth16 assist minimum public inputs |
| `AOEM_GROTH16_PROVE_CTX_FILE=<path>` | Groth16 proving-context file path |
| `AOEM_GROTH16_PROVE_CTX_PERSIST=0|1` | Groth16 proving-context persist/load switch |
| `AOEM_GROTH16_PROVE_SELF_VERIFY=0|1` | Groth16 prove internal self-verify switch |
| `AOEM_STAGE42_MSM_BACKEND=auto|gpu|cpu` | shared MSM backend request |
| `AOEM_STAGE42_MSM_BACKEND_STRICT=0|1` | shared MSM strict mode |
| `AOEM_PRIMITIVE_SORT_PROFILE=<profile>` | primitive graph profile routing |
| `AOEM_PRIMITIVE_TRANSFORMER_MINI_AUTO=0|1` | transformer-mini profile auto switch |
| `AOEM_PRIMITIVE_TRANSFORMER_MINI_MIN_LEN=<N>` | transformer-mini profile threshold |

Runtime notes (no extra env required):
- Halo2: AOEM keeps process-level setup cache by `(k,max_proofs)` to reduce repeated keygen/setup.
- Bulletproof: AOEM caches `CommitmentGenerator/RangeProofGenerator` to reduce repeated prove construction overhead.

Release silent contract:
- In `release` builds, diagnostic/perf trace output in production hot-path is compile-time disabled.
- Diagnostic env knobs are intended for debug/perf analysis build, not production fast path.

### 7.1 Groth16 Prove Context Recommended Settings

- Production high-performance:
  - `AOEM_GROTH16_PROVE_CTX_PERSIST=1`
  - `AOEM_GROTH16_PROVE_CTX_FILE=<stable writable path>`
  - `AOEM_GROTH16_PROVE_SELF_VERIFY=0`
- Diagnostic/safety mode:
  - Keep `AOEM_GROTH16_PROVE_CTX_PERSIST=1`
  - Enable `AOEM_GROTH16_PROVE_SELF_VERIFY=1` only in controlled windows

Contract note:
- proving context persistence is only for proving key/context cache reuse.
- It does not alter proof semantics or verification rules.

## 8. Return Code Rules

Use `aoem.h` function comments as the authority. Common semantics:
- `0`: success
- `-1`: invalid handle (common for execution APIs)
- `-2`: invalid parameter / bad input format
- `-4`: execution/encoding/verification error
- `-5`: capability not built or backend unavailable

On failure, read `aoem_last_error(handle)` immediately.

## 9. Related Documents

- `docs/AOEM-FFI/AOEM-FFI-FULLMAX-CAPABILITY-MATRIX-2026-03-12.md`
- `docs_CN/AOEM-FFI/AOEM-FFI-HOST-CALL-PARAMS-V1-2026-03-10.md`
- `docs_CN/AOEM-FFI/AOEM-FFI-FULLMAX-CAPABILITY-MATRIX-2026-03-12.md`

