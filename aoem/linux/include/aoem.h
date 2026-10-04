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

// AOEM FFI surface tiers:
// - Production ABI: stable host-facing process, execution, state, hash, signature,
//   and privacy-native execution entrypoints.
// - Feature ABI: optional advanced capabilities guarded by Cargo features or plugin probes.
// - Internal/experimental ABI: diagnostics, probes, and compatibility shims. These may remain
//   exported for binary compatibility, but they are not default product commitments.

typedef struct aoem_create_options_v1 {
  uint32_t abi_version;   // must be 1
  uint32_t struct_size;   // sizeof(aoem_create_options_v1)
  uint32_t ingress_workers; // 0 => keep default
  uint32_t flags;         // reserved, must be 0
} aoem_create_options_v1;

// Domain-neutral opaque semantic task. Input and output buffers remain owned
// by the embedding host for the complete synchronous batch call.
typedef struct aoem_semantic_task_v1 {
  const uint8_t* input_ptr;
  size_t input_len;
  uint8_t* output_ptr;
  size_t output_capacity;
  size_t output_len;
  int32_t status;
} aoem_semantic_task_v1;

typedef int32_t (*aoem_semantic_task_callback_v1)(
  const uint8_t* input_ptr,
  size_t input_len,
  uint8_t* output_ptr,
  size_t output_capacity,
  size_t* output_len,
  void* user_data
);

// Domain-neutral opaque input for nonblocking semantic submission. AOEM copies
// input bytes before returning from aoem_submit_semantic_batch_v1.
typedef struct aoem_semantic_input_v1 {
  const uint8_t* input_ptr;
  size_t input_len;
} aoem_semantic_input_v1;

typedef int32_t (*aoem_semantic_input_callback_v1)(
  const uint8_t* input_ptr,
  size_t input_len,
  void* user_data
);

// Domain-neutral continuation emission for nonblocking semantic task graphs.
// The emitted bytes are copied before this function returns. The emit function
// and emit_data are valid only for the duration of the current task callback
// and must never be retained by the host.
typedef int32_t (*aoem_semantic_graph_emit_v1)(
  const uint8_t* input_ptr,
  size_t input_len,
  void* emit_data
);

typedef int32_t (*aoem_semantic_graph_callback_v1)(
  const uint8_t* input_ptr,
  size_t input_len,
  aoem_semantic_graph_emit_v1 emit,
  void* emit_data,
  void* user_data
);

// Domain-neutral fixed-layout semantic graph ABI. AOEM owns scheduling and
// bounded queue/event storage; the host owns opaque context handles and payload
// interpretation. A step is accepted atomically: continuation, emitted task,
// and state event are all accepted, or the unchanged input is retried later.
#define AOEM_SEMANTIC_GRAPH_ABI_V2 2u
#define AOEM_SEMANTIC_GRAPH_ABI_V3 3u
#define AOEM_ATOMIC_WRITE_ABI_V1 1u
#define AOEM_STATUS_OK 0
#define AOEM_STATUS_WOULD_BLOCK 1
#define AOEM_ERROR_INVALID_ARGUMENT (-1)
#define AOEM_ERROR_ABI_MISMATCH (-2)
#define AOEM_ERROR_DESCRIPTOR_TOO_LARGE (-3)
#define AOEM_ERROR_EVENT_TOO_LARGE (-4)
#define AOEM_ERROR_ADMISSION_REJECTED (-5)
#define AOEM_ERROR_UNKNOWN_CONTEXT_HANDLE (-6)
#define AOEM_ERROR_STALE_GENERATION_HANDLE (-7)
#define AOEM_ERROR_GRAPH_CANCELLED (-8)
#define AOEM_ERROR_GRAPH_FAULTED (-9)
#define AOEM_ERROR_STATE_WRITE_FAILED (-10)
#define AOEM_ERROR_CALLBACK_PANICKED (-11)

#define AOEM_STEP_HAS_CONTINUATION (1u << 0)
#define AOEM_STEP_HAS_EMITTED_TASK (1u << 1)
#define AOEM_STEP_HAS_EVENT (1u << 2)
#define AOEM_STEP_HAS_ATOMIC_WRITE_SET (1u << 3)
#define AOEM_ATOMIC_WRITE_PUT_V1 1u
#define AOEM_ATOMIC_WRITE_DELETE_V1 2u
#define AOEM_GRAPH_HAS_ADMISSION_WRITE_SET_V3 (1u << 0)

typedef struct aoem_task_descriptor_v2 {
  uint16_t abi_version;
  uint16_t task_kind;
  uint16_t payload_len;
  uint8_t priority;
  uint8_t flags;
  uint64_t graph_id;
  uint64_t task_id;
  uint64_t context_handle;
  uint64_t sequence;
  uint8_t payload[88];
} aoem_task_descriptor_v2;

typedef struct aoem_state_event_v2 {
  uint16_t abi_version;
  uint16_t event_kind;
  uint16_t payload_len;
  uint16_t flags;
  uint64_t graph_id;
  uint64_t task_id;
  uint64_t context_handle;
  uint64_t sequence;
  uint8_t payload[216];
} aoem_state_event_v2;

typedef struct aoem_task_step_output_v2 {
  uint32_t flags;
  uint32_t reserved;
  aoem_task_descriptor_v2 continuation;
  aoem_task_descriptor_v2 emitted_task;
  aoem_state_event_v2 event;
} aoem_task_step_output_v2;

typedef struct aoem_graph_submit_options_v2 {
  uint16_t abi_version;
  uint16_t flags;
  uint32_t max_queued_tasks;
  uint32_t event_capacity;
  uint64_t initial_event_sequence;
} aoem_graph_submit_options_v2;

typedef struct aoem_graph_completion_v2 {
  uint16_t abi_version;
  uint16_t reserved;
  int32_t status;
  uint64_t graph_id;
  uint64_t processed;
  uint64_t succeeded;
  uint64_t failed;
  uint64_t would_block_retries;
  uint64_t peak_queued_tasks;
} aoem_graph_completion_v2;

typedef int32_t (*aoem_task_execute_callback_v2)(
  const aoem_task_descriptor_v2* descriptor,
  aoem_task_step_output_v2* output,
  void* user_data
);
typedef int32_t (*aoem_context_retain_callback_v2)(
  uint64_t context_handle,
  void* user_data
);
typedef int32_t (*aoem_context_release_callback_v2)(
  uint64_t context_handle,
  void* user_data
);
typedef int32_t (*aoem_state_event_callback_v2)(
  const aoem_state_event_v2* event,
  void* user_data
);
typedef void (*aoem_graph_completion_callback_v2)(
  const aoem_graph_completion_v2* completion,
  void* user_data
);

typedef struct aoem_graph_callbacks_v2 {
  aoem_task_execute_callback_v2 execute;
  aoem_context_retain_callback_v2 retain_context;
  aoem_context_release_callback_v2 release_context;
  aoem_state_event_callback_v2 state_event;
  aoem_graph_completion_callback_v2 completion;
  void* user_data;
} aoem_graph_callbacks_v2;

typedef struct aoem_atomic_write_record_v1 {
  uint8_t kind;
  uint8_t reserved[3];
  uint16_t key_len;
  uint16_t value_len;
  uint8_t key[96];
  uint8_t value[512];
} aoem_atomic_write_record_v1;

typedef struct aoem_atomic_write_set_v1 {
  uint16_t abi_version;
  uint16_t write_count;
  uint32_t reserved;
  uint64_t stream_id;
  // Completion-correlation identity; unique among in-flight sets in this stream.
  uint64_t sequence;
  aoem_atomic_write_record_v1 writes[4];
} aoem_atomic_write_set_v1;

typedef struct aoem_task_step_output_v3 {
  uint32_t flags;
  uint32_t reserved;
  aoem_task_descriptor_v2 continuation;
  aoem_task_descriptor_v2 emitted_task;
  aoem_state_event_v2 event;
  aoem_atomic_write_set_v1 atomic_write_set;
} aoem_task_step_output_v3;

typedef struct aoem_graph_submit_options_v3 {
  uint16_t abi_version;
  uint16_t flags;
  uint32_t max_queued_tasks;
  uint32_t event_capacity;
  uint64_t initial_event_sequence;
  aoem_atomic_write_set_v1 admission_write_set;
} aoem_graph_submit_options_v3;

typedef int32_t (*aoem_task_execute_callback_v3)(
  const aoem_task_descriptor_v2* descriptor,
  aoem_task_step_output_v3* output,
  void* user_data
);
typedef int32_t (*aoem_graph_completion_write_callback_v3)(
  const aoem_graph_completion_v2* completion,
  aoem_atomic_write_set_v1* output,
  void* user_data
);

typedef struct aoem_graph_callbacks_v3 {
  aoem_task_execute_callback_v3 execute;
  aoem_context_retain_callback_v2 retain_context;
  aoem_context_release_callback_v2 release_context;
  aoem_state_event_callback_v2 state_event;
  aoem_graph_completion_write_callback_v3 completion_write;
  aoem_graph_completion_callback_v2 completion;
  void* user_data;
} aoem_graph_callbacks_v3;

#if defined(__cplusplus)
static_assert(sizeof(aoem_task_descriptor_v2) == 128, "AOEM V2 task descriptor ABI");
static_assert(sizeof(aoem_state_event_v2) == 256, "AOEM V2 state event ABI");
static_assert(sizeof(aoem_task_step_output_v2) == 520, "AOEM V2 step output ABI");
static_assert(sizeof(aoem_atomic_write_record_v1) == 616, "AOEM atomic write record ABI");
static_assert(sizeof(aoem_atomic_write_set_v1) == 2488, "AOEM atomic write set ABI");
static_assert(sizeof(aoem_task_step_output_v3) == 3008, "AOEM V3 step output ABI");
static_assert(sizeof(aoem_graph_submit_options_v3) == 2512, "AOEM V3 submit options ABI");
#elif defined(__STDC_VERSION__) && __STDC_VERSION__ >= 201112L
_Static_assert(sizeof(aoem_task_descriptor_v2) == 128, "AOEM V2 task descriptor ABI");
_Static_assert(sizeof(aoem_state_event_v2) == 256, "AOEM V2 state event ABI");
_Static_assert(sizeof(aoem_task_step_output_v2) == 520, "AOEM V2 step output ABI");
_Static_assert(sizeof(aoem_atomic_write_record_v1) == 616, "AOEM atomic write record ABI");
_Static_assert(sizeof(aoem_atomic_write_set_v1) == 2488, "AOEM atomic write set ABI");
_Static_assert(sizeof(aoem_task_step_output_v3) == 3008, "AOEM V3 step output ABI");
_Static_assert(sizeof(aoem_graph_submit_options_v3) == 2512, "AOEM V3 submit options ABI");
#endif

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

typedef void (*aoem_semantic_completion_callback_v1)(
  const aoem_exec_v2_result* result,
  void* user_data
);

typedef struct aoem_primitive_result_v1 {
  uint32_t primitive;    // echo input primitive kind
  uint32_t backend_kind; // 1=spirv-vulkan, 2=cuda
  uint32_t stage_count;
  uint32_t values_len;
  uint32_t indices_len;
  uint64_t output_hash;
} aoem_primitive_result_v1;

// Production ABI: process and bundle identity.
AOEM_API uint32_t aoem_abi_version(void);
AOEM_API const char* aoem_version_string(void);
// Process-level one-time warmup entry.
// Call once at process startup to pre-resolve capabilities and optional sidecar plugins.
AOEM_API int32_t aoem_global_init(void);
// Production ABI: capability introspection. Capability booleans report compiled/probed
// availability; they do not mean every exported symbol is part of the default Production ABI.
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
// Feature ABI: zkVM optional capability probe.
AOEM_API uint32_t aoem_zkvm_supported(void);
// Feature ABI: generic zkVM prove+verify entry.
// backend values:
//   0 = auto
//   1 = trace
//   2 = risc0
//   3 = sp1
//   4 = halo2 (feature-gated PoC)
// Witness payload contract:
// - trace/risc0 path: 20 bytes {a0:u64, a1:u64, rounds:u32} (little-endian)
//                     legacy 16 bytes {a0:u64, a1:u64} defaults rounds=10
// - sp1 path: raw witness bytes passed to SP1 stdin
// - halo2 path: HALO2_WITNESS_V1 wire
//   magic "AH2W0001" + blob_count:u32 + repeated{blob_len:u32 + blob_bytes}
// Program payload contract:
// - trace/risc0 path: ignored (can be null/0)
// - sp1 path: required guest ELF bytes
// - halo2 path: HALO2_PROGRAM_V1 wire
//   magic "AH2P0001" + k:u32 + max_proofs:u32
// return code:
//  0 = call succeeded (out_verified is 0/1)
// -2 = invalid argument / malformed witness
// -4 = prove or verify execution error
// -5 = capability not built / backend unavailable on current build/platform
AOEM_API int32_t aoem_zkvm_prove_verify_v1(
  uint32_t backend,
  const uint8_t* program_ptr,
  size_t program_len,
  const uint8_t* witness_ptr,
  size_t witness_len,
  uint32_t* out_verified
);
// Portable RISC0 C ABI v1, receipt envelope AORCP002; not a trace or business proof.
// image_id points to 8 host-endian u32 RISC0 image-ID words, pinned by the host.
// All input regions must remain readable and not overlap writable output slots.
// Program: 1..64 MiB; serialized stdin, receipt, expected journal: at most 16 MiB.
// Null byte pointers are allowed only for zero lengths. Empty journal is exact,
// never a wildcard. Receipt does not supply trusted program/output pins.
// Prove is trusted local work; caller must isolate/budget arbitrary guest execution.
// Output slots are reset on failure; success allocates a receipt freed by aoem_free.
// RISC0 2.3.2 combined user+kernel program (not a bare ELF); receipt is AORCP002.
// Old AORCP001 receipts are rejected. Function signatures/ownership are unchanged.
// Built-in supported Linux risc0 or a compatible configured zkVM sidecar; no trace fallback.
// Returns 0 on success, -2 invalid arguments/bounds, -4 proof/verification error,
// -5 unavailable. Fake development receipts are rejected.
AOEM_API int32_t aoem_risc0_prove_v1(
  const uint8_t* elf, size_t elf_len,
  const uint8_t* input, size_t input_len,
  const uint32_t* image_id, uint8_t** out_receipt, size_t* out_receipt_len
);
AOEM_API int32_t aoem_risc0_verify_v1(
  const uint8_t* receipt, size_t receipt_len, const uint32_t* image_id,
  const uint8_t* expected_journal, size_t expected_journal_len
);

