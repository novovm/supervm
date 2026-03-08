// Copyright (c) 2026 AOEM SYSTEM TECHNOLOGY
// All rights reserved.
// Author: AOEM SYSTEM TECHNOLOGY

#ifndef AOEM_H
#define AOEM_H

#include <stddef.h>
#include <stdint.h>

#ifdef _WIN32
  #ifdef AOEM_EXPORTS
    #define AOEM_API __declspec(dllexport)
  #else
    #define AOEM_API __declspec(dllimport)
  #endif
#else
  #define AOEM_API
#endif

#ifdef __cplusplus
extern "C" {
#endif

typedef struct aoem_create_options_v1 {
  uint32_t abi_version;   // must be 1
  uint32_t struct_size;   // sizeof(aoem_create_options_v1)
  uint32_t ingress_workers; // 0 => keep default
  uint32_t flags;         // reserved, must be 0
} aoem_create_options_v1;

typedef struct aoem_op_v2 {
  uint8_t opcode;         // 1=read,2=write,3=add_i64,4=inc_i64
  uint8_t flags;          // reserved
  uint16_t reserved;      // reserved
  const uint8_t* key_ptr;
  uint32_t key_len;
  const uint8_t* value_ptr;
  uint32_t value_len;
  int64_t delta;
  uint64_t expect_version; // UINT64_MAX means None
  uint64_t plan_id;        // 0 => auto
} aoem_op_v2;

typedef struct aoem_exec_v2_result {
  uint32_t processed;
  uint32_t success;
  uint32_t failed_index;  // UINT32_MAX means none
  uint64_t total_writes;
} aoem_exec_v2_result;

AOEM_API uint32_t aoem_abi_version(void);
AOEM_API const char* aoem_version_string(void);
// Process-level one-time warmup entry.
// Call once at process startup to pre-resolve capabilities and optional sidecar plugins.
AOEM_API int32_t aoem_global_init(void);
AOEM_API const char* aoem_capabilities_json(void);
// Persist delegation (runtime plugin model):
// - If AOEM_PERSISTENCE_PATH is set and non-empty, core AOEM FFI will
//   attempt to load a persist sidecar plugin (e.g. aoem_ffi_persist).
// - Optional backend selector:
//   AOEM_FFI_PERSIST_BACKEND=rocksdb|none   (default: rocksdb)
// - Optional plugin discovery env:
//   AOEM_FFI_PERSIST_PLUGIN=<absolute or relative plugin path>
//   AOEM_FFI_PERSIST_PLUGIN_DIR=<directory containing plugin binary>
// - If plugin load/probe fails, AOEM degrades to local in-memory path.
// WASM runtime delegation (runtime plugin model):
// - AOEM_FFI_WASM_RUNTIME=wasmtime   (default: none)
// - Optional plugin discovery env:
//   AOEM_FFI_WASM_PLUGIN=<absolute or relative plugin path>
//   AOEM_FFI_WASM_PLUGIN_DIR=<directory containing plugin binary>
// - If plugin load/probe fails, AOEM degrades to local runtime path.
// zkVM delegation (runtime plugin model):
// - AOEM_FFI_ZKVM_MODE=executor      (default: none)
// - Optional plugin discovery env:
//   AOEM_FFI_ZKVM_PLUGIN=<absolute or relative plugin path>
//   AOEM_FFI_ZKVM_PLUGIN_DIR=<directory containing plugin binary>
// - If plugin load/probe fails, aoem_zkvm_* returns capability-not-built semantics.
// ML-DSA delegation (runtime plugin model):
// - AOEM_FFI_MLDSA_MODE=enabled      (default: none)
// - Optional plugin discovery env:
//   AOEM_FFI_MLDSA_PLUGIN=<absolute or relative plugin path>
//   AOEM_FFI_MLDSA_PLUGIN_DIR=<directory containing plugin binary>
// - If plugin load/probe fails, aoem_mldsa_* returns capability-not-built semantics.
// zkVM optional capability probe.
AOEM_API uint32_t aoem_zkvm_supported(void);
// Minimal host-side zkVM prove+verify roundtrip probe (Trace/Fibonacci).
// return code:
//  1 = prove+verify succeeded
//  0 = verify returned false
// -2 = prove failed
// -3 = verify error
// -5 = capability not built (zkvm-executor feature disabled)
AOEM_API int32_t aoem_zkvm_trace_fib_prove_verify(
  uint32_t rounds,
  uint64_t witness_a,
  uint64_t witness_b
);
// ML-DSA optional capability.
// level values: 44 (ML-DSA-44), 65 (ML-DSA-65), 87 (ML-DSA-87).
// legacy aliases 2/3/5 are also accepted by the Rust implementation.
AOEM_API uint32_t aoem_mldsa_supported(void);
AOEM_API uint32_t aoem_mldsa_pubkey_size(uint32_t level);
AOEM_API uint32_t aoem_mldsa_signature_size(uint32_t level);
// return code:
//  0 = call succeeded (out_valid is 0/1)
// -2 = invalid argument
// -4 = verify input/format error
// -5 = capability not built (mldsa feature disabled)
AOEM_API int32_t aoem_mldsa_verify(
  uint32_t level,
  const uint8_t* pubkey_ptr,
  size_t pubkey_len,
  const uint8_t* message_ptr,
  size_t message_len,
  const uint8_t* signature_ptr,
  size_t signature_len,
  uint32_t* out_valid
);
AOEM_API int32_t aoem_mldsa_verify_auto(
  const uint8_t* pubkey_ptr,
  size_t pubkey_len,
  const uint8_t* message_ptr,
  size_t message_len,
  const uint8_t* signature_ptr,
  size_t signature_len,
  uint32_t* out_valid
);
AOEM_API uint32_t aoem_recommend_parallelism(
  uint64_t txs,
  uint32_t batch,
  uint64_t key_space,
  double rw
);
AOEM_API void* aoem_create(void);
AOEM_API void* aoem_create_with_options(const aoem_create_options_v1* opts);
AOEM_API void aoem_destroy(void* handle);
AOEM_API int32_t aoem_execute(
  void* handle,
  const uint8_t* input_ptr,
  size_t input_len,
  uint8_t** output_ptr,
  size_t* output_len
);
// Output format:
// - default: AOER binary envelope (high-performance path)
// - compatibility: JSON only when AOEM_FFI_RESPONSE_JSON=1
// Production guard:
// - aoem_execute is disabled by default in production profile
// - enable only for debug with AOEM_FFI_ENABLE_SINGLE_EXEC=1
// Fast discard mode:
// - pass output_ptr=NULL and output_len=NULL to execute without allocating response bytes
AOEM_API int32_t aoem_execute_batch(
  void* handle,
  const uint8_t* input_ptr,
  size_t input_len,
  uint8_t** output_ptr,
  size_t* output_len
);
AOEM_API int32_t aoem_execute_ops_v2(
  void* handle,
  const aoem_op_v2* ops_ptr,
  uint32_t op_count,
  aoem_exec_v2_result* out_result
);
// Generic ops-wire ingestion (production-friendly).
// Wire format (little-endian):
// - magic: "AOV2\0" (5 bytes)
// - version: u16 (currently 1)
// - flags: u16 (reserved; should be 0)
// - op_count: u32
// - repeated op_count times:
//   opcode:u8, flags:u8, reserved:u16,
//   key_len:u32, value_len:u32,
//   delta:i64, expect_version:u64, plan_id:u64,
//   key_bytes[key_len], value_bytes[value_len]
// This API is domain-agnostic: caller can encode any business workload
// into AOEM primitive ops without per-app host-side ExecOp struct plumbing.
AOEM_API int32_t aoem_execute_ops_wire_v1(
  void* handle,
  const uint8_t* input_ptr,
  size_t input_len,
  aoem_exec_v2_result* out_result
);
// Batch fast discard mode:
// - pass output_ptr=NULL and output_len=NULL
AOEM_API void aoem_free(uint8_t* ptr, size_t len);
AOEM_API const char* aoem_last_error(void* handle);

#ifdef __cplusplus
}
#endif

#endif
