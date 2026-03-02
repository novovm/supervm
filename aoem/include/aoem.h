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
AOEM_API const char* aoem_capabilities_json(void);
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
// Batch fast discard mode:
// - pass output_ptr=NULL and output_len=NULL
AOEM_API void aoem_free(uint8_t* ptr, size_t len);
AOEM_API const char* aoem_last_error(void* handle);

#ifdef __cplusplus
}
#endif

#endif