// Internal/diagnostic ABI: minimal host-side zkVM prove+verify roundtrip probe (Trace/Fibonacci).
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
// Feature ABI: ML-DSA optional capability.
// level values: 44 (ML-DSA-44), 65 (ML-DSA-65), 87 (ML-DSA-87).
// legacy aliases 2/3/5 are also accepted by the Rust implementation.
// Message contract for sign/verify/auto/batch: FIPS 204 internal M' bytes.
// AOEM does not add a context prefix or prehash the supplied message.
// For external pure ML-DSA the caller supplies 0x00 || context_length:u8 ||
// context || message, exactly once (context length <= 255).
// Expanded secret-key lengths are 2560/4032/4896 bytes. Qualify the loaded
// runtime against final FIPS 204 vectors before use: older shipped builds
// contain pre-standard Dilithium behavior at levels 65/87. Never silently
// downgrade or treat draft-era material as final-standard credentials.
AOEM_API uint32_t aoem_mldsa_supported(void);
AOEM_API uint32_t aoem_mldsa_pubkey_size(uint32_t level);
AOEM_API uint32_t aoem_mldsa_signature_size(uint32_t level);
AOEM_API uint32_t aoem_mldsa_secret_key_size(uint32_t level);
// return code:
//  0 = call succeeded
// -2 = invalid argument
// -4 = keygen error
// -5 = capability not built (mldsa feature disabled)
// output memory is allocated by AOEM and must be released with aoem_free.
AOEM_API int32_t aoem_mldsa_keygen_v1(
  uint32_t level,
  uint8_t** out_pubkey_ptr,
  size_t* out_pubkey_len,
  uint8_t** out_secret_key_ptr,
  size_t* out_secret_key_len
);
// return code:
//  0 = call succeeded
// -2 = invalid argument
// -4 = sign input/format error
// -5 = capability not built (mldsa feature disabled)
// output memory is allocated by AOEM and must be released with aoem_free.
AOEM_API int32_t aoem_mldsa_sign_v1(
  uint32_t level,
  const uint8_t* secret_key_ptr,
  size_t secret_key_len,
  const uint8_t* message_ptr,
  size_t message_len,
  uint8_t** out_signature_ptr,
  size_t* out_signature_len
);
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
typedef struct aoem_mldsa_verify_item_v1 {
  // 0 => auto-detect by pubkey length; otherwise 44/65/87 (legacy 2/3/5 accepted).
  uint32_t level;
  const uint8_t* pubkey_ptr;
  size_t pubkey_len;
  const uint8_t* message_ptr;
  size_t message_len;
  const uint8_t* signature_ptr;
  size_t signature_len;
} aoem_mldsa_verify_item_v1;
// Batch ML-DSA verify.
// - out_results is a byte array with length=item_count; each byte is 0/1.
// - output memory is allocated by AOEM and must be released with aoem_free.
// return code:
//  0 = call succeeded
// -2 = invalid argument
// -4 = verify input/format error
// -5 = capability not built (mldsa feature disabled and plugin unavailable)
AOEM_API int32_t aoem_mldsa_verify_batch_v1(
  const aoem_mldsa_verify_item_v1* items_ptr,
  size_t item_count,
  uint8_t** out_results_ptr,
  size_t* out_results_len,
  uint32_t* out_valid_count
);
// Production ABI: classic crypto/hash ABI (host-oriented, binary-safe).
// Hash outputs are fixed 32 bytes.
// return code:
//  0 = call succeeded
// -2 = invalid argument
AOEM_API int32_t aoem_sha256_v1(
  const uint8_t* data_ptr,
  size_t data_len,
  uint8_t* out_hash32
);
AOEM_API int32_t aoem_keccak256_v1(
  const uint8_t* data_ptr,
  size_t data_len,
  uint8_t* out_hash32
);
AOEM_API int32_t aoem_blake3_256_v1(
  const uint8_t* data_ptr,
  size_t data_len,
  uint8_t* out_hash32
);
// Production ABI: Ed25519 verify.
// return code:
//  0 = call succeeded (out_valid is 0/1)
// -2 = invalid argument
// -4 = verify input/format error
AOEM_API int32_t aoem_ed25519_verify_v1(
  const uint8_t* pubkey_ptr,
  size_t pubkey_len,
  const uint8_t* message_ptr,
  size_t message_len,
  const uint8_t* signature_ptr,
  size_t signature_len,
  uint32_t* out_valid
);
typedef struct aoem_ed25519_verify_item_v1 {
  const uint8_t* pubkey_ptr;
  size_t pubkey_len;
  const uint8_t* message_ptr;
  size_t message_len;
  const uint8_t* signature_ptr;
  size_t signature_len;
} aoem_ed25519_verify_item_v1;
// Production ABI: batch Ed25519 verify.
// - out_results is a byte array with length=item_count; each byte is 0/1.
// - output memory is allocated by AOEM and must be released with aoem_free.
// return code:
//  0 = call succeeded
// -2 = invalid argument
// -4 = verify input/format error
AOEM_API int32_t aoem_ed25519_verify_batch_v1(
  const aoem_ed25519_verify_item_v1* items_ptr,
  size_t item_count,
  uint8_t** out_results_ptr,
  size_t* out_results_len,
  uint32_t* out_valid_count
);
// Production ABI: secp256k1 verify/recover.
// Signature format: 65 bytes [r(32)||s(32)||v(1)], where v in {0,1,27,28}.
// message32 is 32 bytes.
// return code:
//  0 = call succeeded (out_valid is 0/1 for verify APIs)
// -2 = invalid argument
// -4 = verify/recover input or decode error
AOEM_API int32_t aoem_secp256k1_verify_v1(
  const uint8_t* message32_ptr,
  size_t message32_len,
  const uint8_t* signature65_ptr,
  size_t signature65_len,
  const uint8_t* pubkey_ptr,
  size_t pubkey_len,
  uint32_t* out_valid
);
// Output pubkey is SEC1 uncompressed 65-byte form.
// output memory is allocated by AOEM and must be released with aoem_free.
AOEM_API int32_t aoem_secp256k1_recover_pubkey_v1(
  const uint8_t* message32_ptr,
  size_t message32_len,
  const uint8_t* signature65_ptr,
  size_t signature65_len,
  uint8_t** out_pubkey_ptr,
  size_t* out_pubkey_len
);
// Production ABI: verify-only ECDSA prehash host entrypoints.
// These APIs verify public inputs only. They do not sign, recover, store keys,
// accept private key material, manage nonces, or apply wallet/chain semantics.
// q is SEC1 compressed or uncompressed public key bytes.
// z, r, and s are each exactly 32 bytes. The caller hashes the message before
// calling this ABI.
// return code:
//  0 = parsed and verification decision written to out_ok
// -1 = null pointer or empty required pointer input
// -2 = invalid z/r/s length
// -3 = public key or signature parse failure
// out_ok:
//  1 = signature verified
//  0 = signature did not verify or call failed before success
AOEM_API int32_t aoem_ffi_secp256k1_verify(
  const uint8_t* q_ptr,
  size_t q_len,
  const uint8_t* z_ptr,
  size_t z_len,
  const uint8_t* r_ptr,
  size_t r_len,
  const uint8_t* s_ptr,
  size_t s_len,
  uint8_t* out_ok
);
AOEM_API int32_t aoem_ffi_p256_verify(
  const uint8_t* q_ptr,
  size_t q_len,
  const uint8_t* z_ptr,
  size_t z_len,
  const uint8_t* r_ptr,
  size_t r_len,
  const uint8_t* s_ptr,
  size_t s_len,
  uint8_t* out_ok
);
// Feature ABI: ring-signature verification (Web30-compatible payload).
// signature_json payload schema:
// {
//   "ring_members": [[u8;32], ...],
//   "key_image": [u8;32],
//   "c": [[u8;32], ...],   // c[0] is used as initial challenge
//   "r": [[u8;32], ...]    // response scalars
// }
// message is bound with amount (u128 little-endian) before verification.
// return code:
//  0 = call succeeded (out_valid is 0/1)
// -2 = invalid argument
// -4 = decode/verify error
AOEM_API uint32_t aoem_ring_signature_supported(void);
// Feature ABI: ring-signature keygen.
// output memory is allocated by AOEM and must be released with aoem_free.
// return code:
//  0 = call succeeded
// -2 = invalid argument
// -4 = keygen error
// -5 = capability not built (privacy-verify feature disabled)
AOEM_API int32_t aoem_ring_signature_keygen_v1(
  uint8_t** out_public_key_ptr,
  size_t* out_public_key_len,
  uint8_t** out_secret_key_ptr,
  size_t* out_secret_key_len
);
// Feature ABI: ring-signature sign (Web30-compatible output payload).
// ring_json accepts:
//  1) [[u8;32], ...]
//  2) {"ring_members":[[u8;32], ...]}
// output signature_json schema matches verify input schema.
// output memory is allocated by AOEM and must be released with aoem_free.
// return code:
//  0 = call succeeded
// -2 = invalid argument
// -4 = decode/sign error
// -5 = capability not built (privacy-verify feature disabled)
AOEM_API int32_t aoem_ring_signature_sign_web30_v1(
  const uint8_t* ring_json_ptr,
  size_t ring_json_len,
  uint32_t secret_index,
  const uint8_t* secret_key_ptr,
  size_t secret_key_len,
  const uint8_t* public_key_ptr,
  size_t public_key_len,
  const uint8_t* message_ptr,
  size_t message_len,
  uint64_t amount_lo,
  uint64_t amount_hi,
  uint8_t** out_signature_json_ptr,
  size_t* out_signature_json_len
);
AOEM_API int32_t aoem_ring_signature_verify_web30_v1(
  const uint8_t* signature_json_ptr,
  size_t signature_json_len,
  const uint8_t* message_ptr,
  size_t message_len,
  uint64_t amount_lo,
  uint64_t amount_hi,
  uint32_t* out_valid
);
// Feature ABI: ring-signature batch verify (Web30-compatible payload).
// batch_json schema:
// [
//   {
//     "signature": { ... Web30RingSignatureV1 ... },
//     "message": [u8, ...],          // raw message bytes
//     "amount_lo": u64,
//     "amount_hi": u64
//   },
//   ...
// ]
// Outputs:
// - out_results: byte bitmap, 1=valid, 0=invalid (same order as input array)
// - out_valid_count: number of valid items
// return code:
//  0 = call succeeded
// -2 = invalid argument
// -4 = decode/verify error
// -5 = capability not built (privacy-verify feature disabled)
AOEM_API int32_t aoem_ring_signature_verify_batch_web30_v1(
  const uint8_t* batch_json_ptr,
  size_t batch_json_len,
  uint8_t** out_results_ptr,
  size_t* out_results_len,
  uint32_t* out_valid_count
);
// Feature ABI: Groth16 fixed-circuit prove.
// Witness wire (little-endian, 24 bytes):
//   [a:u64][b:u64][c:u64], with constraint a*b == c
// Outputs:
// - out_vk: PreparedVerifyingKey<Bls12_381> bytes (arkworks uncompressed wire)
// - out_proof: Proof<Bls12_381> bytes (arkworks uncompressed wire)
// - out_public_inputs: FR_VEC_WIRE_V1 for [c]
// return code:
//  0 = call succeeded
// -2 = invalid argument / malformed witness
// -4 = prove/encode error
// -5 = capability not built (privacy-verify feature disabled)
AOEM_API int32_t aoem_groth16_prove_v1(
  const uint8_t* witness_ptr,
  size_t witness_len,
  uint8_t** out_vk_ptr,
  size_t* out_vk_len,
  uint8_t** out_proof_ptr,
  size_t* out_proof_len,
  uint8_t** out_public_inputs_ptr,
  size_t* out_public_inputs_len
);
// Feature ABI: Groth16 batch prove.
// Input:
// - witnesses wire: [count:u32_le][len:u32_le][bytes...][len:u32_le][bytes...]...
// - each witness item bytes: same as aoem_groth16_prove_v1 witness wire (24 bytes [a][b][c]).
// Outputs:
// - out_vk: PreparedVerifyingKey<Bls12_381> bytes (shared for batch, arkworks uncompressed wire)
// - out_proofs_wire: len-prefixed blob list wire of proof bytes (same count as input)
// - out_public_inputs_wire: len-prefixed blob list wire of FR_VEC_WIRE_V1 payloads (same count as input)
// return code:
//  0 = call succeeded
// -2 = invalid argument / malformed witness wire
// -4 = prove/encode/self-verify error
// -5 = capability not built (privacy-verify feature disabled)
AOEM_API int32_t aoem_groth16_prove_batch_v1(
  const uint8_t* witnesses_wire_ptr,
  size_t witnesses_wire_len,
  uint8_t** out_vk_ptr,
  size_t* out_vk_len,
  uint8_t** out_proofs_wire_ptr,
  size_t* out_proofs_wire_len,
  uint8_t** out_public_inputs_wire_ptr,
  size_t* out_public_inputs_wire_len
);
// Feature ABI: Groth16 single-proof verify.
// Input contracts:
// - vk_ptr/vk_len: PreparedVerifyingKey<Bls12_381> bytes (arkworks uncompressed unchecked wire).
// - proof_ptr/proof_len: Proof<Bls12_381> bytes (arkworks uncompressed unchecked wire).
// - public_inputs_ptr/public_inputs_len: FR_VEC_WIRE_V1
//   [count:u32_le][Fr0(uncompressed)][Fr1(uncompressed)]...
// return code:
//  0 = call succeeded (out_valid is 0/1)
// -2 = invalid argument
// -4 = decode/verify error
// -5 = capability not built (privacy-verify feature disabled)
AOEM_API int32_t aoem_groth16_verify_v1(
  const uint8_t* vk_ptr,
  size_t vk_len,
  const uint8_t* proof_ptr,
  size_t proof_len,
  const uint8_t* public_inputs_ptr,
  size_t public_inputs_len,
  uint32_t* out_valid
);
// Feature ABI: Groth16 batch verify.
// Shared verifying key:
// - vk_ptr/vk_len: PreparedVerifyingKey<Bls12_381> bytes (arkworks uncompressed unchecked wire)
// Batch wire for proofs/public-inputs (both are required, same count):
// - [count:u32_le][len:u32_le][bytes...][len:u32_le][bytes...]...
// - proofs wire item bytes: Proof<Bls12_381> bytes (arkworks uncompressed unchecked wire)
// - public_inputs wire item bytes: FR_VEC_WIRE_V1
// Output:
// - out_results: byte bitmap in input order (1=valid, 0=invalid)
// - out_valid_count: count(valid)
// return code:
//  0 = call succeeded
// -2 = invalid argument / count mismatch
// -4 = decode/verify error
// -5 = capability not built (privacy-verify feature disabled)
AOEM_API int32_t aoem_groth16_verify_batch_v1(
  const uint8_t* vk_ptr,
  size_t vk_len,
  const uint8_t* proofs_wire_ptr,
  size_t proofs_wire_len,
  const uint8_t* public_inputs_wire_ptr,
  size_t public_inputs_wire_len,
  uint8_t** out_results_ptr,
  size_t* out_results_len,
  uint32_t* out_valid_count
);
// Feature ABI: Bulletproof range prove.
// Input:
// - amount_lo/amount_hi: amount (u128 little-endian split; amount_hi must be 0 in v1)
// - bits: range bits (0 -> default 64)
// Outputs:
// - out_commitment: 32-byte commitment
// - out_proof: Bulletproof bytes
// return code:
//  0 = call succeeded
// -2 = invalid argument
// -4 = prove/verify self-check error
// -5 = capability not built (privacy-verify feature disabled)
AOEM_API int32_t aoem_bulletproof_prove_v1(
  uint64_t amount_lo,
  uint64_t amount_hi,
  uint32_t bits,
  uint8_t** out_commitment_ptr,
  size_t* out_commitment_len,
  uint8_t** out_proof_ptr,
  size_t* out_proof_len
);
// Feature ABI: Bulletproof range proof verify.
// Input contracts:
// - commitment_ptr/commitment_len: 32-byte commitment
// - proof_ptr/proof_len: Bulletproof bytes
// - bits: range bits (0 -> default 64)
// return code:
//  0 = call succeeded (out_valid is 0/1)
// -2 = invalid argument
// -4 = decode/verify error
// -5 = capability not built (privacy-verify feature disabled)
AOEM_API int32_t aoem_bulletproof_verify_v1(
  const uint8_t* commitment_ptr,
  size_t commitment_len,
  const uint8_t* proof_ptr,
  size_t proof_len,
  uint32_t bits,
  uint32_t* out_valid
);
// Feature ABI: Bulletproof batch prove.
// Input: JSON array
// [
//   { "amount_lo": u64, "amount_hi": u64, "bits": u32 },
//   ...
// ]
// Output: JSON array
// [
//   { "commitment": [u8;32], "proof": [u8,...], "bits": u32 },
//   ...
// ]
AOEM_API int32_t aoem_bulletproof_prove_batch_v1(
  const uint8_t* batch_json_ptr,
  size_t batch_json_len,
  uint8_t** out_batch_json_ptr,
  size_t* out_batch_json_len
);
// Feature ABI: Bulletproof batch verify.
// Input: same JSON array produced by aoem_bulletproof_prove_batch_v1.
// Output:
// - out_results: byte bitmap (1=valid, 0=invalid)
// - out_valid_count: count(valid)
AOEM_API int32_t aoem_bulletproof_verify_batch_v1(
  const uint8_t* batch_json_ptr,
  size_t batch_json_len,
  uint8_t** out_results_ptr,
  size_t* out_results_len,
  uint32_t* out_valid_count
);
// Feature ABI: RingCT transaction prove/generate.
// v1 generates synthetic inputs/keys; it does not spend a wallet's existing
// output. The generated payload is admitted only by the same process cache.
// Input:
// - message_ptr/message_len: transaction message (bound to ring signature)
// - amount_lo/amount_hi: amount (u128 little-endian split; amount_hi must be 0 in v1)
// - ring_size: ring member count (>=2)
// Output:
// - out_tx_payload_json: JSON payload of PrivacyTransaction
// return code:
//  0 = call succeeded
// -2 = invalid argument
// -4 = generation/verify self-check error
// -5 = capability not built (privacy-verify feature disabled)
AOEM_API int32_t aoem_ringct_prove_v1(
  const uint8_t* message_ptr,
  size_t message_len,
  uint64_t amount_lo,
  uint64_t amount_hi,
  uint32_t ring_size,
  uint8_t** out_tx_payload_json_ptr,
  size_t* out_tx_payload_json_len
);
// Feature ABI: RingCT batch prove/generate.
// Input: JSON array
// [
//   { "message": [u8,...], "amount_lo": u64, "amount_hi": u64, "ring_size": u32 },
//   ...
// ]
// Output: JSON array of PrivacyTransaction payloads.
AOEM_API int32_t aoem_ringct_prove_batch_v1(
  const uint8_t* batch_json_ptr,
  size_t batch_json_len,
  uint8_t** out_batch_json_ptr,
  size_t* out_batch_json_len
);
// Production privacy ABI when built with privacy-verify: unified privacy-native execution.
// Current RingCt v1 admits exact, locally generated cached payloads only.
// A cold process rejects remote payloads. accepted/state_materialized do not
// establish ledger input ownership or authorize a confidential asset transfer.
// Do not bypass admission using legacy verify or host-supplied trust flags.
// Request JSON v1:
// {
//   "version": 1,
//   "kind": "RingCt",
//   "backend": "Cpu" | "FullGpu" | "Auto",
//   "transactions": [
//     { "encoding": "hex" | "json", "data": "..." }
//   ]
// }
// For encoding="hex", data is hex-encoded JSON bytes of a PrivacyTransaction.
// For encoding="json", data can be either a JSON object or a JSON string.
// Response JSON v1:
// {
//   "version": 1,
//   "accepted": bool,
//   "status": "Accepted" | "Rejected" | "Failed",
//   "error_code": null | "UnsupportedKind" | "AdmissionRejected" |
//                 "BackendUnavailable" | "ExecutionRejected" |
//                 "StateMaterializationFailed",
//   "error_reason": null | string,
//   "backend_used": "Cpu" | "FullGpu" | "Auto",
//   "gpu_path_hit": bool,
//   "cpu_core_triggered_any": bool,
//   "cpu_slow_path_triggered_any": bool,
//   "state_materialized": bool,
//   "tx_results": [
//     { "accepted": bool, "error_code": null | string, "error_reason": null | string }
//   ]
// }
// JSON v2 extends THIS SAME symbol; it does not use the historical prove cache.
// Fixed classic profile (NOT post-quantum), exact Host-computed u64 fee:
// {"version":2,"profile":"clsag_bpplus_edwards_u64_v1","backend":"Cpu",
//  "transactions":[{"transaction_hex":"<canonical lower-case hex>","context":{
//    "expected_domain":[/*32 bytes*/],"expected_exact_fee":7,
//    "rings":[{"members":[{"output_index":0,"public_key":[/*32 bytes*/],
//                           "commitment":[/*32 bytes*/]} /*16 members*/]}]}}]}
// Domain, fee, rings and indices MUST be independently authenticated by the Host,
// never copied untrusted from a submitting transaction. Rings are in serialized
// transaction input order, with strictly increasing member indices. All points
// are Edwards-curve canonical encodings, NOT the legacy Ristretto profile.
// v2 rejects unknown/duplicate fields at every level. Limits: JSON <=16 MiB,
// 1..32 transactions, binary tx <=128 KiB, 1..32 input rings, exactly 16 members.
// Omitted backend defaults to Auto; Cpu/Auto verify on CPU. FullGpu is rejected
// explicitly, never silently downgraded. No private wallet witness is accepted.
// v2 responses set version=2, profile, accepted, proof_verified, status, errors,
// backend_used, route_id and tx_results. Each accepted item includes verification:
// {transaction_hash:[32 bytes],domain:[32 bytes],exact_fee:u64,key_images:[[32 bytes]],
//  outputs:[{public_key:[32 bytes],commitment:[32 bytes],encrypted_amount:[8 bytes],
//            view_tag:u8}]}. Rejected items have NO verification summary.
// Batch accepted/proof_verified is true only if ALL items pass; all items sharing
// a key image in that batch are rejected. Mixed-batch valid non-conflicting items
// can retain their individual proof_verified summary. This never commits a batch.
// ALWAYS false in v2: state_materialized, ledger_committed, post_quantum,
// ring_ledger_authorization_verified, historical_double_spend_checked. Success
// means only cryptographic verification against the supplied public context.
// Host still enforces ledger membership, historical double-spend rejection and
// atomic asset commit; independent requests do not maintain a spent-image set.
// v1 request/response/cache behavior above remains unchanged. v2 schema/size
// errors return -2 plus failure JSON; proof/backend/license rejection returns 0
// plus a non-accepted response. Infrastructure failure (including unavailable
// entropy) or a caught internal panic returns -4/status=Failed, fail-closed.
// Inspect response.accepted, not only the C return code. Feature-not-built is -5.
// Output memory is allocated by AOEM and must be released with aoem_free.
AOEM_API int32_t aoem_privacy_execute_v1(
  const uint8_t* request_ptr,
  size_t request_len,
  uint8_t** out_response_ptr,
  size_t* out_response_len
);
// KMS/HSM sign baseline ABI (host integration hook).
// Mode selection:
// - AOEM_FFI_KMS_MODE=local|plugin|none   (default: local)
// - AOEM_FFI_HSM_MODE=local|plugin|none   (default: local)
// - In local mode, KMS/HSM calls route to local ML-DSA signer.
// - In plugin mode, AOEM tries sidecar plugin symbols:
//   aoem_kms_sign_v1 / aoem_hsm_sign_v1 / aoem_free.
//   AOEM copies plugin signature output into host-owned buffer before return,
//   and uses plugin aoem_free to release plugin-owned temporary output.
// - In none mode, returns capability-not-built semantics (-5).
// Optional plugin discovery env:
// - AOEM_FFI_KMS_PLUGIN / AOEM_FFI_KMS_PLUGIN_DIR
// - AOEM_FFI_HSM_PLUGIN / AOEM_FFI_HSM_PLUGIN_DIR
// v1 uses the same signature contract as ML-DSA sign:
// - level: 44/65/87 (legacy 2/3/5 aliases accepted by Rust implementation)
// - key_material: raw private key bytes (provider-resolved by host side)
// output memory is allocated by AOEM and must be released with aoem_free.
AOEM_API int32_t aoem_kms_sign_v1(
  uint32_t level,
  const uint8_t* key_material_ptr,
  size_t key_material_len,
  const uint8_t* message_ptr,
  size_t message_len,
  uint8_t** out_signature_ptr,
  size_t* out_signature_len
);
AOEM_API int32_t aoem_hsm_sign_v1(
  uint32_t level,
  const uint8_t* key_material_ptr,
  size_t key_material_len,
  const uint8_t* message_ptr,
  size_t message_len,
  uint8_t** out_signature_ptr,
  size_t* out_signature_len
);
// Internal/compatibility ABI: scheduler heuristic helper, not a primary host contract.
AOEM_API uint32_t aoem_recommend_parallelism(
  uint64_t txs,
  uint32_t batch,
  uint64_t key_space,
  double rw
);
// Production ABI: creates and destroys AOEM execution contexts.
AOEM_API void* aoem_create(void);
AOEM_API void* aoem_create_with_options(const aoem_create_options_v1* opts);
AOEM_API void aoem_destroy(void* handle);
// Internal/compatibility ABI: disabled by default in production profile; prefer
// aoem_execute_batch or aoem_execute_ops_*.
AOEM_API int32_t aoem_execute(
  void* handle,
  const uint8_t* input_ptr,
  size_t input_len,
  uint8_t** output_ptr,
  size_t* output_len
);
// Production ABI: primary batch execution entrypoint.
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
// Production ABI: domain-neutral opaque semantic task scheduling.
// AOEM runs callbacks concurrently on its resident worker pool and preserves
// descriptor/output ordering. The callback must be thread-safe and must not
// unwind across the C ABI boundary.
AOEM_API int32_t aoem_execute_semantic_batch_v1(
  void* handle,
  aoem_semantic_task_v1* tasks_ptr,
  uint32_t task_count,
  aoem_semantic_task_callback_v1 callback,
  void* user_data,
  aoem_exec_v2_result* out_result
);
// Production ABI: nonblocking domain-neutral semantic scheduling. Inputs are
// copied before return; completion is called exactly once after all callbacks.
AOEM_API int32_t aoem_submit_semantic_batch_v1(
  void* handle,
  const aoem_semantic_input_v1* inputs_ptr,
  uint32_t input_count,
  aoem_semantic_input_callback_v1 callback,
  aoem_semantic_completion_callback_v1 completion,
  void* user_data
);
// Production ABI: nonblocking domain-neutral semantic continuation graph.
// Seed inputs are copied before return. Each callback may emit zero or more
// child inputs; AOEM copies and schedules them on the same resident worker pool.
// Completion runs exactly once after every seed and emitted task has finished.
AOEM_API int32_t aoem_submit_semantic_graph_v1(
  void* handle,
  const aoem_semantic_input_v1* seeds_ptr,
  uint32_t seed_count,
  aoem_semantic_graph_callback_v1 callback,
  aoem_semantic_completion_callback_v1 completion,
  void* user_data
);
// Production ABI: bounded, nonblocking, fixed-descriptor semantic task graph.
// AOEM is the sole scheduler. max_queued_tasks/event_capacity bound concurrent
// residency, not total graph work. Context retain/release calls are balanced.
AOEM_API int32_t aoem_submit_semantic_graph_v2(
  void* handle,
  const aoem_task_descriptor_v2* seeds_ptr,
  uint32_t seed_count,
  const aoem_graph_submit_options_v2* options,
  const aoem_graph_callbacks_v2* callbacks
);
// Production ABI: V2 scheduler plus fixed atomic write-set admission. The
// writer is bound once to an already-open AOEM RocksDB provider handle.
AOEM_API int32_t aoem_bind_semantic_atomic_writer_v1(
  void* handle,
  uint64_t database_id,
  uint32_t queue_capacity,
  uint32_t max_batch_sets
);
AOEM_API int32_t aoem_submit_semantic_graph_v3(
  void* handle,
  const aoem_task_descriptor_v2* seeds_ptr,
  uint32_t seed_count,
  const aoem_graph_submit_options_v3* options,
  const aoem_graph_callbacks_v3* callbacks
);
AOEM_API int32_t aoem_cancel_semantic_graph_v2(
  void* handle,
  uint64_t graph_id
);
AOEM_API uint64_t aoem_semantic_graph_v2_active_count(void* handle);
// Expert ABI: trusted-host typed operation execution.
// This is the highest-throughput struct-array fast path for in-process/C/C++/Rust
// integrations and performance baselines. Prefer aoem_execute_ops_wire_v1 as the
// default product ABI for cross-language, replayable, black-box host ingestion.
AOEM_API int32_t aoem_execute_ops_v2(
  void* handle,
  const aoem_op_v2* ops_ptr,
  uint32_t op_count,
  aoem_exec_v2_result* out_result
);
// Production ABI: default generic ops-wire ingestion.
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
// Product positioning: this is the default public execution ABI. aoem_execute_ops_v2
// remains available as an expert fast path for trusted hosts and apples-to-apples
// performance comparison.
AOEM_API int32_t aoem_execute_ops_wire_v1(
  void* handle,
  const uint8_t* input_ptr,
  size_t input_len,
  aoem_exec_v2_result* out_result
);

