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

typedef struct aoem_primitive_result_v1 {
  uint32_t primitive;    // echo input primitive kind
  uint32_t backend_kind; // 1=spirv-vulkan, 2=cuda
  uint32_t stage_count;
  uint32_t values_len;
  uint32_t indices_len;
  uint64_t output_hash;
} aoem_primitive_result_v1;

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
// Generic zkVM prove+verify entry.
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
// Classic crypto/hash ABI (host-oriented, binary-safe).
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
// Ed25519 verify.
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
// Batch Ed25519 verify.
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
// secp256k1 verify/recover.
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
// Ring-signature verification (Web30-compatible payload).
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
// Ring-signature keygen.
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
// Ring-signature sign (Web30-compatible output payload).
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
// Ring-signature batch verify (Web30-compatible payload).
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
// Groth16 fixed-circuit prove (FFI baseline contract).
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
// Groth16 batch prove (FFI high-throughput contract).
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
// Groth16 single-proof verify (verify-only FFI).
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
// Groth16 batch verify (verify-only FFI, binary wire).
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
// Bulletproof range prove (FFI baseline contract).
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
// Bulletproof range proof verify (verify-only FFI).
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
// Bulletproof batch prove.
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
// Bulletproof batch verify.
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
// RingCT transaction prove/generate (FFI baseline contract).
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
// RingCT batch prove/generate.
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
// RingCT transaction verify (verify-only FFI).
// tx_encoding values:
//   1 = JSON payload (serde schema of PrivacyTransaction)
// return code:
//  0 = call succeeded (out_valid is 0/1)
// -2 = invalid argument/unsupported encoding
// -4 = decode/verify error
// -5 = capability not built (privacy-verify feature disabled)
AOEM_API int32_t aoem_ringct_verify_v1(
  const uint8_t* tx_payload_ptr,
  size_t tx_payload_len,
  uint32_t tx_encoding,
  uint32_t* out_valid
);
// RingCT batch verify.
// Input: JSON array of PrivacyTransaction payloads.
// Output:
// - out_results: byte bitmap (1=valid, 0=invalid)
// - out_valid_count: count(valid)
AOEM_API int32_t aoem_ringct_verify_batch_v1(
  const uint8_t* batch_json_ptr,
  size_t batch_json_len,
  uint8_t** out_results_ptr,
  size_t* out_results_len,
  uint32_t* out_valid_count
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
// Generic primitive execution (domain-agnostic; for AI/crypto/etc workloads).
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
AOEM_API void aoem_free(uint8_t* ptr, size_t len);
AOEM_API const char* aoem_last_error(void* handle);

#ifdef __cplusplus
}
#endif

#endif