// AOAI v2 resident causal-sequence protocol carried only as the value of
// compute.ai.sgm_infer_v1 in aoem_execute_ops_wire_v1. Training and inference
// execute through AOEM Vulkan/SPIR-V; no AOEM implementation type crosses this
// boundary. Existing TrainStep remains update + post-commit evaluation.
// SetTrainingWorkspace explicitly seals the resident token-row capacity before
// any training or inference. The default is 1024 and the public maximum is
// 8192; AOEM never changes it through hardware detection or hidden fallback.
// TrainUpdate performs BPTT + AdamW without a second metrics forward pass.
// EvaluateBatch performs metrics-only forward execution without changing the
// parameter version or optimizer step. Both batch commands use the same body as
// TrainStep. These are research/training controls and must be recorded in model
// provenance; they do not create a second released-model inference mode.
// SetBaseTrainingLearningRateScale seals a finite scale in [1e-6, 1] before any
// training or inference starts. It applies only to base causal-language AdamW
// updates; typed, semantic, and cognition-language updates retain their sealed
// optimizer rate.
// APFLSEQ14 output-adapter language training freezes the causal core and both
// distinct vocabulary Banks, updates only the package's existing output-adapter
// tensor, and exports by replacing only that tensor. Train/evaluate carry
// expected_parameter_version:u64, expected_optimizer_step:u64, three u32 counts,
// reserved:u32, input tokens, target tokens, and a byte supervision mask. Export
// carries both expected versions followed by path_len:u32, reserved:u32, and the
// UTF-8 destination path. These commands do not add an inference mode.
#define AOEM_AI_CAUSAL_SEQUENCE_WIRE_VERSION_V2 2u
#define AOEM_AI_CAUSAL_SEQUENCE_COMMAND_OPEN_V2 1u
#define AOEM_AI_CAUSAL_SEQUENCE_COMMAND_TRAIN_STEP_V2 2u
#define AOEM_AI_CAUSAL_SEQUENCE_COMMAND_PREFILL_V2 3u
#define AOEM_AI_CAUSAL_SEQUENCE_COMMAND_DECODE_V2 4u
#define AOEM_AI_CAUSAL_SEQUENCE_COMMAND_RESET_V2 5u
#define AOEM_AI_CAUSAL_SEQUENCE_COMMAND_EXPORT_V2 6u
#define AOEM_AI_CAUSAL_SEQUENCE_COMMAND_CLOSE_V2 7u
#define AOEM_AI_CAUSAL_SEQUENCE_COMMAND_QUERY_INFO_V2 8u
#define AOEM_AI_CAUSAL_SEQUENCE_COMMAND_SET_TRAINING_SCOPE_V2 9u
#define AOEM_AI_CAUSAL_SEQUENCE_COMMAND_POINTER_TRAIN_STEP_V2 10u
#define AOEM_AI_CAUSAL_SEQUENCE_COMMAND_INSPECT_OPERATOR_BANK_V2 11u
#define AOEM_AI_CAUSAL_SEQUENCE_COMMAND_SET_TRAINING_WORKSPACE_V2 12u
#define AOEM_AI_CAUSAL_SEQUENCE_COMMAND_TRAIN_UPDATE_V2 13u
#define AOEM_AI_CAUSAL_SEQUENCE_COMMAND_EVALUATE_BATCH_V2 14u
#define AOEM_AI_CAUSAL_SEQUENCE_COMMAND_EXECUTE_TYPED_PROGRAM_V2 15u
#define AOEM_AI_CAUSAL_SEQUENCE_COMMAND_TRAIN_TYPED_PROGRAM_V2 16u
#define AOEM_AI_CAUSAL_SEQUENCE_COMMAND_EXECUTE_SEMANTIC_PROGRAM_V2 17u
#define AOEM_AI_CAUSAL_SEQUENCE_COMMAND_TRAIN_SEMANTIC_PROGRAM_V2 18u
#define AOEM_AI_CAUSAL_SEQUENCE_COMMAND_TRAIN_COGNITION_LANGUAGE_V2 19u
#define AOEM_AI_CAUSAL_SEQUENCE_COMMAND_EVALUATE_COGNITION_LANGUAGE_V2 20u
#define AOEM_AI_CAUSAL_SEQUENCE_COMMAND_SET_BASE_TRAINING_LEARNING_RATE_SCALE_V2 21u
#define AOEM_AI_CAUSAL_SEQUENCE_COMMAND_CAPTURE_MATRIX_TRAJECTORY_V2 22u
#define AOEM_AI_CAUSAL_SEQUENCE_COMMAND_TRAIN_OUTPUT_ADAPTER_LANGUAGE_V2 23u
#define AOEM_AI_CAUSAL_SEQUENCE_COMMAND_EVALUATE_OUTPUT_ADAPTER_LANGUAGE_V2 24u
#define AOEM_AI_CAUSAL_SEQUENCE_COMMAND_EXPORT_OUTPUT_ADAPTER_LANGUAGE_V2 25u
#define AOEM_AI_CAUSAL_SEQUENCE_COMMAND_TRAIN_LANGUAGE_RESIDUAL_V2 26u
#define AOEM_AI_CAUSAL_SEQUENCE_COMMAND_EVALUATE_LANGUAGE_RESIDUAL_V2 27u
#define AOEM_AI_CAUSAL_SEQUENCE_COMMAND_EXPORT_LANGUAGE_RESIDUAL_V2 28u
#define AOEM_AI_CAUSAL_SEQUENCE_DEFAULT_TRAINING_TOKEN_ROWS_V2 1024u
#define AOEM_AI_CAUSAL_SEQUENCE_MAX_TRAINING_TOKEN_ROWS_V2 8192u

// Typed-relation resident training session protocol carried only as the value
// of compute.ai.sgm_infer_v1 in aoem_execute_ops_wire_v1. All integers and FP32
// bit patterns are little-endian. Request header (32 bytes):
//   "AOTR\0", version:u16, command:u8, reserved:u8, header_len:u8,
//   reserved[2], body_len:u32, session_id:u64, request_id:u64.
// Result header (40 bytes) replaces the magic with "AOTS\0" and appends
// status:i32,reserved:u32. Reserved fields must be zero. Create seals one arm,
// dimensions, optimizer, global norm clip, and initialization seed. Train and
// evaluate carry typed entity/relation ID arrays. TrainCycle repeats the same
// Train update over an encoded fixed-batch cycle while state remains resident;
// its numerical and optimizer ordering is identical to repeated Train commands.
// TrainCycle body: expected_parameter_version:u64, update_count:u32,
// batch_count:u32, then repeated batch_body_len:u32, reserved:u32,
// batch_body[batch_body_len]. Its result body is parameter_version:u64,
// optimizer_step:u64, updates_applied:u32, batch_count:u32, mean_ce:f32,
// pointer_exact:f32, pre_clip_gradient_norm:f32, applied_scale:f32.
// Export returns an APFLTRM v1 digest-bound parameter material. No Vulkan handle
// or AOEM Rust layout crosses this protocol boundary.
#define AOEM_AI_TYPED_RELATION_WIRE_VERSION_V1 1u
#define AOEM_AI_TYPED_RELATION_COMMAND_CREATE_V1 1u
#define AOEM_AI_TYPED_RELATION_COMMAND_TRAIN_V1 2u
#define AOEM_AI_TYPED_RELATION_COMMAND_EVALUATE_V1 3u
#define AOEM_AI_TYPED_RELATION_COMMAND_EXPORT_V1 4u
#define AOEM_AI_TYPED_RELATION_COMMAND_CLOSE_V1 5u
#define AOEM_AI_TYPED_RELATION_COMMAND_QUERY_INFO_V1 6u
#define AOEM_AI_TYPED_RELATION_COMMAND_TRAIN_CYCLE_V1 7u
#define AOEM_AI_TYPED_RELATION_ARM_EQUIVARIANT_CANDIDATE_V1 1u
#define AOEM_AI_TYPED_RELATION_ARM_CAUSAL_TUPLE_ATTENTION_V1 2u
#define AOEM_AI_TYPED_RELATION_ARM_INVARIANT_SET_POOL_V1 3u
// Production ABI: host-neutral binary RocksDB provider.
//
// Request:  AOSQ | version:u16 | opcode:u16 | payload_len:u32 | payload.
// Response: AOSR | version:u16 | opcode:u16 | status:i32 | payload_len:u32 | payload.
// All integers are little-endian. Opcodes:
//   1 open, 2 close, 3 get, 4 multi_get, 5 write_batch,
//   6 snapshot_open, 7 snapshot_close, 8 scan_range.
// Payloads:
//   open: path_len:u32, path[path_len], max_open_files:u32,
//     write_buffer_bytes:u64, block_cache_bytes:u64,
//     max_background_jobs:u32, sync_every:u32, compression:u8.
//   close: db_id:u64.
//   get: db_id:u64, snapshot_id:u64, key_len:u32, key[key_len].
//   multi_get: db_id:u64, snapshot_id:u64, count:u32,
//     repeated key_len:u32, key[key_len].
//   write_batch: db_id:u64, count:u32, repeated kind:u8 (1=put,2=delete),
//     key_len:u32, key[key_len], and for put value_len:u32, value[value_len].
//   snapshot_open: db_id:u64. snapshot_close: db_id:u64, snapshot_id:u64.
//   scan_range: db_id:u64, snapshot_id:u64, start_len:u32, start[start_len],
//     end_len:u32, end_exclusive[end_len], limit:u32 (1..4096).
// Successful open/snapshot_open responses contain the returned u64 id.
// Read responses contain count:u32 followed by repeated found:u8,
// value_len:u32, value[value_len]. Error response payloads are UTF-8 details.
// Range responses contain count:u32 followed by repeated key_len:u32, key,
// value_len:u32, value. Range order is the database bytewise key order.
// Every database operation carries db_id:u64. Read operations additionally
// carry snapshot_id:u64 (zero selects the live database). One AOEM context may
// own multiple independent database ids; different ids have independent DB
// handles, WALs, caches, batches, and snapshot sets and may execute in parallel.
// The same path cannot be opened twice in one context.
// Values are opaque bytes; this contract never encodes JSON or host schemas.
// Available when AOEM is built with the rocksdb-persistence feature.
AOEM_API int32_t aoem_storage_provider_wire_v1(
  void* handle,
  const uint8_t* request_ptr,
  size_t request_len,
  uint8_t** out_ptr,
  size_t* out_len
);
// Production ABI: JSON state materialization/read/snapshot surface.
// Request and response payloads are UTF-8 JSON envelopes allocated/freed with aoem_free.
AOEM_API int32_t aoem_state_write_v1(
  const uint8_t* request_ptr,
  size_t request_len,
  uint8_t** out_ptr,
  size_t* out_len
);
AOEM_API int32_t aoem_state_read_v1(
  const uint8_t* request_ptr,
  size_t request_len,
  uint8_t** out_ptr,
  size_t* out_len
);
AOEM_API int32_t aoem_state_snapshot_v1(
  const uint8_t* request_ptr,
  size_t request_len,
  uint8_t** out_ptr,
  size_t* out_len
);

#define AOEM_APFL_MODEL_PACKAGE_VERSION_CAUSAL_SEQUENCE_V1 3u
#define AOEM_APFL_MODEL_ARCHITECTURE_CAUSAL_SEQUENCE_V1 2u
#define AOEM_APFL_DTYPE_BF16 1u
#define AOEM_APFL_DTYPE_Q4_BLOCK64 2u
#define AOEM_APFL_DTYPE_F32 7u
#define AOEM_APFL_DTYPE_CAUSAL_SEQUENCE_PROGRAM 8u
#define AOEM_APFL_CAUSAL_SEQUENCE_PROGRAM_TENSOR_ID 4u
#define AOEM_APFL_CAUSAL_SEQUENCE_PROGRAM_HEADER_SIZE 256u
#define AOEM_APFL_CAUSAL_SEQUENCE_PROGRAM_VERSION_V1 1u
#define AOEM_APFL_CAUSAL_SEQUENCE_PROGRAM_VERSION_V2 2u
#define AOEM_APFL_CAUSAL_SEQUENCE_PROGRAM_VERSION_V3 3u
#define AOEM_APFL_CAUSAL_SEQUENCE_PROGRAM_VERSION_V4 4u
#define AOEM_APFL_CAUSAL_SEQUENCE_PROGRAM_VERSION_V5 5u
#define AOEM_APFL_CAUSAL_SEQUENCE_PROGRAM_VERSION_V6 6u
#define AOEM_APFL_CAUSAL_SEQUENCE_PROGRAM_VERSION_V7 7u
#define AOEM_APFL_CAUSAL_SEQUENCE_PROGRAM_VERSION_V8 8u
#define AOEM_APFL_CAUSAL_SEQUENCE_REQUIRED_FLAGS_V1 0x1fu
#define AOEM_APFL_CAUSAL_SEQUENCE_REQUIRED_FLAGS_V2 0x3fu
#define AOEM_APFL_CAUSAL_SEQUENCE_REQUIRED_FLAGS_V3 0x3fu
#define AOEM_APFL_CAUSAL_SEQUENCE_REQUIRED_FLAGS_V4 0x7fu
#define AOEM_APFL_CAUSAL_SEQUENCE_REQUIRED_FLAGS_V5 0xffu
#define AOEM_APFL_CAUSAL_SEQUENCE_REQUIRED_FLAGS_V6 0x1ffu
#define AOEM_APFL_CAUSAL_SEQUENCE_REQUIRED_FLAGS_V7 0x3ffu
#define AOEM_APFL_CAUSAL_SEQUENCE_REQUIRED_FLAGS_V8 0x7ffu
#define AOEM_APFL_CAUSAL_SEQUENCE_VOCABULARY_MEMORY_NONE 0u
#define AOEM_APFL_CAUSAL_SEQUENCE_VOCABULARY_MEMORY_CAUSAL_UNIQUE_TOKEN_GATE_V1 1u
#define AOEM_APFL_CAUSAL_SEQUENCE_VOCABULARY_MEMORY_CONTENT_ADDRESSED_TOKEN_POINTER_V1 2u
#define AOEM_APFL_CAUSAL_SEQUENCE_MEMORY_CONTINUOUS_RELATIONAL_MEMORY_V1 3u
#define AOEM_APFL_CAUSAL_SEQUENCE_MEMORY_CONTENT_INDUCED_RECURRENT_OPERATOR_BANK_V1 4u
#define AOEM_APFL_CAUSAL_SEQUENCE_MEMORY_ROLE_SLOT_RECURRENT_OPERATOR_BANK_V1 5u
#define AOEM_APFL_CAUSAL_SEQUENCE_MEMORY_CONTENT_UNIT_ROLE_OPERATOR_BANK_V1 6u
#define AOEM_APFL_CAUSAL_SEQUENCE_MEMORY_EXCHANGEABLE_CONTENT_SET_OPERATOR_BANK_V1 7u
#define AOEM_APFL_CAUSAL_SEQUENCE_CONTINUOUS_RELATIONAL_MEMORY_ADDRESS_WIDTH_V1 64u
#define AOEM_APFL_CAUSAL_SEQUENCE_CONTINUOUS_RELATIONAL_MEMORY_VALUE_WIDTH_V1 128u
#define AOEM_APFL_CAUSAL_SEQUENCE_CONTINUOUS_RELATIONAL_MEMORY_COMPOSITION_STEPS_V1 2u
#define AOEM_APFL_CAUSAL_SEQUENCE_CONTINUOUS_RELATIONAL_MEMORY_TENSOR_BASE_ID_V1 13u
#define AOEM_APFL_CAUSAL_SEQUENCE_OPERATOR_BANK_ADDRESS_WIDTH_V1 64u
#define AOEM_APFL_CAUSAL_SEQUENCE_OPERATOR_BANK_COMPOSITION_STEPS_V1 4u
#define AOEM_APFL_CAUSAL_SEQUENCE_OPERATOR_BANK_EQUATION_V1 1u
#define AOEM_APFL_CAUSAL_SEQUENCE_OPERATOR_BANK_TENSOR_BASE_ID_V1 13u
#define AOEM_APFL_CAUSAL_SEQUENCE_ROLE_SLOT_OPERATOR_BANK_SLOT_COUNT_V1 8u
#define AOEM_APFL_CAUSAL_SEQUENCE_ROLE_SLOT_OPERATOR_BANK_EQUATION_V1 1u
#define AOEM_APFL_CAUSAL_SEQUENCE_ROLE_SLOT_OPERATOR_BANK_SLOT_WEIGHT_TENSOR_ID_V1 26u
#define AOEM_APFL_CAUSAL_SEQUENCE_ROLE_SLOT_OPERATOR_BANK_SLOT_BIAS_TENSOR_ID_V1 27u
#define AOEM_APFL_CAUSAL_SEQUENCE_CONTENT_UNIT_ROLE_OPERATOR_BANK_CONTENT_WIDTH_V1 192u
#define AOEM_APFL_CAUSAL_SEQUENCE_CONTENT_UNIT_ROLE_OPERATOR_BANK_SLOT_COUNT_V1 8u
#define AOEM_APFL_CAUSAL_SEQUENCE_CONTENT_UNIT_ROLE_OPERATOR_BANK_EQUATION_V1 1u
#define AOEM_APFL_CAUSAL_SEQUENCE_CONTENT_UNIT_ROLE_OPERATOR_BANK_RETAIN_STATE_TENSOR_ID_V1 26u
#define AOEM_APFL_CAUSAL_SEQUENCE_CONTENT_UNIT_ROLE_OPERATOR_BANK_RETAIN_UNIT_TENSOR_ID_V1 27u
#define AOEM_APFL_CAUSAL_SEQUENCE_CONTENT_UNIT_ROLE_OPERATOR_BANK_RETAIN_BIAS_TENSOR_ID_V1 28u
#define AOEM_APFL_CAUSAL_SEQUENCE_CONTENT_UNIT_ROLE_OPERATOR_BANK_WRITE_UNIT_TENSOR_ID_V1 29u
#define AOEM_APFL_CAUSAL_SEQUENCE_CONTENT_UNIT_ROLE_OPERATOR_BANK_ROLE_WEIGHT_TENSOR_ID_V1 30u
#define AOEM_APFL_CAUSAL_SEQUENCE_CONTENT_UNIT_ROLE_OPERATOR_BANK_ROLE_BIAS_TENSOR_ID_V1 31u
#define AOEM_APFL_CAUSAL_SEQUENCE_EXCHANGEABLE_CONTENT_SET_CONTENT_WIDTH_V1 192u
#define AOEM_APFL_CAUSAL_SEQUENCE_EXCHANGEABLE_CONTENT_SET_CONTENT_SLOT_COUNT_V1 8u
#define AOEM_APFL_CAUSAL_SEQUENCE_EXCHANGEABLE_CONTENT_SET_ROLE_SLOT_COUNT_V1 8u
#define AOEM_APFL_CAUSAL_SEQUENCE_EXCHANGEABLE_CONTENT_SET_EQUATION_V1 1u
#define AOEM_APFL_CAUSAL_SEQUENCE_EXCHANGEABLE_CONTENT_SET_ASSIGNMENT_WEIGHT_TENSOR_ID_V1 32u
#define AOEM_APFL_CAUSAL_SEQUENCE_EXCHANGEABLE_CONTENT_SET_ASSIGNMENT_BIAS_TENSOR_ID_V1 33u
#define AOEM_APFL_CAUSAL_SEQUENCE_PARAMETER_LAYOUT_VERSION_V1 1u
#define AOEM_APFL_CAUSAL_SEQUENCE_LAYER_TENSOR_STRIDE_V1 16u
#define AOEM_AI_CAUSAL_SEQUENCE_PROGRAM_INFO_VERSION_V1 1u
#define AOEM_AI_CAUSAL_SEQUENCE_PACKAGE_INFO_VERSION_V1 1u
#define AOEM_AI_CAUSAL_SEQUENCE_PROGRAM_INFO_VERSION_V2 2u
#define AOEM_AI_CAUSAL_SEQUENCE_PACKAGE_INFO_VERSION_V2 2u
#define AOEM_AI_CAUSAL_SEQUENCE_PROGRAM_INFO_VERSION_V3 3u
#define AOEM_AI_CAUSAL_SEQUENCE_PACKAGE_INFO_VERSION_V3 3u
#define AOEM_AI_CAUSAL_SEQUENCE_PROGRAM_INFO_VERSION_V4 4u
#define AOEM_AI_CAUSAL_SEQUENCE_PACKAGE_INFO_VERSION_V4 4u
#define AOEM_AI_CAUSAL_SEQUENCE_PROGRAM_INFO_VERSION_V5 5u
#define AOEM_AI_CAUSAL_SEQUENCE_PACKAGE_INFO_VERSION_V5 5u
#define AOEM_AI_CAUSAL_SEQUENCE_PROGRAM_INFO_VERSION_V6 6u
#define AOEM_AI_CAUSAL_SEQUENCE_PACKAGE_INFO_VERSION_V6 6u
#define AOEM_AI_CAUSAL_SEQUENCE_PROGRAM_INFO_VERSION_V7 7u
#define AOEM_AI_CAUSAL_SEQUENCE_PACKAGE_INFO_VERSION_V7 7u
#define AOEM_AI_CAUSAL_SEQUENCE_PROGRAM_INFO_VERSION_V8 8u
#define AOEM_AI_CAUSAL_SEQUENCE_PACKAGE_INFO_VERSION_V8 8u

// Validate the canonical APFLSEQ1 program tensor before opening a future
// resident sequence-training or inference session. V1 has one model semantic:
// causal multi-head attention, pre-RMSNorm, RoPE, SwiGLU, and resident KV cache.
// The program is metadata inside the final APFL package; model parameters remain
// independent package tensors. Callers initialize struct_size and version.
typedef struct aoem_ai_causal_sequence_program_info_v1 {
  uint32_t struct_size;
  uint32_t version;
  uint64_t flags;
  uint32_t layer_count;
  uint32_t model_width;
  uint32_t attention_heads;
  uint32_t key_value_heads;
  uint32_t head_dim;
  uint32_t ffn_width;
  uint32_t vocabulary_width;
  uint32_t max_sequence_length;
  float rms_norm_epsilon;
  float rope_theta;
  float attention_scale;
  float final_logit_softcap;
  uint64_t parameter_count;
  uint64_t initialization_seed;
  uint32_t vocabulary_bank_tensor_id;
  uint32_t input_adapter_tensor_id;
  uint32_t final_norm_tensor_id;
  uint32_t output_adapter_tensor_id;
  uint32_t layer_tensor_id_base;
  uint32_t layer_tensor_id_stride;
  uint32_t parameter_tensor_count;
  uint32_t reserved0;
  uint8_t tokenizer_digest[32];
  uint8_t vocabulary_bank_digest[32];
  uint8_t contract_digest[32];
} aoem_ai_causal_sequence_program_info_v1;

// Package-level result after validating APFLSEQ1, the fixed Q4 vocabulary Bank,
// all parameter tensor records and payload digests, tokenizer identity, model
// header identity, and one uniform F32-training or BF16-product storage mode.
typedef struct aoem_ai_causal_sequence_package_info_v1 {
  uint32_t struct_size;
  uint32_t version;
  aoem_ai_causal_sequence_program_info_v1 program;
  uint32_t package_version;
  uint32_t architecture;
  uint32_t parameter_storage_dtype;
  uint32_t package_tensor_count;
  uint32_t vocabulary_size;
  uint32_t reserved0;
  uint8_t model_root[32];
} aoem_ai_causal_sequence_package_info_v1;

AOEM_API const char* aoem_ai_causal_sequence_last_error_v1(void);
AOEM_API int32_t aoem_ai_causal_sequence_program_validate_v1(
  const uint8_t* program_ptr,
  size_t program_len,
  aoem_ai_causal_sequence_program_info_v1* out_info
);
AOEM_API int32_t aoem_ai_causal_sequence_package_validate_v1(
  const uint8_t* package_path_utf8_ptr,
  size_t package_path_utf8_len,
  aoem_ai_causal_sequence_package_info_v1* out_info
);

// V2 reports the exact APFLSEQ program sub-contract and the learned causal
// vocabulary-Memory parameter bindings. The V1 validation ABI intentionally
// rejects contracts it cannot fully describe.
typedef struct aoem_ai_causal_sequence_program_info_v2 {
  uint32_t struct_size;
  uint32_t version;
  uint64_t flags;
  uint32_t layer_count;
  uint32_t model_width;
  uint32_t attention_heads;
  uint32_t key_value_heads;
  uint32_t head_dim;
  uint32_t ffn_width;
  uint32_t vocabulary_width;
  uint32_t max_sequence_length;
  float rms_norm_epsilon;
  float rope_theta;
  float attention_scale;
  float final_logit_softcap;
  uint64_t parameter_count;
  uint64_t initialization_seed;
  uint32_t vocabulary_bank_tensor_id;
  uint32_t input_adapter_tensor_id;
  uint32_t final_norm_tensor_id;
  uint32_t output_adapter_tensor_id;
  uint32_t layer_tensor_id_base;
  uint32_t layer_tensor_id_stride;
  uint32_t parameter_tensor_count;
  uint32_t reserved0;
  uint8_t tokenizer_digest[32];
  uint8_t vocabulary_bank_digest[32];
  uint8_t contract_digest[32];
  uint32_t program_contract_version;
  uint32_t vocabulary_memory_kind;
  uint32_t vocabulary_memory_gate_weight_tensor_id;
  uint32_t vocabulary_memory_gate_bias_tensor_id;
} aoem_ai_causal_sequence_program_info_v2;

typedef struct aoem_ai_causal_sequence_package_info_v2 {
  uint32_t struct_size;
  uint32_t version;
  aoem_ai_causal_sequence_program_info_v2 program;
  uint32_t package_version;
  uint32_t architecture;
  uint32_t parameter_storage_dtype;
  uint32_t package_tensor_count;
  uint32_t vocabulary_size;
  uint32_t reserved0;
  uint8_t model_root[32];
} aoem_ai_causal_sequence_package_info_v2;

AOEM_API int32_t aoem_ai_causal_sequence_program_validate_v2(
  const uint8_t* program_ptr,
  size_t program_len,
  aoem_ai_causal_sequence_program_info_v2* out_info
);
AOEM_API int32_t aoem_ai_causal_sequence_package_validate_v2(
  const uint8_t* package_path_utf8_ptr,
  size_t package_path_utf8_len,
  aoem_ai_causal_sequence_package_info_v2* out_info
);

// V3 adds the complete learned content-addressed token-pointer Memory contract.
// Query and Key are projected from APFL semantic states, relative-position bias
// selects a causal history position, and the selected historical token identity
// is scattered into the vocabulary logits. V1/V2 validation ABIs reject V3
// because they cannot fully describe these model parameters.
typedef struct aoem_ai_causal_sequence_program_info_v3 {
  uint32_t struct_size;
  uint32_t version;
  uint64_t flags;
  uint32_t layer_count;
  uint32_t model_width;
  uint32_t attention_heads;
  uint32_t key_value_heads;
  uint32_t head_dim;
  uint32_t ffn_width;
  uint32_t vocabulary_width;
  uint32_t max_sequence_length;
  float rms_norm_epsilon;
  float rope_theta;
  float attention_scale;
  float final_logit_softcap;
  uint64_t parameter_count;
  uint64_t initialization_seed;
  uint32_t vocabulary_bank_tensor_id;
  uint32_t input_adapter_tensor_id;
  uint32_t final_norm_tensor_id;
  uint32_t output_adapter_tensor_id;
  uint32_t layer_tensor_id_base;
  uint32_t layer_tensor_id_stride;
  uint32_t parameter_tensor_count;
  uint32_t reserved0;
  uint8_t tokenizer_digest[32];
  uint8_t vocabulary_bank_digest[32];
  uint8_t contract_digest[32];
  uint32_t program_contract_version;
  uint32_t vocabulary_memory_kind;
  uint32_t vocabulary_memory_gate_weight_tensor_id;
  uint32_t vocabulary_memory_gate_bias_tensor_id;
  uint32_t vocabulary_memory_query_weight_tensor_id;
  uint32_t vocabulary_memory_key_weight_tensor_id;
  uint32_t vocabulary_memory_relative_position_bias_tensor_id;
  uint32_t vocabulary_memory_address_width;
} aoem_ai_causal_sequence_program_info_v3;

typedef struct aoem_ai_causal_sequence_package_info_v3 {
  uint32_t struct_size;
  uint32_t version;
  aoem_ai_causal_sequence_program_info_v3 program;
  uint32_t package_version;
  uint32_t architecture;
  uint32_t parameter_storage_dtype;
  uint32_t package_tensor_count;
  uint32_t vocabulary_size;
  uint32_t reserved0;
  uint8_t model_root[32];
} aoem_ai_causal_sequence_package_info_v3;

AOEM_API int32_t aoem_ai_causal_sequence_program_validate_v3(
  const uint8_t* program_ptr,
  size_t program_len,
  aoem_ai_causal_sequence_program_info_v3* out_info
);
AOEM_API int32_t aoem_ai_causal_sequence_package_validate_v3(
  const uint8_t* package_path_utf8_ptr,
  size_t package_path_utf8_len,
  aoem_ai_causal_sequence_package_info_v3* out_info
);

// V4 is the fixed APFLSEQ4 continuous relational Memory contract. The package
// seals a 64-wide address projection, a learned 128-wide continuous Value, one
// shared relation projection, exactly two resident causal composition reads,
// and a gated residual back into the language state. Tensor ids are derived
// from the program's byte-140 base in the fixed order reported below. V3 and
// older validators reject this contract rather than silently treating it as a
// token-pointer Memory.
typedef struct aoem_ai_causal_sequence_program_info_v4 {
  uint32_t struct_size;
  uint32_t version;
  uint64_t flags;
  uint32_t layer_count;
  uint32_t model_width;
  uint32_t attention_heads;
  uint32_t key_value_heads;
  uint32_t head_dim;
  uint32_t ffn_width;
  uint32_t vocabulary_width;
  uint32_t max_sequence_length;
  float rms_norm_epsilon;
  float rope_theta;
  float attention_scale;
  float final_logit_softcap;
  uint64_t parameter_count;
  uint64_t initialization_seed;
  uint32_t vocabulary_bank_tensor_id;
  uint32_t input_adapter_tensor_id;
  uint32_t final_norm_tensor_id;
  uint32_t output_adapter_tensor_id;
  uint32_t layer_tensor_id_base;
  uint32_t layer_tensor_id_stride;
  uint32_t parameter_tensor_count;
  uint32_t reserved0;
  uint8_t tokenizer_digest[32];
  uint8_t vocabulary_bank_digest[32];
  uint8_t contract_digest[32];
  uint32_t program_contract_version;
  uint32_t memory_kind;
  uint32_t continuous_memory_tensor_id_base;
  uint32_t continuous_memory_query_weight_tensor_id;
  uint32_t continuous_memory_key_weight_tensor_id;
  uint32_t continuous_memory_value_weight_tensor_id;
  uint32_t continuous_memory_relation_weight_tensor_id;
  uint32_t continuous_memory_output_weight_tensor_id;
  uint32_t continuous_memory_relative_position_bias_tensor_id;
  uint32_t continuous_memory_gate_state_weight_tensor_id;
  uint32_t continuous_memory_gate_value_weight_tensor_id;
  uint32_t continuous_memory_gate_bias_tensor_id;
  uint32_t continuous_memory_address_width;
  uint32_t continuous_memory_value_width;
  uint32_t continuous_memory_composition_steps;
  uint32_t reserved1;
} aoem_ai_causal_sequence_program_info_v4;

typedef struct aoem_ai_causal_sequence_package_info_v4 {
  uint32_t struct_size;
  uint32_t version;
  aoem_ai_causal_sequence_program_info_v4 program;
  uint32_t package_version;
  uint32_t architecture;
  uint32_t parameter_storage_dtype;
  uint32_t package_tensor_count;
  uint32_t vocabulary_size;
  uint32_t reserved0;
  uint8_t model_root[32];
} aoem_ai_causal_sequence_package_info_v4;

AOEM_API int32_t aoem_ai_causal_sequence_program_validate_v4(
  const uint8_t* program_ptr,
  size_t program_len,
  aoem_ai_causal_sequence_program_info_v4* out_info
);
AOEM_API int32_t aoem_ai_causal_sequence_package_validate_v4(
  const uint8_t* package_path_utf8_ptr,
  size_t package_path_utf8_len,
  aoem_ai_causal_sequence_package_info_v4* out_info
);

// V5 seals a strict-causal content-induced recurrent OperatorBank. Earlier
// states write learned Key/Value outer products into a resident 64x64 Bank;
// the current Query applies the same learned update exactly four times before
// a gated residual returns to the canonical vocabulary surface. V4 and older
// validators reject this contract rather than substituting an older Memory
// equation.
typedef struct aoem_ai_causal_sequence_program_info_v5 {
  uint32_t struct_size;
  uint32_t version;
  uint64_t flags;
  uint32_t layer_count;
  uint32_t model_width;
  uint32_t attention_heads;
  uint32_t key_value_heads;
  uint32_t head_dim;
  uint32_t ffn_width;
  uint32_t vocabulary_width;
  uint32_t max_sequence_length;
  float rms_norm_epsilon;
  float rope_theta;
  float attention_scale;
  float final_logit_softcap;
  uint64_t parameter_count;
  uint64_t initialization_seed;
  uint32_t vocabulary_bank_tensor_id;
  uint32_t input_adapter_tensor_id;
  uint32_t final_norm_tensor_id;
  uint32_t output_adapter_tensor_id;
  uint32_t layer_tensor_id_base;
  uint32_t layer_tensor_id_stride;
  uint32_t parameter_tensor_count;
  uint32_t reserved0;
  uint8_t tokenizer_digest[32];
  uint8_t vocabulary_bank_digest[32];
  uint8_t contract_digest[32];
  uint32_t program_contract_version;
  uint32_t operator_bank_kind;
  uint32_t operator_bank_tensor_id_base;
  uint32_t operator_bank_query_weight_tensor_id;
  uint32_t operator_bank_key_weight_tensor_id;
  uint32_t operator_bank_value_weight_tensor_id;
  uint32_t operator_bank_output_weight_tensor_id;
  uint32_t operator_bank_write_gate_state_weight_tensor_id;
  uint32_t operator_bank_write_gate_bias_tensor_id;
  uint32_t operator_bank_compose_query_weight_tensor_id;
  uint32_t operator_bank_compose_value_weight_tensor_id;
  uint32_t operator_bank_compose_state_weight_tensor_id;
  uint32_t operator_bank_compose_bias_tensor_id;
  uint32_t operator_bank_output_gate_state_weight_tensor_id;
  uint32_t operator_bank_output_gate_value_weight_tensor_id;
  uint32_t operator_bank_output_gate_bias_tensor_id;
  uint32_t operator_bank_address_width;
  uint32_t operator_bank_composition_steps;
  uint32_t operator_bank_equation;
  uint32_t reserved1;
} aoem_ai_causal_sequence_program_info_v5;

typedef struct aoem_ai_causal_sequence_package_info_v5 {
  uint32_t struct_size;
  uint32_t version;
  aoem_ai_causal_sequence_program_info_v5 program;
  uint32_t package_version;
  uint32_t architecture;
  uint32_t parameter_storage_dtype;
  uint32_t package_tensor_count;
  uint32_t vocabulary_size;
  uint32_t reserved0;
  uint8_t model_root[32];
} aoem_ai_causal_sequence_package_info_v5;

AOEM_API int32_t aoem_ai_causal_sequence_program_validate_v5(
  const uint8_t* program_ptr,
  size_t program_len,
  aoem_ai_causal_sequence_program_info_v5* out_info
);
AOEM_API int32_t aoem_ai_causal_sequence_package_validate_v5(
  const uint8_t* package_path_utf8_ptr,
  size_t package_path_utf8_len,
  aoem_ai_causal_sequence_package_info_v5* out_info
);

// V6 preserves the V5 recurrent OperatorBank and splits its strict-causal
// prefix Bank into eight anonymous competitive role slots. Slot assignment is
// learned from content; no slot has a handwritten language or task identity.
// V5 and older validators reject this contract rather than collapsing the
// learned role dimension into a single Bank.
typedef struct aoem_ai_causal_sequence_program_info_v6 {
  uint32_t struct_size;
  uint32_t version;
  uint64_t flags;
  uint32_t layer_count;
  uint32_t model_width;
  uint32_t attention_heads;
  uint32_t key_value_heads;
  uint32_t head_dim;
  uint32_t ffn_width;
  uint32_t vocabulary_width;
  uint32_t max_sequence_length;
  float rms_norm_epsilon;
  float rope_theta;
  float attention_scale;
  float final_logit_softcap;
  uint64_t parameter_count;
  uint64_t initialization_seed;
  uint32_t vocabulary_bank_tensor_id;
  uint32_t input_adapter_tensor_id;
  uint32_t final_norm_tensor_id;
  uint32_t output_adapter_tensor_id;
  uint32_t layer_tensor_id_base;
  uint32_t layer_tensor_id_stride;
  uint32_t parameter_tensor_count;
  uint32_t reserved0;
  uint8_t tokenizer_digest[32];
  uint8_t vocabulary_bank_digest[32];
  uint8_t contract_digest[32];
  uint32_t program_contract_version;
  uint32_t operator_bank_kind;
  uint32_t operator_bank_tensor_id_base;
  uint32_t operator_bank_query_weight_tensor_id;
  uint32_t operator_bank_key_weight_tensor_id;
  uint32_t operator_bank_value_weight_tensor_id;
  uint32_t operator_bank_output_weight_tensor_id;
  uint32_t operator_bank_write_gate_state_weight_tensor_id;
  uint32_t operator_bank_write_gate_bias_tensor_id;
  uint32_t operator_bank_compose_query_weight_tensor_id;
  uint32_t operator_bank_compose_value_weight_tensor_id;
  uint32_t operator_bank_compose_state_weight_tensor_id;
  uint32_t operator_bank_compose_bias_tensor_id;
  uint32_t operator_bank_output_gate_state_weight_tensor_id;
  uint32_t operator_bank_output_gate_value_weight_tensor_id;
  uint32_t operator_bank_output_gate_bias_tensor_id;
  uint32_t operator_bank_slot_weight_tensor_id;
  uint32_t operator_bank_slot_bias_tensor_id;
  uint32_t operator_bank_address_width;
  uint32_t operator_bank_slot_count;
  uint32_t operator_bank_composition_steps;
  uint32_t operator_bank_equation;
  uint32_t reserved1;
} aoem_ai_causal_sequence_program_info_v6;

typedef struct aoem_ai_causal_sequence_package_info_v6 {
  uint32_t struct_size;
  uint32_t version;
  aoem_ai_causal_sequence_program_info_v6 program;
  uint32_t package_version;
  uint32_t architecture;
  uint32_t parameter_storage_dtype;
  uint32_t package_tensor_count;
  uint32_t vocabulary_size;
  uint32_t reserved0;
  uint8_t model_root[32];
} aoem_ai_causal_sequence_package_info_v6;

AOEM_API int32_t aoem_ai_causal_sequence_program_validate_v6(
  const uint8_t* program_ptr,
  size_t program_len,
  aoem_ai_causal_sequence_program_info_v6* out_info
);
AOEM_API int32_t aoem_ai_causal_sequence_package_validate_v6(
  const uint8_t* package_path_utf8_ptr,
  size_t package_path_utf8_len,
  aoem_ai_causal_sequence_package_info_v6* out_info
);

// V7 replaces V6's per-token direct role selection with three persistent,
// strictly causal content units (query/key/value). A learned retain gate carries
// each content unit across tokens; normalized content jointly controls Bank
// writes and eight anonymous role slots. No role has a host-authored identity.
// V6 and older validators reject this contract.
typedef struct aoem_ai_causal_sequence_program_info_v7 {
  uint32_t struct_size;
  uint32_t version;
  uint64_t flags;
  uint32_t layer_count;
  uint32_t model_width;
  uint32_t attention_heads;
  uint32_t key_value_heads;
  uint32_t head_dim;
  uint32_t ffn_width;
  uint32_t vocabulary_width;
  uint32_t max_sequence_length;
  float rms_norm_epsilon;
  float rope_theta;
  float attention_scale;
  float final_logit_softcap;
  uint64_t parameter_count;
  uint64_t initialization_seed;
  uint32_t vocabulary_bank_tensor_id;
  uint32_t input_adapter_tensor_id;
  uint32_t final_norm_tensor_id;
  uint32_t output_adapter_tensor_id;
  uint32_t layer_tensor_id_base;
  uint32_t layer_tensor_id_stride;
  uint32_t parameter_tensor_count;
  uint32_t reserved0;
  uint8_t tokenizer_digest[32];
  uint8_t vocabulary_bank_digest[32];
  uint8_t contract_digest[32];
  uint32_t program_contract_version;
  uint32_t operator_bank_kind;
  uint32_t operator_bank_tensor_id_base;
  uint32_t operator_bank_query_weight_tensor_id;
  uint32_t operator_bank_key_weight_tensor_id;
  uint32_t operator_bank_value_weight_tensor_id;
  uint32_t operator_bank_output_weight_tensor_id;
  uint32_t operator_bank_write_gate_state_weight_tensor_id;
  uint32_t operator_bank_write_gate_bias_tensor_id;
  uint32_t operator_bank_compose_query_weight_tensor_id;
  uint32_t operator_bank_compose_value_weight_tensor_id;
  uint32_t operator_bank_compose_state_weight_tensor_id;
  uint32_t operator_bank_compose_bias_tensor_id;
  uint32_t operator_bank_output_gate_state_weight_tensor_id;
  uint32_t operator_bank_output_gate_value_weight_tensor_id;
  uint32_t operator_bank_output_gate_bias_tensor_id;
  uint32_t operator_bank_content_retain_state_weight_tensor_id;
  uint32_t operator_bank_content_retain_unit_weight_tensor_id;
  uint32_t operator_bank_content_retain_bias_tensor_id;
  uint32_t operator_bank_content_write_unit_weight_tensor_id;
  uint32_t operator_bank_content_role_weight_tensor_id;
  uint32_t operator_bank_content_role_bias_tensor_id;
  uint32_t operator_bank_address_width;
  uint32_t operator_bank_content_width;
  uint32_t operator_bank_slot_count;
  uint32_t operator_bank_composition_steps;
  uint32_t operator_bank_equation;
  uint32_t reserved1;
} aoem_ai_causal_sequence_program_info_v7;

typedef struct aoem_ai_causal_sequence_package_info_v7 {
  uint32_t struct_size;
  uint32_t version;
  aoem_ai_causal_sequence_program_info_v7 program;
  uint32_t package_version;
  uint32_t architecture;
  uint32_t parameter_storage_dtype;
  uint32_t package_tensor_count;
  uint32_t vocabulary_size;
  uint32_t reserved0;
  uint8_t model_root[32];
} aoem_ai_causal_sequence_package_info_v7;

AOEM_API int32_t aoem_ai_causal_sequence_program_validate_v7(
  const uint8_t* program_ptr,
  size_t program_len,
  aoem_ai_causal_sequence_program_info_v7* out_info
);
AOEM_API int32_t aoem_ai_causal_sequence_package_validate_v7(
  const uint8_t* package_path_utf8_ptr,
  size_t package_path_utf8_len,
  aoem_ai_causal_sequence_package_info_v7* out_info
);

// V8 replaces V7's single recurrent content unit with eight anonymous,
// exchangeable content slots. Learned assignment and strict-causal content
// addressing select which persistent slot is updated and read. V7 and older
// validators reject this contract rather than collapsing the set to one unit.
typedef struct aoem_ai_causal_sequence_program_info_v8 {
  uint32_t struct_size;
  uint32_t version;
  uint64_t flags;
  uint32_t layer_count;
  uint32_t model_width;
  uint32_t attention_heads;
  uint32_t key_value_heads;
  uint32_t head_dim;
  uint32_t ffn_width;
  uint32_t vocabulary_width;
  uint32_t max_sequence_length;
  float rms_norm_epsilon;
  float rope_theta;
  float attention_scale;
  float final_logit_softcap;
  uint64_t parameter_count;
  uint64_t initialization_seed;
  uint32_t vocabulary_bank_tensor_id;
  uint32_t input_adapter_tensor_id;
  uint32_t final_norm_tensor_id;
  uint32_t output_adapter_tensor_id;
  uint32_t layer_tensor_id_base;
  uint32_t layer_tensor_id_stride;
  uint32_t parameter_tensor_count;
  uint32_t reserved0;
  uint8_t tokenizer_digest[32];
  uint8_t vocabulary_bank_digest[32];
  uint8_t contract_digest[32];
  uint32_t program_contract_version;
  uint32_t operator_bank_kind;
  uint32_t operator_bank_tensor_id_base;
  uint32_t operator_bank_query_weight_tensor_id;
  uint32_t operator_bank_key_weight_tensor_id;
  uint32_t operator_bank_value_weight_tensor_id;
  uint32_t operator_bank_output_weight_tensor_id;
  uint32_t operator_bank_write_gate_state_weight_tensor_id;
  uint32_t operator_bank_write_gate_bias_tensor_id;
  uint32_t operator_bank_compose_query_weight_tensor_id;
  uint32_t operator_bank_compose_value_weight_tensor_id;
  uint32_t operator_bank_compose_state_weight_tensor_id;
  uint32_t operator_bank_compose_bias_tensor_id;
  uint32_t operator_bank_output_gate_state_weight_tensor_id;
  uint32_t operator_bank_output_gate_value_weight_tensor_id;
  uint32_t operator_bank_output_gate_bias_tensor_id;
  uint32_t operator_bank_content_retain_state_weight_tensor_id;
  uint32_t operator_bank_content_retain_unit_weight_tensor_id;
  uint32_t operator_bank_content_retain_bias_tensor_id;
  uint32_t operator_bank_content_write_unit_weight_tensor_id;
  uint32_t operator_bank_content_role_weight_tensor_id;
  uint32_t operator_bank_content_role_bias_tensor_id;
  uint32_t operator_bank_content_assignment_weight_tensor_id;
  uint32_t operator_bank_content_assignment_bias_tensor_id;
  uint32_t operator_bank_address_width;
  uint32_t operator_bank_content_width;
  uint32_t operator_bank_content_slot_count;
  uint32_t operator_bank_slot_count;
  uint32_t operator_bank_composition_steps;
  uint32_t operator_bank_equation;
  uint32_t reserved1;
} aoem_ai_causal_sequence_program_info_v8;

typedef struct aoem_ai_causal_sequence_package_info_v8 {
  uint32_t struct_size;
  uint32_t version;
  aoem_ai_causal_sequence_program_info_v8 program;
  uint32_t package_version;
  uint32_t architecture;
  uint32_t parameter_storage_dtype;
  uint32_t package_tensor_count;
  uint32_t vocabulary_size;
  uint32_t reserved0;
  uint8_t model_root[32];
} aoem_ai_causal_sequence_package_info_v8;

AOEM_API int32_t aoem_ai_causal_sequence_program_validate_v8(
  const uint8_t* program_ptr,
  size_t program_len,
  aoem_ai_causal_sequence_program_info_v8* out_info
);
AOEM_API int32_t aoem_ai_causal_sequence_package_validate_v8(
  const uint8_t* package_path_utf8_ptr,
  size_t package_path_utf8_len,
  aoem_ai_causal_sequence_package_info_v8* out_info
);

// V9 preserves V8's parameter and slot contracts while replacing per-slot
// post-write RMS normalization with amplitude-preserving convex content
// updates. The sealed program equation distinguishes V8 and V9 semantics.
typedef aoem_ai_causal_sequence_program_info_v8
  aoem_ai_causal_sequence_program_info_v9;
typedef aoem_ai_causal_sequence_package_info_v8
  aoem_ai_causal_sequence_package_info_v9;

AOEM_API int32_t aoem_ai_causal_sequence_program_validate_v9(
  const uint8_t* program_ptr,
  size_t program_len,
  aoem_ai_causal_sequence_program_info_v9* out_info
);
AOEM_API int32_t aoem_ai_causal_sequence_package_validate_v9(
  const uint8_t* package_path_utf8_ptr,
  size_t package_path_utf8_len,
  aoem_ai_causal_sequence_package_info_v9* out_info
);

#define AOEM_AI_SGM_HIERARCHICAL_TRAINING_RUNTIME_IDENTITY_VERSION_V1 1u
#define AOEM_AI_SGM_HIERARCHICAL_TRAINING_RUNTIME_BACKEND_VULKAN 1u

// Read the exact Vulkan physical-device and driver identity owned by an active
// hierarchical training session. Callers initialize struct_size and version.
// This is provenance only: it does not dispatch work or change session state.
typedef struct aoem_ai_sgm_hierarchical_training_runtime_identity_v1 {
  uint32_t struct_size;
  uint32_t version;
  uint32_t backend_kind;
  uint32_t flags;
  uint32_t api_version;
  uint32_t driver_version;
  uint32_t vendor_id;
  uint32_t device_id;
  uint32_t device_type;
  uint32_t queue_family_index;
  uint32_t reserved0;
  uint32_t reserved1;
  uint8_t device_uuid[16];
  uint8_t driver_uuid[16];
  uint8_t pipeline_cache_uuid[16];
} aoem_ai_sgm_hierarchical_training_runtime_identity_v1;

AOEM_API int32_t aoem_ai_sgm_hierarchical_train_get_runtime_identity_v1(
  uint64_t session_id,
  aoem_ai_sgm_hierarchical_training_runtime_identity_v1* out_identity
);

#define AOEM_AI_SGM_MODEL_SESSION_RUNTIME_IDENTITY_VERSION_V1 1u
#define AOEM_AI_SGM_MODEL_SESSION_RUNTIME_BACKEND_VULKAN 1u

// Read the exact Vulkan physical-device and driver identity owned by an active
// resident APFL model session opened through compute.ai.sgm_infer_v1. Callers
// initialize struct_size and version. This query is provenance-only.
typedef struct aoem_ai_sgm_model_session_runtime_identity_v1 {
  uint32_t struct_size;
  uint32_t version;
  uint32_t backend_kind;
  uint32_t flags;
  uint32_t api_version;
  uint32_t driver_version;
  uint32_t vendor_id;
  uint32_t device_id;
  uint32_t device_type;
  uint32_t queue_family_index;
  uint32_t reserved0;
  uint32_t reserved1;
  uint8_t device_uuid[16];
  uint8_t driver_uuid[16];
  uint8_t pipeline_cache_uuid[16];
} aoem_ai_sgm_model_session_runtime_identity_v1;

AOEM_API int32_t aoem_ai_sgm_model_session_get_runtime_identity_v1(
  uint64_t session_id,
  aoem_ai_sgm_model_session_runtime_identity_v1* out_identity
);
// AI SGM training ABI: hard-token forward with a differentiable sparse top-k
// surrogate backward path. The session must already own a Q4 vocabulary and
// this policy must be selected before the first optimizer step.
AOEM_API int32_t aoem_ai_sgm_hierarchical_train_set_straight_through_token_feedback_v1(
  uint64_t session_id,
  uint32_t top_k,
  float temperature
);

#define AOEM_AI_SGM_OPTIMIZER_UPDATE_SCOPE_CONFIG_VERSION_V1 1u
#define AOEM_AI_SGM_OPTIMIZER_UPDATE_SCOPE_CONFIG_FLAGS_V1 0u
// These family IDs are also the exact values returned in the family field of
// aoem_ai_sgm_group_common_descent_metrics_v1/v2/v3.
#define AOEM_AI_SGM_PARAMETER_FAMILY_COARSE_INPUT_V1 0u
#define AOEM_AI_SGM_PARAMETER_FAMILY_COARSE_RECURRENT_V1 1u
#define AOEM_AI_SGM_PARAMETER_FAMILY_GENERATOR_INPUT_V1 2u
#define AOEM_AI_SGM_PARAMETER_FAMILY_GENERATOR_RECURRENT_V1 3u
#define AOEM_AI_SGM_PARAMETER_FAMILY_SEMANTIC_READOUT_V1 4u
#define AOEM_AI_SGM_PARAMETER_FAMILY_ROUTER_CONTROL_V1 5u
#define AOEM_AI_SGM_PARAMETER_FAMILY_STAGE_MIX_V1 6u
#define AOEM_AI_SGM_PARAMETER_FAMILY_MEMORY_GATE_V1 7u
#define AOEM_AI_SGM_PARAMETER_FAMILY_MEMORY_OUTPUT_V1 8u
#define AOEM_AI_SGM_PARAMETER_FAMILY_LANGUAGE_SEMANTIC_V1 9u
#define AOEM_AI_SGM_PARAMETER_FAMILY_LANGUAGE_LEXICAL_V1 10u
#define AOEM_AI_SGM_PARAMETER_FAMILY_LANGUAGE_CONTROL_V1 11u
#define AOEM_AI_SGM_PARAMETER_FAMILY_CONDITIONAL_TRANSITION_V1 12u
#define AOEM_AI_SGM_PARAMETER_FAMILY_BIT_V1(family) \
  (UINT64_C(1) << (family))
#define AOEM_AI_SGM_OPTIMIZER_UPDATE_SCOPE_STAGE_FAMILY_MASK_V1 \
  UINT64_C(0x11ff)
#define AOEM_AI_SGM_OPTIMIZER_UPDATE_SCOPE_LANGUAGE_MANIFOLD_FAMILY_MASK_V1 \
  UINT64_C(0x0e00)

// Permanent per-parameter-family optimizer update scope. A set family bit is
// update-enabled; a clear family bit is frozen. stage_family_update_masks must
// contain exactly one mask for every configured computation stage and may use
// only AOEM_AI_SGM_OPTIMIZER_UPDATE_SCOPE_STAGE_FAMILY_MASK_V1 bits. The
// language-manifold mask uses the same global family-bit positions and may use
// only AOEM_AI_SGM_OPTIMIZER_UPDATE_SCOPE_LANGUAGE_MANIFOLD_FAMILY_MASK_V1.
// Zero is a valid mask and freezes every family in that scope.
typedef struct aoem_ai_sgm_optimizer_update_scope_config_v1 {
  uint32_t struct_size;
  uint32_t version;
  // Must equal AOEM_AI_SGM_OPTIMIZER_UPDATE_SCOPE_CONFIG_FLAGS_V1.
  uint32_t flags;
  uint32_t reserved0;
  const uint64_t* stage_family_update_masks;
  size_t stage_family_update_mask_count;
  uint64_t language_manifold_family_update_mask;
  uint64_t reserved1;
} aoem_ai_sgm_optimizer_update_scope_config_v1;

// Configure only while the session is at optimizer step zero, before any
// optimizer attempt or accumulated training microbatch. Without this call all
// existing and subsequently attached families are update-enabled, preserving
// prior behavior. Configuration may be replaced while these preconditions
// still hold; the masks become immutable once optimization starts.
//
// A frozen family receives no ordinary AdamW parameter or moment write and no
// decoupled weight decay. The same physical no-write contract applies to
// optimizer-aware group-common-descent and exact-forward transactions:
// parameters and AdamW first/second moments remain bitwise unchanged. Training
// may still compute gradients and pre-optimizer common-descent metrics for a
// frozen family, and global optimizer attempt/step scheduling still advances
// when the containing transaction otherwise commits.
AOEM_API int32_t aoem_ai_sgm_hierarchical_train_set_optimizer_update_scope_v1(
  uint64_t session_id,
  const aoem_ai_sgm_optimizer_update_scope_config_v1* config
);

#define AOEM_AI_SGM_PAIRED_SEQUENCE_PREFERENCE_CONFIG_VERSION_V1 1u
#define AOEM_AI_SGM_PAIRED_SEQUENCE_PREFERENCE_STATS_VERSION_V1 1u

// Versioned, domain-neutral paired hard-trajectory sequence preference objective.
// Lanes are adjacent pairs: lane 2p is chosen and lane 2p+1 is rejected.
// A pair contributes only when both lanes have at least one vocabulary-supervised token.
// The optimized objective is:
//   chosen_ce_scale * mean_pair(mean_token_ce_chosen)
//   + loss_scale * mean_pair(softplus(-beta * (chosen-rejected))).
// Disabled is canonical only when beta, loss_scale, and chosen_ce_scale are all zero.
typedef struct aoem_ai_sgm_paired_sequence_preference_config_v1 {
  uint32_t struct_size;
  uint32_t version;
  uint32_t enabled;
  uint32_t flags;
  float beta;
  float loss_scale;
  float chosen_ce_scale;
  uint32_t reserved0;
} aoem_ai_sgm_paired_sequence_preference_config_v1;

typedef struct aoem_ai_sgm_paired_sequence_preference_stats_v1 {
  uint32_t struct_size;
  uint32_t version;
  uint64_t optimizer_step;
  uint64_t pair_count;
  uint64_t reserved0;
  double mean_margin;
  double mean_loss;
  double mean_chosen_score;
  double mean_rejected_score;
} aoem_ai_sgm_paired_sequence_preference_stats_v1;

// Configure after Q4 vocabulary attachment and before the first optimizer step.
// The batch size must be even and is capped at 256 by the hierarchical trainer;
// one 256-lane reduction workgroup therefore covers all adjacent pairs.
// When enabled, beta and loss_scale must be positive, chosen_ce_scale must be
// finite and nonnegative, and flags and all reserved fields must be zero.
AOEM_API int32_t aoem_ai_sgm_hierarchical_train_set_paired_sequence_preference_v1(
  uint64_t session_id,
  const aoem_ai_sgm_paired_sequence_preference_config_v1* config
);

// Query callers initialize struct_size and version before calling.
AOEM_API int32_t aoem_ai_sgm_hierarchical_train_get_paired_sequence_preference_config_v1(
  uint64_t session_id,
  aoem_ai_sgm_paired_sequence_preference_config_v1* out_config
);

// Statistics are means over valid pairs from the most recent paired training sequence.
// Validation calls with update_weights=false do not replace the last training statistics.
// mean_loss is the configured weighted total objective, evaluated per pair before averaging.
AOEM_API int32_t aoem_ai_sgm_hierarchical_train_get_paired_sequence_preference_stats_v1(
  uint64_t session_id,
  aoem_ai_sgm_paired_sequence_preference_stats_v1* out_stats
);

// Score the configured adjacent chosen/rejected lane pairs without BPTT,
// optimizer updates, parameter changes, or sequence-carry changes. The
// paired training mode must have been enabled through the setter above. The
// response-only vocabulary mask may have different valid lengths per lane.
// Results are returned directly and do not replace the most recent paired
// training statistics queried above. Callers initialize out_stats struct_size
// and version before calling.
AOEM_API int32_t aoem_ai_sgm_hierarchical_evaluate_paired_sequence_preference_v1(
  uint64_t session_id,
  const float* input,
  size_t input_len,
  const float* target,
  size_t target_len,
  const uint32_t* target_token_ids,
  size_t target_token_ids_len,
  const uint32_t* state_supervision_mask,
  size_t state_supervision_mask_len,
  const uint32_t* vocabulary_supervision_mask,
  size_t vocabulary_supervision_mask_len,
  aoem_ai_sgm_paired_sequence_preference_stats_v1* out_stats
);

// Score adjacent chosen/rejected lane pairs with the supplied objective
// equation without enabling or changing the session's paired training mode.
// This evaluation-only entry is valid at any optimizer boundary, including
// after completed optimizer steps. It does not run BPTT, update parameters,
// change sequence carry, or replace paired training statistics. config must be
// canonically enabled; callers initialize out_stats struct_size and version.
AOEM_API int32_t aoem_ai_sgm_hierarchical_evaluate_paired_sequence_preference_with_config_v1(
  uint64_t session_id,
  const aoem_ai_sgm_paired_sequence_preference_config_v1* config,
  const float* input,
  size_t input_len,
  const float* target,
  size_t target_len,
  const uint32_t* target_token_ids,
  size_t target_token_ids_len,
  const uint32_t* state_supervision_mask,
  size_t state_supervision_mask_len,
  const uint32_t* vocabulary_supervision_mask,
  size_t vocabulary_supervision_mask_len,
  aoem_ai_sgm_paired_sequence_preference_stats_v1* out_stats
);

#define AOEM_AI_SGM_OBJECTIVE_ROLE_BATCH_VERSION_V1 1u
#define AOEM_AI_SGM_OBJECTIVE_ROLE_TRANSACTION_CONFIG_VERSION_V1 1u
#define AOEM_AI_SGM_OBJECTIVE_ROLE_TRANSACTION_RESULT_VERSION_V1 1u
#define AOEM_AI_SGM_GROUP_COMMON_DESCENT_CONFIG_VERSION_V1 1u
#define AOEM_AI_SGM_GROUP_COMMON_DESCENT_RESULT_VERSION_V1 1u
#define AOEM_AI_SGM_GROUP_COMMON_DESCENT_RESULT_VERSION_V2 2u
#define AOEM_AI_SGM_GROUP_COMMON_DESCENT_GROUP_RESULT_VERSION_V1 1u
#define AOEM_AI_SGM_GROUP_OBJECTIVE_VERSION_V2 2u
#define AOEM_AI_SGM_GROUP_COMMON_DESCENT_GROUP_RESULT_VERSION_V2 2u
#define AOEM_AI_SGM_GROUP_OBJECTIVE_STANDARD_STATE_SEQUENCE_V2 1u
#define AOEM_AI_SGM_GROUP_OBJECTIVE_PAIRED_SEQUENCE_PREFERENCE_V2 2u
#define AOEM_AI_SGM_EXACT_FORWARD_PROTECTION_CONFIG_VERSION_V1 1u
#define AOEM_AI_SGM_EXACT_FORWARD_PROTECTION_CANDIDATE_ACCEPTED_V2 (1ull << 0)
#define AOEM_AI_SGM_EXACT_FORWARD_PROTECTION_ROLLBACK_APPLIED_V2 (1ull << 1)
#define AOEM_AI_SGM_EXACT_FORWARD_PROTECTION_CANDIDATE_OPTIMIZER_BLOCKS_APPLIED_FOR_EXACT_REPLAY_V2 (1ull << 2)
#define AOEM_AI_SGM_EXACT_FORWARD_PROTECTION_TRANSACTION_COMMITTED_V2 (1ull << 3)
#define AOEM_AI_SGM_EXACT_FORWARD_PROTECTION_FLAGS_V2 \
  (AOEM_AI_SGM_EXACT_FORWARD_PROTECTION_CANDIDATE_ACCEPTED_V2 | \
   AOEM_AI_SGM_EXACT_FORWARD_PROTECTION_ROLLBACK_APPLIED_V2 | \
   AOEM_AI_SGM_EXACT_FORWARD_PROTECTION_CANDIDATE_OPTIMIZER_BLOCKS_APPLIED_FOR_EXACT_REPLAY_V2 | \
   AOEM_AI_SGM_EXACT_FORWARD_PROTECTION_TRANSACTION_COMMITTED_V2)
#define AOEM_AI_SGM_GROUP_COMMON_DESCENT_METRICS_VERSION_V1 1u
#define AOEM_AI_SGM_GROUP_COMMON_DESCENT_METRICS_VERSION_V2 2u
#define AOEM_AI_SGM_GROUP_COMMON_DESCENT_METRICS_VERSION_V3 3u
#define AOEM_AI_SGM_GROUP_COMMON_DESCENT_OPTIMIZER_ACTUAL_F32_PARAMETER_DISPLACEMENT_V3 (1ull << 0)
#define AOEM_AI_SGM_GROUP_COMMON_DESCENT_OPTIMIZER_GLOBAL_STEP_CONDITIONAL_COMMIT_V3 (1ull << 1)
#define AOEM_AI_SGM_GROUP_COMMON_DESCENT_OPTIMIZER_TWO_PHASE_GPU_TRANSACTION_V3 (1ull << 2)
#define AOEM_AI_SGM_GROUP_COMMON_DESCENT_OPTIMIZER_UPDATE_DISABLED_V3 (1ull << 3)
#define AOEM_AI_SGM_GROUP_COMMON_DESCENT_OPTIMIZER_FLAGS_V3 \
  (AOEM_AI_SGM_GROUP_COMMON_DESCENT_OPTIMIZER_ACTUAL_F32_PARAMETER_DISPLACEMENT_V3 | \
   AOEM_AI_SGM_GROUP_COMMON_DESCENT_OPTIMIZER_GLOBAL_STEP_CONDITIONAL_COMMIT_V3 | \
   AOEM_AI_SGM_GROUP_COMMON_DESCENT_OPTIMIZER_TWO_PHASE_GPU_TRANSACTION_V3)
#define AOEM_AI_SGM_GROUP_COMMON_DESCENT_OPTIMIZER_KNOWN_FLAGS_V3 \
  (AOEM_AI_SGM_GROUP_COMMON_DESCENT_OPTIMIZER_FLAGS_V3 | \
   AOEM_AI_SGM_GROUP_COMMON_DESCENT_OPTIMIZER_UPDATE_DISABLED_V3)
#define AOEM_AI_SGM_GROUP_COMMON_DESCENT_MAX_AUXILIARY_GROUPS_V1 8u

// One token-major objective-role microbatch. lane_count may be smaller than the
// sealed trainer batch size; AOEM pads the remaining physical lanes with zero
// input, target, masks, and continuation. Every pointer length is compact:
// state arrays are sequence_length * lane_count * hidden_size, token masks and
// ids are sequence_length * lane_count, and continuation is lane_count.
// flags must be zero. A lane contributes only through nonzero supervision masks.
typedef struct aoem_ai_sgm_objective_role_batch_v1 {
  uint32_t struct_size;
  uint32_t version;
  uint32_t lane_count;
  uint32_t flags;
  const float* input;
  size_t input_len;
  const float* target;
  size_t target_len;
  const uint32_t* target_token_ids;
  size_t target_token_ids_len;
  // Mandatory compact teacher-forcing/feedback selection mask. AOEM preserves
  // it exactly for ordinary truth-history forward/backward semantics.
  const uint32_t* feedback_mask;
  size_t feedback_mask_len;
  const uint32_t* state_supervision_mask;
  size_t state_supervision_mask_len;
  const uint32_t* vocabulary_supervision_mask;
  size_t vocabulary_supervision_mask_len;
  // Mandatory compact endpoint mask. Its loss uses the trainer's sealed
  // prompt-endpoint geometry scale; zero entries do not select an endpoint.
  const uint32_t* prompt_endpoint_mask;
  size_t prompt_endpoint_mask_len;
  const uint32_t* continuation_mask;
  size_t continuation_mask_len;
} aoem_ai_sgm_objective_role_batch_v1;

// Typed v2 auxiliary group descriptor. kind must be one of the
// AOEM_AI_SGM_GROUP_OBJECTIVE_*_V2 constants and flags must be zero.
typedef struct aoem_ai_sgm_group_objective_v2 {
  uint32_t struct_size;
  uint32_t version;
  uint32_t kind;
  uint32_t flags;
  aoem_ai_sgm_objective_role_batch_v1 batch;
} aoem_ai_sgm_group_objective_v2;

typedef struct aoem_ai_sgm_objective_role_transaction_config_v1 {
  uint32_t struct_size;
  uint32_t version;
  uint32_t flags;
  uint32_t reserved0;
  float maximum_auxiliary_norm_ratio;
  float paired_preference_beta;
  float paired_preference_loss_scale;
  float paired_preference_chosen_ce_scale;
  uint64_t reserved2;
} aoem_ai_sgm_objective_role_transaction_config_v1;

typedef struct aoem_ai_sgm_objective_role_transaction_result_v1 {
  uint32_t struct_size;
  uint32_t version;
  uint64_t optimizer_step;
  uint64_t primary_microbatch_count;
  // Sum of compact lane_count over all primary microbatches.
  uint64_t primary_lane_count;
  uint64_t primary_supervised_token_count;
  uint64_t primary_prompt_endpoint_count;
  uint64_t auxiliary_lane_count;
  uint64_t auxiliary_valid_pair_count;
  double primary_vocabulary_cross_entropy;
  double primary_target_token_top1_accuracy;
  double primary_top1_context_transition_rate;
  double primary_target_token_mean_rank;
  double primary_target_token_top8_accuracy;
  double primary_target_token_top32_accuracy;
  double primary_target_token_top128_accuracy;
  double primary_target_token_mean_margin;
  double auxiliary_mean_margin;
  double auxiliary_mean_loss;
  double auxiliary_mean_chosen_score;
  double auxiliary_mean_rejected_score;
} aoem_ai_sgm_objective_role_transaction_result_v1;

// Versioned atomic primary/auxiliary objective transaction.
//
// primary_batch_count must equal the session's sealed
// gradient_accumulation_steps. AOEM executes every primary descriptor exactly
// once, retaining its feedback, prompt-endpoint, continuation, and carry
// semantics without updating weights. Primary next-token CE and state/endpoint
// objectives are normalized over their corresponding supervised counts across
// the complete primary descriptor array.
//
// The single auxiliary descriptor uses the configured paired sequence
// preference objective. Adjacent lanes are chosen/rejected; a pair is valid
// only when both lanes contain a vocabulary-supervised token. The existing
// paired_preference_beta, paired_preference_loss_scale, and
// paired_preference_chosen_ce_scale are sealed by this transaction config, and
// the pair gradient is normalized by the valid-pair count. For a 32-lane trainer,
// six real pairs occupy lanes 0..11 once; lanes 12..31 use zero supervision
// masks. AOEM never repeats valid pairs to fill physical lanes. The auxiliary
// state_supervision_mask and prompt_endpoint_mask must be entirely zero: this
// descriptor is a pure paired-preference objective and cannot inject state,
// endpoint, or temporal losses into the auxiliary parameter gradient.
//
// AOEM writes primary and auxiliary parameter gradients separately, applies
// the existing 4096-element block projection and norm cap, and submits exactly
// one AdamW update. Primary and auxiliary carry state remain isolated. Parameter
// state, optimizer moments, carry state, preference stats, and the output
// result are committed only after the complete transaction succeeds. A failure
// before Vulkan submission leaves the session reusable. If Vulkan reports an
// indeterminate failure after submission, AOEM fail-stops the affected runtime:
// no later dispatch, readback, export, or retry is permitted through that
// runtime, and the caller must destroy the session and restore its last durable
// model checkpoint. Partially submitted state is therefore never observable as
// a valid transaction result.
// All flags and reserved fields must be zero. Callers initialize out_result's
// struct_size and version before calling.
AOEM_API int32_t aoem_ai_sgm_hierarchical_train_objective_role_transaction_v1(
  uint64_t session_id,
  const aoem_ai_sgm_objective_role_transaction_config_v1* config,
  const aoem_ai_sgm_objective_role_batch_v1* primary_batches,
  size_t primary_batch_count,
  const aoem_ai_sgm_objective_role_batch_v1* auxiliary_batch,
  aoem_ai_sgm_objective_role_transaction_result_v1* out_result
);

// Versioned atomic objective-role transaction with an optional auxiliary role.
//
// Primary descriptor, normalization, carry, optimizer, and failure semantics
// are identical to v1. auxiliary_batch_count must be 0 or 1. For 0, callers
// pass auxiliary_batches=NULL; AOEM performs no auxiliary BPTT or parameter
// gradient merge and reports zero auxiliary lanes, pairs, and statistics. For
// 1, the descriptor and paired-preference semantics are identical to v1.
// Every successful call submits exactly one AdamW update. The v1 config and
// result wire structures are deliberately reused; the v2 symbol makes optional
// auxiliary support fail-fast discoverable without changing v1 behavior.
AOEM_API int32_t aoem_ai_sgm_hierarchical_train_objective_role_transaction_v2(
  uint64_t session_id,
  const aoem_ai_sgm_objective_role_transaction_config_v1* config,
  const aoem_ai_sgm_objective_role_batch_v1* primary_batches,
  size_t primary_batch_count,
  const aoem_ai_sgm_objective_role_batch_v1* auxiliary_batches,
  size_t auxiliary_batch_count,
  aoem_ai_sgm_objective_role_transaction_result_v1* out_result
);

// Canonical training-group common-descent transaction.
//
// Each auxiliary descriptor is one independent training group and must contain
// only paired-preference supervision. Groups are evaluated from the same
// pre-update parameter state, keep separate full-BPTT parameter gradients, and
// never consume held-out material. AOEM combines the primary gradient and all
// nonzero group gradients with the fixed normalized minimum-norm convex-hull
// solver, and certifies every 4096-element block against every participating
// pre-optimizer gradient with a normalized cosine lower tolerance of -1e-5.
// It then submits exactly one optimizer update. Conflicting blocks shrink
// toward zero; uncertified or non-finite blocks are zeroed.
//
// This contract certifies the merged gradient before AdamW. It does not claim
// that optimizer moments, diagonal preconditioning, or weight decay preserve
// the same per-group first-order geometry. Product promotion remains a caller
// decision based on physically isolated held-out evaluation.
//
// auxiliary_group_count must be in 1..=8. Group descriptors are ordered by a
// caller-stable canonical group order; every continuation mask must be zero, so
// no group can inherit or commit another group's carry state. All descriptors,
// config fields, and output capacities are validated before GPU dispatch.
typedef struct aoem_ai_sgm_group_common_descent_config_v1 {
  uint32_t struct_size;
  uint32_t version;
  uint32_t flags;
  uint32_t reserved0;
  float paired_preference_beta;
  float paired_preference_loss_scale;
  float paired_preference_chosen_ce_scale;
  uint32_t reserved1;
} aoem_ai_sgm_group_common_descent_config_v1;

typedef struct aoem_ai_sgm_exact_forward_protection_config_v1 {
  uint32_t struct_size;
  uint32_t version;
  uint32_t flags;
  uint32_t reserved0;
  double maximum_state_rmse_relative_increase;
  double maximum_state_cosine_decrease;
} aoem_ai_sgm_exact_forward_protection_config_v1;

typedef struct aoem_ai_sgm_group_common_descent_result_v1 {
  uint32_t struct_size;
  uint32_t version;
  uint64_t optimizer_step;
  uint64_t primary_microbatch_count;
  uint64_t primary_lane_count;
  uint64_t primary_supervised_token_count;
  uint64_t primary_prompt_endpoint_count;
  uint64_t auxiliary_group_count;
  uint64_t auxiliary_lane_count;
  uint64_t auxiliary_valid_pair_count;
  double primary_vocabulary_cross_entropy;
  double primary_target_token_top1_accuracy;
  double primary_top1_context_transition_rate;
  double primary_target_token_mean_rank;
  double primary_target_token_top8_accuracy;
  double primary_target_token_top32_accuracy;
  double primary_target_token_top128_accuracy;
  double primary_target_token_mean_margin;
} aoem_ai_sgm_group_common_descent_result_v1;

// The complete aggregate field prefix after struct_size/version is identical
// to result_v1. optimizer_step is the candidate optimizer step and equals
// candidate_optimizer_step. protection_flags uses only the
// AOEM_AI_SGM_EXACT_FORWARD_PROTECTION_*_V2 bits.
typedef struct aoem_ai_sgm_group_common_descent_result_v2 {
  uint32_t struct_size;
  uint32_t version;
  uint64_t optimizer_step;
  uint64_t primary_microbatch_count;
  uint64_t primary_lane_count;
  uint64_t primary_supervised_token_count;
  uint64_t primary_prompt_endpoint_count;
  uint64_t auxiliary_group_count;
  uint64_t auxiliary_lane_count;
  uint64_t auxiliary_valid_pair_count;
  double primary_vocabulary_cross_entropy;
  double primary_target_token_top1_accuracy;
  double primary_top1_context_transition_rate;
  double primary_target_token_mean_rank;
  double primary_target_token_top8_accuracy;
  double primary_target_token_top32_accuracy;
  double primary_target_token_top128_accuracy;
  double primary_target_token_mean_margin;
  uint64_t optimizer_attempt_step;
  uint64_t committed_optimizer_step;
  uint64_t candidate_optimizer_step;
  uint64_t protection_flags;
  uint64_t standard_state_group_count;
  uint64_t state_supervised_token_count;
  double baseline_root_mean_square_error;
  double candidate_root_mean_square_error;
  double baseline_mean_cosine_similarity;
  double candidate_mean_cosine_similarity;
  double maximum_group_rmse_relative_increase;
  double maximum_group_cosine_decrease;
} aoem_ai_sgm_group_common_descent_result_v2;

typedef struct aoem_ai_sgm_group_common_descent_group_result_v1 {
  uint32_t struct_size;
  uint32_t version;
  uint32_t group_index;
  uint32_t flags;
  uint64_t lane_count;
  uint64_t valid_pair_count;
  double mean_margin;
  double mean_loss;
  double mean_chosen_score;
  double mean_rejected_score;
} aoem_ai_sgm_group_common_descent_group_result_v1;

typedef struct aoem_ai_sgm_group_common_descent_group_result_v2 {
  uint32_t struct_size;
  uint32_t version;
  uint32_t group_index;
  uint32_t kind;
  uint32_t flags;
  uint32_t reserved0;
  uint64_t lane_count;
  uint64_t state_supervised_token_count;
  uint64_t vocabulary_supervised_token_count;
  uint64_t prompt_endpoint_count;
  uint64_t valid_pair_count;
  double mean_margin;
  double mean_loss;
  double mean_chosen_score;
  double mean_rejected_score;
} aoem_ai_sgm_group_common_descent_group_result_v2;

typedef struct aoem_ai_sgm_group_common_descent_metrics_v1 {
  uint32_t struct_size;
  uint32_t version;
  uint64_t optimizer_step;
  uint32_t stage;
  uint32_t family;
  uint64_t block_count;
  uint64_t nonzero_block_count;
  uint64_t certified_block_count;
  uint64_t stalled_block_count;
  uint64_t nonfinite_block_count;
  double primary_gradient_l2_sum;
  double group_gradient_l2_sum;
  double final_gradient_l2_sum;
  double minimum_final_group_cosine;
  double mean_final_group_cosine;
  double mean_frank_wolfe_gap;
  double mean_solver_iterations;
} aoem_ai_sgm_group_common_descent_metrics_v1;

typedef struct aoem_ai_sgm_group_common_descent_metrics_v2 {
  uint32_t struct_size;
  uint32_t version;
  uint64_t optimizer_step;
  uint32_t stage;
  uint32_t family;
  uint64_t block_count;
  uint64_t nonzero_block_count;
  uint64_t certified_block_count;
  uint64_t stalled_block_count;
  uint64_t nonfinite_block_count;
  double primary_gradient_l2_sum;
  double group_gradient_l2_sum;
  double final_gradient_l2_sum;
  double minimum_final_group_cosine;
  double mean_final_group_cosine;
  double mean_frank_wolfe_gap;
  double mean_solver_iterations;
  uint32_t auxiliary_group_count;
  uint32_t reserved0;
  double group_gradient_l2_sums[AOEM_AI_SGM_GROUP_COMMON_DESCENT_MAX_AUXILIARY_GROUPS_V1];
} aoem_ai_sgm_group_common_descent_metrics_v2;

typedef struct aoem_ai_sgm_group_common_descent_metrics_v3 {
  uint32_t struct_size;
  uint32_t version;
  uint64_t optimizer_step;
  uint32_t stage;
  uint32_t family;
  uint64_t block_count;
  uint64_t nonzero_block_count;
  uint64_t certified_block_count;
  uint64_t stalled_block_count;
  uint64_t nonfinite_block_count;
  double primary_gradient_l2_sum;
  double group_gradient_l2_sum;
  double final_gradient_l2_sum;
  double minimum_final_group_cosine;
  double mean_final_group_cosine;
  double mean_frank_wolfe_gap;
  double mean_solver_iterations;
  uint32_t auxiliary_group_count;
  uint32_t reserved0;
  double group_gradient_l2_sums[AOEM_AI_SGM_GROUP_COMMON_DESCENT_MAX_AUXILIARY_GROUPS_V1];
  uint64_t optimizer_certified_block_count;
  uint64_t optimizer_stalled_block_count;
  uint64_t optimizer_nonfinite_block_count;
  uint64_t optimizer_applied_block_count;
  double optimizer_delta_l2_sum;
  double minimum_optimizer_group_cosine;
  double mean_optimizer_group_cosine;
  uint64_t optimizer_certificate_flags;
} aoem_ai_sgm_group_common_descent_metrics_v3;

AOEM_API int32_t aoem_ai_sgm_hierarchical_train_group_common_descent_transaction_v1(
  uint64_t session_id,
  const aoem_ai_sgm_group_common_descent_config_v1* config,
  const aoem_ai_sgm_objective_role_batch_v1* primary_batches,
  size_t primary_batch_count,
  const aoem_ai_sgm_objective_role_batch_v1* auxiliary_groups,
  size_t auxiliary_group_count,
  aoem_ai_sgm_group_common_descent_result_v1* out_result,
  aoem_ai_sgm_group_common_descent_group_result_v1* out_group_results,
  size_t group_result_capacity
);

// Typed group common-descent transaction. Aggregate config/result semantics,
// group limit, normalized 4096-element common-descent merge, pre-AdamW
// certificate, failure semantics, and single AdamW commit are identical to v1.
//
// STANDARD_STATE_SEQUENCE groups require at least one state-supervised token
// and require vocabulary, feedback, prompt-endpoint, and continuation masks to
// be entirely zero. They run ordinary state-residual BPTT into their own group
// gradient set and report exact state/vocabulary/endpoint supervision counts;
// valid_pair_count and every preference statistic are exactly zero.
//
// PAIRED_SEQUENCE_PREFERENCE groups preserve the v1 adjacent-lane preference
// contract and statistics. Both kinds may appear in any caller-stable order.
AOEM_API int32_t aoem_ai_sgm_hierarchical_train_group_common_descent_transaction_v2(
  uint64_t session_id,
  const aoem_ai_sgm_group_common_descent_config_v1* config,
  const aoem_ai_sgm_objective_role_batch_v1* primary_batches,
  size_t primary_batch_count,
  const aoem_ai_sgm_group_objective_v2* auxiliary_groups,
  size_t auxiliary_group_count,
  aoem_ai_sgm_group_common_descent_result_v1* out_result,
  aoem_ai_sgm_group_common_descent_group_result_v2* out_group_results,
  size_t group_result_capacity
);

// v3 preserves the typed objective contract from v2 and implements a two-phase
// GPU transaction. Certification writes only the merged primary gradient and
// metrics. Each 4096-parameter block is certified against the actual finite-
// precision FP32 parameter displacement old - fl(old - nominal_delta), not the
// nominal AdamW delta. Only after every family certificate and every commit
// operation are prepared does one ordered Vulkan submission conditionally
// commit all certified parameter, first-moment, second-moment, and carry writes.
//
// optimizer_step is the global AdamW bias-correction step scheduled for this
// transaction. It advances after a successful final submission even when an
// individual block stalls or is rejected. Such a block leaves its parameters,
// first moment, and second moment bitwise unchanged. The resulting path is
// globally scheduled, conditionally committed AdamW parameter displacement; it
// is not claimed to be trajectory-equivalent to ordinary AdamW across stalls.
// A final submission failure terminates the training session.
AOEM_API int32_t aoem_ai_sgm_hierarchical_train_group_common_descent_transaction_v3(
  uint64_t session_id,
  const aoem_ai_sgm_group_common_descent_config_v1* config,
  const aoem_ai_sgm_objective_role_batch_v1* primary_batches,
  size_t primary_batch_count,
  const aoem_ai_sgm_group_objective_v2* auxiliary_groups,
  size_t auxiliary_group_count,
  aoem_ai_sgm_group_common_descent_result_v1* out_result,
  aoem_ai_sgm_group_common_descent_group_result_v2* out_group_results,
  size_t group_result_capacity
);

// v4 preserves the typed group_objective_v2/group_result_v2 contract and the
// optimizer-displacement certificate from v3, and adds exact-forward
// protection over every STANDARD_STATE_SEQUENCE group. At least one such group
// is mandatory. Both protection thresholds must be finite and non-negative;
// config flags and reserved fields must be zero.
//
// AOEM snapshots optimizer state, physically applies the certified candidate
// optimizer blocks, and evaluates every STANDARD_STATE_SEQUENCE group through
// the same finite-precision state-forward graph used for its baseline. This
// gate uses the typed group's required zero feedback, vocabulary, prompt,
// continuation, and initial carried-state contract. It does not cover a full
// autoregressive product rollout, vocabulary ranking, sampling, or EOS.
// Every successful v4 call sets
// CANDIDATE_OPTIMIZER_BLOCKS_APPLIED_FOR_EXACT_REPLAY in protection_flags. This
// bit means that optimizer_applied_block_count from a following metrics_v3
// query describes blocks physically applied to the replay candidate. It does
// not mean those blocks remain in the final model.
//
// An accepted candidate additionally sets CANDIDATE_ACCEPTED and
// TRANSACTION_COMMITTED, leaves ROLLBACK_APPLIED clear, and reports
// committed_optimizer_step == candidate_optimizer_step. A rejected candidate
// sets ROLLBACK_APPLIED, leaves CANDIDATE_ACCEPTED and TRANSACTION_COMMITTED
// clear, restores the optimizer state, and reports committed_optimizer_step <
// candidate_optimizer_step. Rejection is a successful, session-preserving
// transaction outcome. A runtime execution error invalidates the session.
AOEM_API int32_t aoem_ai_sgm_hierarchical_train_group_common_descent_transaction_v4(
  uint64_t session_id,
  const aoem_ai_sgm_group_common_descent_config_v1* config,
  const aoem_ai_sgm_exact_forward_protection_config_v1* protection_config,
  const aoem_ai_sgm_objective_role_batch_v1* primary_batches,
  size_t primary_batch_count,
  const aoem_ai_sgm_group_objective_v2* auxiliary_groups,
  size_t auxiliary_group_count,
  aoem_ai_sgm_group_common_descent_result_v2* out_result,
  aoem_ai_sgm_group_common_descent_group_result_v2* out_group_results,
  size_t group_result_capacity
);

// Query the per-stage/per-parameter-family metrics from the last successful
// group common-descent transaction. Pass metrics=NULL and metrics_capacity=0
// to query the required count. out_metric_count is mandatory.
AOEM_API int32_t aoem_ai_sgm_hierarchical_train_get_group_common_descent_metrics_v1(
  uint64_t session_id,
  aoem_ai_sgm_group_common_descent_metrics_v1* metrics,
  size_t metrics_capacity,
  size_t* out_metric_count
);

// v2 preserves every v1 metric and adds the exact internal per-group gradient
// L2 sums. auxiliary_group_count is in 1..=8 for every returned row. Entries
// below that count are finite and non-negative; every unused tail entry is
// positive zero. Query and capacity semantics are identical to v1.
AOEM_API int32_t aoem_ai_sgm_hierarchical_train_get_group_common_descent_metrics_v2(
  uint64_t session_id,
  aoem_ai_sgm_group_common_descent_metrics_v2* metrics,
  size_t metrics_capacity,
  size_t* out_metric_count
);

// v3 is valid after a successful optimizer-aware v3 or exact-forward protected
// v4 transaction. Its complete v2 prefix retains the exact v1/v2 gradient-
// metric semantics. The appended optimizer fields form an independent
// partition. For an update-enabled family, certified + stalled + nonfinite
// equals the v2 certified block count, and applied equals optimizer certified
// after the optimizer submission. optimizer_certificate_flags equals
// AOEM_AI_SGM_GROUP_COMMON_DESCENT_OPTIMIZER_FLAGS_V3.
//
// For an update-disabled family, optimizer_certificate_flags additionally has
// AOEM_AI_SGM_GROUP_COMMON_DESCENT_OPTIMIZER_UPDATE_DISABLED_V3 set. All four
// optimizer block counts, optimizer_delta_l2_sum, and both optimizer cosine
// fields are exactly positive zero. Such a family is neither applied nor
// stalled: every parameter and first/second-moment buffer is physically
// unchanged, including decoupled weight decay. Its v2 certified_block_count
// still describes the pre-optimizer gradient certificate, but the optimizer
// partition is intentionally empty; callers must not require optimizer
// certified + stalled + nonfinite to equal that gradient count. Flags never
// contain bits outside AOEM_AI_SGM_GROUP_COMMON_DESCENT_OPTIMIZER_KNOWN_FLAGS_V3.
//
// After v4, optimizer_applied_block_count is evidence about blocks physically
// applied to the exact-replay candidate. Callers must inspect result_v2
// protection_flags before interpreting it as final model state. It represents
// a committed model update only when TRANSACTION_COMMITTED is set. When
// ROLLBACK_APPLIED is set, the same count is retained as candidate evidence,
// but every candidate optimizer-state write has been restored.
AOEM_API int32_t aoem_ai_sgm_hierarchical_train_get_group_common_descent_metrics_v3(
  uint64_t session_id,
  aoem_ai_sgm_group_common_descent_metrics_v3* metrics,
  size_t metrics_capacity,
  size_t* out_metric_count
);
// Feature ABI: generic primitive execution (domain-agnostic; for AI/crypto/etc workloads).
// primitive_kind values:
//   0=sort, 1=scan, 2=scatter, 3=fft, 4=merkle, 5=ntt, 6=gemm
// backend_request values:
//   0=auto, 1=spirv-vulkan, 2=cuda
// output wire format (little-endian):
//   magic "AOPR\0" + version:u16 + flags:u16 +
//   primitive:u32 + backend_kind:u32 + stage_count:u32 +
//   values_len:u32 + indices_len:u32 + output_hash:u64 +
//   values[values_len]:u32 + indices[indices_len]:u32
// return code:
//   0 = success
//  -1 = invalid handle
//  -2 = invalid argument
//  -4 = execution/policy error
AOEM_API int32_t aoem_execute_primitive_v1(
  void* handle,
  uint32_t primitive_kind,
  uint32_t backend_request,
  uint32_t vendor_id,
  const uint32_t* values_ptr,
  uint32_t values_len,
  const uint32_t* indices_ptr,
  uint32_t indices_len,
  aoem_primitive_result_v1* out_result,
  uint8_t** output_ptr,
  size_t* output_len
);
// Batch fast discard mode:
// - pass output_ptr=NULL and output_len=NULL
// Production ABI: memory release and per-handle error inspection.
// Any AOEM-owned output buffer returned by this header must be released with aoem_free.
AOEM_API void aoem_free(uint8_t* ptr, size_t len);
AOEM_API const char* aoem_last_error(void* handle);

// Checked integer SSA execution through compute.ai.sgm_infer_v1 (no new C entry).
// Payload: 8 bytes "AOIP0\0\0\0", LE u32 version=1, rows (1..4096),
// program_bytes (<=1024), reserved=0; APFLINT1/2/3 program; row-major LE i64 inputs.
// APFLINT1: magic[8], u32 numeric=1,input_count,step_count,output_count.
// APFLINT2/3: same header plus u32 constant_count. Then input permutation[u32],
// constants[i64] (v2/v3), steps[opcode,left_register,right_register,failure],
// output_registers[u32]. Counts: inputs<=16, constants<=32, steps<=32, outputs1..16.
// Registers are inputs, constants, then prior SSA results. Opcodes: add0, subtract1,
// multiply2, exact_divide3, positive_modulus_remainder4, gcd5, inverse_mod6,
// floor_divide7 (v2+), equal8, less9, less_equal10, require_equal11 (v3).
// Comparisons return 0 or 1; require_equal returns left iff equal, else status7.
// Failure 0=abort, 1=no-solution only on nonexact/not-invertible.
// V2 without constants or floor_divide is noncanonical and rejected.
// V3 without opcodes8..11 is noncanonical and rejected; older versions reject them.
// Result key: <prefix>/ai/sgm_infer/integer_program/result. JSON rows contain
// status (0=ok,1=no-solution,2=overflow,3=zero divisor,4=nonexact division,
// 5=invalid modulus,6=not invertible,7=condition not met), failed_instruction (null on success),
// values (i64 array; empty on every failed row). No CPU numeric fallback.
// AOIP0 version=2 seals checked_i128_v1: APFLINT4 only, program_bytes<=2048,
// inputs are row-major little-endian signed 128-bit integers (16 bytes each).
// APFLINT4 uses the V3 layout, numeric=2, but constants occupy 16 bytes each.
// Counts, opcodes and SSA rules are unchanged. It is not accepted by
// version=1 or the i64 search ABI. Result version=2, numeric_contract=checked_i128_v1;
// values are canonical decimal strings, preserving all 128 bits through JSON.
// APFLINT4 additionally allows failure=2: claim-no-unique-solution on nonexact/
// not-invertible, returned as status8 with no values. This differs from no-solution
// (status1): a singular system may have infinitely many solutions. Both are
// program-selected claims requiring goal verification, not kernel proof claims.
// AOIP0 version=3 executes a bounded ordered call graph (APFLCG01), not search.
// program_bytes<=65536; the request rows and i128 input encoding stay unchanged.
// Graph: magic[8], LE u32 numeric=2,input_count,bank_count,call_count,output_count;
// then each Bank as u32 byte_length + APFLINT4 bytes; then each call as
// u32 bank_index,arity,argument_slots[arity]; then final output_slots[u32].
// Limits: inputs<=16, Banks1..16, calls1..16, final outputs1..16, at most64
// published call results and512 cumulative primitive instructions. Bank limits
// retain APFLINT4 semantics. Slot order is external inputs, then each call's
// result tuple. Arguments reference only inputs or prior results; no recursion.
// Every call executes, even if its result is unused. First failure aborts the
// row, clears all outputs and reports the cumulative primitive instruction index.
// Result version=3; checked_i128_v1 decimal values/statuses otherwise unchanged.
// Intermediate values and ordered calls execute in one resident GPU dispatch.
// AOIP0 version=4 executes APFLOU01 typed result programs; program_bytes<=4MiB.
// Request row bounds, reserved field and row-major signed i128 encoding are unchanged.
// Asset: magic[8]="APFLOU01", LE u32 version=1,input_count,output_count,
// domain_count,refutation_count,postcondition_count; then length-prefixed APFLCG01
// graphs in that order: domain graphs, refutation graphs, one computation graph,
// postcondition graphs. All graph lengths are LE u32. No trailing bytes permitted.
// Limits: inputs+outputs<=16, outputs>=1, domains/refutations0..16, postconditions1..16.
// Domains/refutations take original inputs and return one Boolean. Computation
// takes original inputs and returns output_count values. Postconditions take
// original inputs followed by computed values, and return one Boolean.
// Result version=4, numeric_contract=checked_i128_v1,
// outcome_contract=goal_bound_result_v1. Rows contain outcome, fault, component,
// failed_instruction and values (canonical i128 decimal strings, empty unless value).
// outcome: 0=value, 1=outside-domain, 2=refuted, 3=execution/check failure.
// Domain checks precede refutations. A successful refutation equal to1 stops
// before computation; false or faulted refutations are unestablished, not vetoes.
// Computation and all original postconditions must succeed before returning value.
// Local Bank failure claims1/2 are not promoted: primitive faults2..7 stay faults.
// AOIP0 version=5 executes only APFLOU01 version=2, with one extra LE u32
// numeric_bits=1024 after postcondition_count. Embedded graph constants remain
// signed i128 and are sign-extended; all registers, operations and results use
// checked signed i1024. Inputs are 32 LE u32 two's-complement limbs per value.
// JSON result version=5, numeric_contract=checked_i1024_le_limbs_v1; each value
// is exactly 32 u32 limbs, least significant first. All other outcome fields,
// checks and fault semantics match version4. Precision is sealed by the asset,
// not selected by a runtime flag. Capacity overflow is failure, never UNSAT.
// component is a zero-based graph index, null only on value; failed_instruction
// is the cumulative instruction index within that graph on a primitive fault,
// otherwise null. fault=0 for value/domain rejection/refutation/false postcondition.
// This kernel executes sealed predicates, not their logical derivations. Trusted
// model compilation must bind refutations to the original goal; arbitrary supplied
// predicates are not independently certified mathematical proofs by this ABI.

// Bounded integer-program search through the same compute.ai.sgm_infer_v1 entry.
// Payload: "AOIS0\0\0\0", LE u32 version=1, queries1..64, asset_bytes<=65536,
// reserved=0; APFLSR01/02/03/04 asset; query/row/parameter-major LE i64 inputs.
// Asset: "APFLSR01" through "APFLSR04", LE u32 numeric=1, parameters1..8, observations1..16,
// capacity16..2048, evaluations1..262144, odd max_cost1..31, opcode_count1..8,
// constant_count0..16, seed_count0..32, checker_bytes<=1024.
// V3 adds LE u32 function_count1..8 after checker_bytes; V1/V2 have no such field.
// Then unique ordered opcodes[u32,0..7], unique constants[i64], seeds each as
// u32 byte_length + APFLINT1/2/3 bytes, V3 functions in the same length+bytes
// form, followed by the checker bytes. Each function is <=1024 bytes, 1..2
// inputs, one output, at least one arithmetic instruction, and abort-only.
// Seeds have parameters inputs, one output, arithmetic opcodes0..7 and abort
// failures only. Checker has parameters+1 inputs (last is proposed result),
// one output, abort failures only and at least one require_equal instruction.
// Search orders cost, opcode, left, right; deduplicates exact observation vectors.
// Seeds initialize the same arena and may subsequently be composed, not routed.
// Result key: <prefix>/ai/sgm_infer/integer_search/result, version1,
// numeric_contract=checked_i64_v1, backend=vulkan_spirv, universal_proof=false.
// Rows: status0=found_on_observations,1=cost_exhausted,2=evaluation_budget,
// 3=node_capacity; selected(nullable index),node_count,evaluations,nodes,values.
// A node is [kind,left/index,right,cost]; kinds0=parameter,1=constant,2=seed,
// 16+opcode=binary composition of prior nodes. APFLSR01 expanded cost: leaves1,
// binary1+left+right; seed costs saturate at max_cost+1. Seed admission is not
// cost-filtered. Failed searches return no nodes, values or selected index.
// APFLSR02 uses reusable-value planning cost: parameter/constant/seed=1,
// binary=1+left+right; independently rejects every seed/composition whose fully
// expanded expression exceeds 31 nodes. The numeric contract is unchanged.
// APFLSR03 also composes sealed functions on prior values. Node kind32+i names
// function i; left/right are its actual argument nodes (right=0 for unary).
// Function planning cost is 1+argument costs; expansion counts every repeated
// argument occurrence, constant and operator. Each function proposal charges
// its full instruction count against evaluations, even if invalid or oversized.
// Cost levels include even values; ordered primitives precede ordered functions
// at each level. Invalid calls are discarded without exposing partial values.
// APFLSR04 retains V3 composition (function_count may be zero), then adds
// LE u32 observation_capacity=observations..16, verification_count1..4096,
// verification_budget1..65536 directly after function_count. Remaining asset
// layout is identical. Each query supplies observations+verification_count rows,
// initial observations first. Values still cover only the initial observations.
// After finding a witness, the same dispatch checks its selected DAG and original
// condition on verification rows in order. The first failure, including arithmetic
// faults, is added to resident observations; search restarts with cumulative work.
// V4 rows add refinement:{restarts,verification_checks,observations_used}; status4
// means observation capacity, status5 verification budget. Each check charges one
// input evaluation of the bounded DAG/checker; it is not a primitive instruction
// counter. Status0 requires every verification input to pass on the final witness.
// Initial and verification inputs must belong to the intended goal domain. An
// invalid input cannot produce a success; no input is silently skipped or repaired.
// Cost semantics are sealed by asset magic, never an environment/runtime switch.
// Success is a finite witness only. Exhaustion is NOT mathematical unsat.
// No host search scheduler, target output array or CPU arithmetic fallback.

// Conditional ordered-call search: the same AOIS magic and unified entry,
// version=3, queries1..4, asset_bytes<=8388608, reserved=0, APFLCP01 asset,
// followed by query/row/parameter-major LE signed i128 inputs. Initial
// observations precede verification rows. No target answer array is supplied.
// APFLCP01 asset version 3 seals a deferred-sibling frontier; version 2 is rejected.
// It seals sparse F32 policy coefficients/feature indices, APFLINT4 Banks
// and APFLCG01 original-goal checkers. Policy scores, conditional normalization,
// bounded priority search, ordered call execution and refinement are resident.
// See docs/AOEM-CONDITIONAL-CALL-SEARCH-V1.md for the exact asset layout.
// Result version=3, numeric_contract=checked_i128_v1,
// policy_contract=sparse_conditional_f32_v1, universal_proof=false.
// Row fields: status,expanded,component_executions,peak_queue,
// verification_checks,refinements,calls:[{program,arguments}],outputs.
// References address all original inputs and every prior call output.
// Status0=finite witness,1=exhausted,2=expansion budget,3=queue capacity,
// 4=observation capacity,5=verification budget,6=node arena capacity.
//
// Verified search AOIS version=6 accepts only APFLVA01 version=3: the v2
// header plus u32 numeric_bits=1024 before its embedded policy. Instructions
// and signed-i128 constants are unchanged, but all operands/intermediates are
// checked signed 1024-bit values, 32 little-endian u32 limbs per input.
// GPU admission stably compacts in-domain rows not refuted by sealed proofs.
// It requires more admitted rows than initial observations; no rejected row
// is replaced or duplicated. Search uses the actual remaining row count.
// Result numeric_contract=checked_i1024_le_limbs_v1,
// admission_contract=stable_compacted_multilimb_inputs_v3; admission includes
// admitted_rows and generator_rejections. Status7 means insufficient admitted
// rows, status8 admission arithmetic fault. Neither means mathematical unsat.
// AOIS version5 and APFLVA01 versions1/2 retain their signed-i128 semantics.
// Nonzero status exposes no program. Finite success is not a universal proof;
// exhaustion is not unsatisfiability. This entry imports policy weights; it
// neither trains them nor admits natural-language requests automatically.

// Sparse conditional policy training: same AOIS magic and unified entry,
// version=4, queries=1, asset_bytes<=16777216, reserved=0, APFLST01 asset only.
// The asset seals initial F32 weights, sparse candidate features, categorical
// targets, explicit update order, learning rate and per-example L2 norm bound.
// Cross-entropy, gradient clipping and sequential SGD execute resident on Vulkan.
// Result at the existing integer_search/result state key has version=4 and kind
// compute.ai.sgm_infer_v1.sparse_policy_train.result; returns finite F32 weights,
// optimizer_updates, before/after_cross_entropy and before/after_top1.
// Zero updates is read-only scoring. Nonfinite/out-of-bound updates fail the
// whole request without exporting weights. No inference contract is changed.
// See docs/AOEM-SPARSE-CATEGORICAL-TRAIN-V1.md for exact layout and numeric rules.

#ifdef __cplusplus
}
#endif

#endif
