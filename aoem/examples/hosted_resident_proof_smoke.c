// Minimal C host smoke for the fixed-profile resident proof workload.
//
// Product path:
//   host -> aoem_execute_ops_wire_v1 -> compute.zk.resident_proof_v1
//        -> AOEM state -> aoem_state_read_v1

#include "aoem.h"

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#if defined(__GNUC__) || defined(__clang__)
#define AOEM_MAYBE_UNUSED __attribute__((unused))
#else
#define AOEM_MAYBE_UNUSED
#endif

#define AOEM_FIXED_PROFILE_RESIDENT_ASSET_V1_ID 0xA0E0A55Eu
#define AOEM_FIXED_PROFILE_RESIDENT_PROOF_V1_ID 1u
#define AOEM_MERKLE_MEMBERSHIP_PROOF_V1_ID 2u
#define AOEM_ZK_MERKLE_MEMBERSHIP_PROOF_V1_ID 3u
#define AOEM_MERKLE_MEMBERSHIP_PUBLIC_INPUT_LEN (32u + 32u + 8u + 4u)
#define AOEM_MERKLE_MEMBERSHIP_MAX_DEPTH 32u
#define AOEM_ZK_MERKLE_STYLE_V1_HASH_PROFILE 1u

#ifdef _WIN32
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
typedef HMODULE aoem_dynlib_t;
#else
#include <dlfcn.h>
typedef void* aoem_dynlib_t;
#endif

typedef uint32_t (*aoem_abi_version_fn)(void);
typedef int32_t (*aoem_global_init_fn)(void);
typedef void* (*aoem_create_fn)(void);
typedef void (*aoem_destroy_fn)(void*);
typedef int32_t (*aoem_execute_ops_wire_v1_fn)(
    void*,
    const uint8_t*,
    size_t,
    aoem_exec_v2_result*);
typedef int32_t (*aoem_state_read_v1_fn)(
    const uint8_t*,
    size_t,
    uint8_t**,
    size_t*);
typedef void (*aoem_free_fn)(uint8_t*, size_t);

typedef struct aoem_host_api {
  aoem_dynlib_t lib;
  aoem_abi_version_fn abi_version;
  aoem_global_init_fn global_init;
  aoem_create_fn create;
  aoem_destroy_fn destroy;
  aoem_execute_ops_wire_v1_fn execute_ops_wire_v1;
  aoem_state_read_v1_fn state_read_v1;
  aoem_free_fn free_buf;
} aoem_host_api;

typedef struct byte_buf {
  uint8_t* data;
  size_t len;
  size_t cap;
} byte_buf;

static void buf_free(byte_buf* b) {
  free(b->data);
  b->data = NULL;
  b->len = 0;
  b->cap = 0;
}

static int buf_reserve(byte_buf* b, size_t extra) {
  if (extra > SIZE_MAX - b->len) {
    return -1;
  }
  size_t need = b->len + extra;
  if (need <= b->cap) {
    return 0;
  }
  size_t cap = b->cap ? b->cap : 256;
  while (cap < need) {
    if (cap > SIZE_MAX / 2) {
      cap = need;
      break;
    }
    cap *= 2;
  }
  uint8_t* next = (uint8_t*)realloc(b->data, cap);
  if (!next) {
    return -1;
  }
  b->data = next;
  b->cap = cap;
  return 0;
}

static int buf_append(byte_buf* b, const void* data, size_t len) {
  if (buf_reserve(b, len) != 0) {
    return -1;
  }
  memcpy(b->data + b->len, data, len);
  b->len += len;
  return 0;
}

static int buf_u8(byte_buf* b, uint8_t v) {
  return buf_append(b, &v, 1);
}

static int buf_u16(byte_buf* b, uint16_t v) {
  uint8_t out[2] = {(uint8_t)(v & 0xff), (uint8_t)((v >> 8) & 0xff)};
  return buf_append(b, out, sizeof(out));
}

static int buf_u32(byte_buf* b, uint32_t v) {
  uint8_t out[4] = {
      (uint8_t)(v & 0xff),
      (uint8_t)((v >> 8) & 0xff),
      (uint8_t)((v >> 16) & 0xff),
      (uint8_t)((v >> 24) & 0xff)};
  return buf_append(b, out, sizeof(out));
}

static int buf_u64(byte_buf* b, uint64_t v) {
  for (int i = 0; i < 8; ++i) {
    if (buf_u8(b, (uint8_t)((v >> (i * 8)) & 0xff)) != 0) {
      return -1;
    }
  }
  return 0;
}

static int buf_i64(byte_buf* b, int64_t v) {
  return buf_u64(b, (uint64_t)v);
}

static void aoem_merkle_contract_mix_byte(uint32_t lanes[8], uint32_t* index, uint8_t byte) {
  uint32_t lane = *index & 7u;
  uint32_t rotate = 5u + ((lane + *index) & 15u);
  uint32_t value = lanes[lane] ^ (uint32_t)byte;
  value *= 0x01000193u;
  value = (value << rotate) | (value >> (32u - rotate));
  value += 0x9e3779b9u ^ ((*index) * 0x85ebca6bu);
  lanes[lane] = value;
  *index += 1u;
}

static void aoem_merkle_contract_mix_bytes(
    uint32_t lanes[8],
    uint32_t* index,
    const uint8_t* data,
    size_t len) {
  uint64_t n = (uint64_t)len;
  for (uint32_t i = 0; i < 8u; ++i) {
    aoem_merkle_contract_mix_byte(lanes, index, (uint8_t)((n >> (i * 8u)) & 0xffu));
  }
  for (size_t i = 0; i < len; ++i) {
    aoem_merkle_contract_mix_byte(lanes, index, data[i]);
  }
}

static void aoem_merkle_contract_digest32(
    const uint8_t* label,
    size_t label_len,
    const uint8_t* const* parts,
    const size_t* part_lens,
    size_t part_count,
    uint8_t out[32]) {
  uint32_t lanes[8] = {
      0x243f6a88u,
      0x85a308d3u,
      0x13198a2eu,
      0x03707344u,
      0xa4093822u,
      0x299f31d0u,
      0x082efa98u,
      0xec4e6c89u};
  uint32_t index = 0;
  static const uint8_t domain[] = "AOEM:resident_proof_contract:v1";
  aoem_merkle_contract_mix_bytes(lanes, &index, domain, sizeof(domain) - 1u);
  aoem_merkle_contract_mix_bytes(lanes, &index, label, label_len);
  uint8_t count_le[8];
  uint64_t count = (uint64_t)part_count;
  for (uint32_t i = 0; i < 8u; ++i) {
    count_le[i] = (uint8_t)((count >> (i * 8u)) & 0xffu);
  }
  aoem_merkle_contract_mix_bytes(lanes, &index, count_le, sizeof(count_le));
  for (size_t i = 0; i < part_count; ++i) {
    aoem_merkle_contract_mix_bytes(lanes, &index, parts[i], part_lens[i]);
  }
  for (uint32_t round = 0; round < 8u; ++round) {
    for (uint32_t lane = 0; lane < 8u; ++lane) {
      uint32_t rotate = 7u + ((round + lane) & 15u);
      uint32_t next = lanes[(lane + 1u) & 7u];
      next = (next << rotate) | (next >> (32u - rotate));
      lanes[lane] ^= next;
      lanes[lane] = lanes[lane] * 0x9e3779b1u + (0x7f4a7c15u ^ round);
    }
  }
  for (uint32_t lane = 0; lane < 8u; ++lane) {
    out[lane * 4u + 0u] = (uint8_t)(lanes[lane] & 0xffu);
    out[lane * 4u + 1u] = (uint8_t)((lanes[lane] >> 8u) & 0xffu);
    out[lane * 4u + 2u] = (uint8_t)((lanes[lane] >> 16u) & 0xffu);
    out[lane * 4u + 3u] = (uint8_t)((lanes[lane] >> 24u) & 0xffu);
  }
}

static void aoem_merkle_style_hash_pair_v1(
    const uint8_t left[32],
    const uint8_t right[32],
    uint8_t out[32]) {
  static const uint8_t label[] = "merkle_style_v1_node";
  const uint8_t* parts[2] = {left, right};
  size_t part_lens[2] = {32u, 32u};
  aoem_merkle_contract_digest32(label, sizeof(label) - 1u, parts, part_lens, 2u, out);
}

static int aoem_build_merkle_membership_fixture(
    uint64_t leaf_index,
    uint32_t tree_depth,
    byte_buf* public_input,
    byte_buf* witness) {
  if (tree_depth > AOEM_MERKLE_MEMBERSHIP_MAX_DEPTH ||
      (tree_depth < 64u && leaf_index >= (1ull << tree_depth))) {
    return -1;
  }
  uint8_t leaf_hash[32];
  for (uint32_t i = 0; i < 32u; ++i) {
    leaf_hash[i] = (uint8_t)(0x51u ^ (i * 11u));
  }
  uint8_t computed_root[32];
  memcpy(computed_root, leaf_hash, sizeof(computed_root));
  for (uint32_t level = 0; level < tree_depth; ++level) {
    uint8_t sibling[32];
    for (uint32_t i = 0; i < 32u; ++i) {
      sibling[i] = (uint8_t)(0xa0u + level + i * 7u);
    }
    if (buf_append(witness, sibling, sizeof(sibling)) != 0) {
      return -1;
    }
    uint8_t next[32];
    if (((leaf_index >> level) & 1ull) == 0ull) {
      aoem_merkle_style_hash_pair_v1(computed_root, sibling, next);
    } else {
      aoem_merkle_style_hash_pair_v1(sibling, computed_root, next);
    }
    memcpy(computed_root, next, sizeof(computed_root));
  }
  return buf_append(public_input, computed_root, 32u) != 0 ||
                 buf_append(public_input, leaf_hash, 32u) != 0 ||
                 buf_u64(public_input, leaf_index) != 0 ||
                 buf_u32(public_input, tree_depth) != 0
             ? -1
             : 0;
}

static void aoem_zk_merkle_leaf_commitment_v1(
    const uint8_t* leaf,
    size_t leaf_len,
    const uint8_t* secret,
    size_t secret_len,
    uint8_t out[32]) {
  static const uint8_t label[] = "zk_merkle_leaf_commitment_v1";
  const uint8_t* parts[2] = {leaf, secret};
  size_t part_lens[2] = {leaf_len, secret_len};
  aoem_merkle_contract_digest32(label, sizeof(label) - 1u, parts, part_lens, 2u, out);
}

static void aoem_zk_merkle_nullifier_v1(
    const uint8_t* leaf,
    size_t leaf_len,
    const uint8_t* secret,
    size_t secret_len,
    uint8_t out[32]) {
  static const uint8_t label[] = "zk_merkle_nullifier_v1";
  const uint8_t* parts[2] = {leaf, secret};
  size_t part_lens[2] = {leaf_len, secret_len};
  aoem_merkle_contract_digest32(label, sizeof(label) - 1u, parts, part_lens, 2u, out);
}

static int aoem_build_zk_merkle_membership_fixture(
    uint64_t leaf_index,
    uint32_t tree_depth,
    byte_buf* public_input,
    byte_buf* witness) {
  if (tree_depth > AOEM_MERKLE_MEMBERSHIP_MAX_DEPTH ||
      (tree_depth < 64u && leaf_index >= (1ull << tree_depth))) {
    return -1;
  }
  uint8_t leaf[32];
  uint8_t secret[32];
  for (uint32_t i = 0; i < 32u; ++i) {
    leaf[i] = (uint8_t)(0x61u ^ (uint8_t)(i * 13u + (uint32_t)leaf_index));
    secret[i] = (uint8_t)(0xc3u ^ (uint8_t)(i * 17u + (uint32_t)(leaf_index >> 1u)));
  }
  uint8_t leaf_commitment[32];
  uint8_t nullifier[32];
  aoem_zk_merkle_leaf_commitment_v1(leaf, sizeof(leaf), secret, sizeof(secret), leaf_commitment);
  aoem_zk_merkle_nullifier_v1(leaf, sizeof(leaf), secret, sizeof(secret), nullifier);

  byte_buf sibling_path = {0};
  uint8_t computed_root[32];
  memcpy(computed_root, leaf_commitment, sizeof(computed_root));
  for (uint32_t level = 0; level < tree_depth; ++level) {
    uint8_t sibling[32];
    for (uint32_t i = 0; i < 32u; ++i) {
      sibling[i] = (uint8_t)((0xb7u + level + i * 5u) ^ (uint32_t)leaf_index);
    }
    if (buf_append(&sibling_path, sibling, sizeof(sibling)) != 0) {
      buf_free(&sibling_path);
      return -1;
    }
    uint8_t next[32];
    if (((leaf_index >> level) & 1ull) == 0ull) {
      aoem_merkle_style_hash_pair_v1(computed_root, sibling, next);
    } else {
      aoem_merkle_style_hash_pair_v1(sibling, computed_root, next);
    }
    memcpy(computed_root, next, sizeof(computed_root));
  }

  int rc = buf_append(public_input, computed_root, 32u) != 0 ||
                   buf_append(public_input, leaf_commitment, 32u) != 0 ||
                   buf_append(public_input, nullifier, 32u) != 0 ||
                   buf_u32(public_input, tree_depth) != 0 ||
                   buf_u32(public_input, AOEM_ZK_MERKLE_STYLE_V1_HASH_PROFILE) != 0 ||
                   buf_u64(witness, leaf_index) != 0 ||
                   buf_u32(witness, (uint32_t)sizeof(leaf)) != 0 ||
                   buf_u32(witness, (uint32_t)sizeof(secret)) != 0 ||
                   buf_append(witness, leaf, sizeof(leaf)) != 0 ||
                   buf_append(witness, secret, sizeof(secret)) != 0 ||
                   buf_append(witness, sibling_path.data, sibling_path.len) != 0
               ? -1
               : 0;
  buf_free(&sibling_path);
  return rc;
}

static void* aoem_load_symbol(aoem_dynlib_t lib, const char* name) {
#ifdef _WIN32
  return (void*)GetProcAddress(lib, name);
#else
  return dlsym(lib, name);
#endif
}

static int load_api(const char* path, aoem_host_api* api) {
  memset(api, 0, sizeof(*api));
#ifdef _WIN32
  api->lib = LoadLibraryA(path);
#else
  api->lib = dlopen(path, RTLD_NOW | RTLD_LOCAL);
#endif
  if (!api->lib) {
    fprintf(stderr, "failed to load AOEM dynamic library: %s\n", path);
    return -1;
  }

#define LOAD_SYM(field, type_name, symbol)                                      \
  do {                                                                         \
    api->field = (type_name)aoem_load_symbol(api->lib, symbol);                 \
    if (!api->field) {                                                         \
      fprintf(stderr, "missing AOEM symbol: %s\n", symbol);                    \
      return -1;                                                               \
    }                                                                          \
  } while (0)

  LOAD_SYM(abi_version, aoem_abi_version_fn, "aoem_abi_version");
  LOAD_SYM(global_init, aoem_global_init_fn, "aoem_global_init");
  LOAD_SYM(create, aoem_create_fn, "aoem_create");
  LOAD_SYM(destroy, aoem_destroy_fn, "aoem_destroy");
  LOAD_SYM(execute_ops_wire_v1, aoem_execute_ops_wire_v1_fn, "aoem_execute_ops_wire_v1");
  LOAD_SYM(state_read_v1, aoem_state_read_v1_fn, "aoem_state_read_v1");
  api->free_buf = (aoem_free_fn)aoem_load_symbol(api->lib, "aoem_free");
  if (!api->free_buf) {
    fprintf(stderr, "missing AOEM symbol: aoem_free\n");
    return -1;
  }
#undef LOAD_SYM

  return 0;
}

static int append_wire_op(byte_buf* wire, uint8_t opcode, const char* key, const byte_buf* value) {
  const uint32_t key_len = (uint32_t)strlen(key);
  if (buf_append(wire, "AOV2\0", 5) != 0 || buf_u16(wire, 1) != 0 ||
      buf_u16(wire, 0) != 0 || buf_u32(wire, 1) != 0 || buf_u8(wire, opcode) != 0 ||
      buf_u8(wire, 0) != 0 || buf_u16(wire, 0) != 0 || buf_u32(wire, key_len) != 0 ||
      buf_u32(wire, (uint32_t)value->len) != 0 || buf_i64(wire, 0) != 0 ||
      buf_u64(wire, UINT64_MAX) != 0 || buf_u64(wire, 0) != 0 ||
      buf_append(wire, key, key_len) != 0 || buf_append(wire, value->data, value->len) != 0) {
    return -1;
  }
  return 0;
}

static int build_proof_payload(
    byte_buf* payload,
    const char* request_id,
    const char* output_prefix,
    const uint8_t* public_input,
    uint32_t public_input_len,
    const uint8_t* witness,
    uint32_t witness_len) {
  const uint16_t flags = (uint16_t)(1u | 2u | 4u | 8u);
  if (!public_input || public_input_len == 0 || !witness || witness_len == 0) {
    return -1;
  }
  if (buf_append(payload, "AOFP\0", 5) != 0 || buf_u16(payload, 2) != 0 ||
      buf_u16(payload, flags) != 0 || buf_u8(payload, 4) != 0 ||
      buf_append(payload, "\0\0\0", 3) != 0 ||
      buf_u16(payload, (uint16_t)strlen(request_id)) != 0 ||
      buf_u16(payload, (uint16_t)strlen(output_prefix)) != 0 ||
      buf_u32(payload, 1) != 0 || buf_u32(payload, 0xA0E05051u) != 0 ||
      buf_u32(payload, 0xA0E09EEDu) != 0 || buf_u32(payload, 256) != 0 ||
      buf_u32(payload, 1) != 0 || buf_u32(payload, 2) != 0 ||
      buf_u32(payload, 16) != 0 || buf_u32(payload, public_input_len) != 0 ||
      buf_u32(payload, witness_len) != 0 ||
      buf_append(payload, request_id, strlen(request_id)) != 0 ||
      buf_append(payload, output_prefix, strlen(output_prefix)) != 0 ||
      buf_append(payload, public_input, public_input_len) != 0 ||
      buf_append(payload, witness, witness_len) != 0) {
    return -1;
  }
  return 0;
}

static int build_proof_wire_with_input(
    byte_buf* wire,
    const char* request_id,
    const char* output_prefix,
    const uint8_t* public_input,
    uint32_t public_input_len,
    const uint8_t* witness,
    uint32_t witness_len) {
  byte_buf payload = {0};
  int rc = build_proof_payload(
      &payload,
      request_id,
      output_prefix,
      public_input,
      public_input_len,
      witness,
      witness_len);
  if (rc == 0) {
    rc = append_wire_op(wire, 98, output_prefix, &payload);
  }
  buf_free(&payload);
  return rc;
}

static int build_proof_wire(byte_buf* wire, const char* request_id, const char* output_prefix) {
  static const uint8_t public_input[] = {
      'a', 'o', 'e', 'm', ':', 'p', 'u', 'b', 'l', 'i', 'c', ':', 'v', '0', '2'};
  static const uint8_t witness[] = {
      'a', 'o', 'e', 'm', ':', 'w', 'i', 't', 'n', 'e', 's', 's', ':', 'v', '0', '2',
      0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88};
  return build_proof_wire_with_input(
      wire,
      request_id,
      output_prefix,
      public_input,
      (uint32_t)sizeof(public_input),
      witness,
      (uint32_t)sizeof(witness));
}

static void fill_batch_public_input(uint32_t batch_index, uint8_t out[16]) {
  for (uint32_t i = 0; i < 16u; ++i) {
    out[i] = (uint8_t)(0x40u + ((batch_index * 13u + i) & 0x3fu));
  }
}

static void fill_batch_witness(uint32_t batch_index, uint8_t out[32]) {
  for (uint32_t i = 0; i < 32u; ++i) {
    out[i] = (uint8_t)(0xa0u ^ ((batch_index * 29u + i * 7u) & 0xffu));
  }
}

static int build_proof_batch_payload_ex(
    byte_buf* payload,
    const char* request_id,
    const char* output_prefix,
    uint32_t batch_count,
    int use_resident_asset,
    uint32_t resident_asset_id,
    uint32_t profile_id) {
  const uint16_t flags = (uint16_t)(1u | 2u | 4u | 8u);
  if (batch_count == 0 || batch_count > 8u) {
    return -1;
  }
  if (buf_append(payload, "AOFP\0", 5) != 0 ||
      buf_u16(payload, (uint16_t)(use_resident_asset ? 4u : 3u)) != 0 ||
      buf_u16(payload, flags) != 0 || buf_u8(payload, 4) != 0 ||
      buf_append(payload, "\0\0\0", 3) != 0 ||
      buf_u16(payload, (uint16_t)strlen(request_id)) != 0 ||
      buf_u16(payload, (uint16_t)strlen(output_prefix)) != 0 ||
      buf_u32(payload, profile_id) != 0 || buf_u32(payload, 0xA0E05051u) != 0 ||
      buf_u32(payload, 0xA0E09EEDu) != 0 || buf_u32(payload, 256) != 0 ||
      buf_u32(payload, 1) != 0 || buf_u32(payload, 2) != 0 ||
      buf_u32(payload, 16) != 0 || buf_u32(payload, batch_count) != 0) {
    return -1;
  }
  if (use_resident_asset && buf_u32(payload, resident_asset_id) != 0) {
    return -1;
  }
  if (buf_append(payload, request_id, strlen(request_id)) != 0 ||
      buf_append(payload, output_prefix, strlen(output_prefix)) != 0) {
    return -1;
  }
  for (uint32_t batch_index = 0; batch_index < batch_count; ++batch_index) {
    uint8_t public_input[16];
    uint8_t witness[32];
    fill_batch_public_input(batch_index, public_input);
    fill_batch_witness(batch_index, witness);
    if (buf_u32(payload, (uint32_t)sizeof(public_input)) != 0 ||
        buf_u32(payload, (uint32_t)sizeof(witness)) != 0 ||
        buf_append(payload, public_input, sizeof(public_input)) != 0 ||
        buf_append(payload, witness, sizeof(witness)) != 0) {
      return -1;
    }
  }
  return 0;
}

static int build_proof_batch_payload(
    byte_buf* payload,
    const char* request_id,
    const char* output_prefix,
    uint32_t batch_count) {
  return build_proof_batch_payload_ex(
      payload,
      request_id,
      output_prefix,
      batch_count,
      0,
      AOEM_FIXED_PROFILE_RESIDENT_ASSET_V1_ID,
      AOEM_FIXED_PROFILE_RESIDENT_PROOF_V1_ID);
}

static int build_proof_resident_asset_batch_payload(
    byte_buf* payload,
    const char* request_id,
    const char* output_prefix,
    uint32_t batch_count,
    uint32_t resident_asset_id) {
  return build_proof_batch_payload_ex(
      payload,
      request_id,
      output_prefix,
      batch_count,
      1,
      resident_asset_id,
      AOEM_FIXED_PROFILE_RESIDENT_PROOF_V1_ID);
}

static int build_proof_merkle_membership_batch_payload(
    byte_buf* payload,
    const char* request_id,
    const char* output_prefix,
    uint32_t batch_count,
    uint32_t resident_asset_id) {
  const uint16_t flags = (uint16_t)(1u | 2u | 4u | 8u);
  if (batch_count == 0 || batch_count > 8u) {
    return -1;
  }
  if (buf_append(payload, "AOFP\0", 5) != 0 || buf_u16(payload, 4u) != 0 ||
      buf_u16(payload, flags) != 0 || buf_u8(payload, 4u) != 0 ||
      buf_append(payload, "\0\0\0", 3) != 0 ||
      buf_u16(payload, (uint16_t)strlen(request_id)) != 0 ||
      buf_u16(payload, (uint16_t)strlen(output_prefix)) != 0 ||
      buf_u32(payload, AOEM_MERKLE_MEMBERSHIP_PROOF_V1_ID) != 0 ||
      buf_u32(payload, 0xA0E05051u) != 0 || buf_u32(payload, 0xA0E09EEDu) != 0 ||
      buf_u32(payload, 256u) != 0 || buf_u32(payload, 1u) != 0 ||
      buf_u32(payload, 2u) != 0 || buf_u32(payload, 16u) != 0 ||
      buf_u32(payload, batch_count) != 0 || buf_u32(payload, resident_asset_id) != 0 ||
      buf_append(payload, request_id, strlen(request_id)) != 0 ||
      buf_append(payload, output_prefix, strlen(output_prefix)) != 0) {
    return -1;
  }
  for (uint32_t batch_index = 0; batch_index < batch_count; ++batch_index) {
    byte_buf public_input = {0};
    byte_buf witness = {0};
    int rc = aoem_build_merkle_membership_fixture(3u + batch_index, 4u, &public_input, &witness);
    if (rc == 0) {
      rc = buf_u32(payload, (uint32_t)public_input.len) != 0 ||
                   buf_u32(payload, (uint32_t)witness.len) != 0 ||
                   buf_append(payload, public_input.data, public_input.len) != 0 ||
                   buf_append(payload, witness.data, witness.len) != 0
               ? -1
               : 0;
    }
    buf_free(&public_input);
    buf_free(&witness);
    if (rc != 0) {
      return -1;
    }
  }
  return 0;
}

static int build_proof_zk_merkle_membership_batch_payload(
    byte_buf* payload,
    const char* request_id,
    const char* output_prefix,
    uint32_t batch_count,
    uint32_t resident_asset_id) {
  const uint16_t flags = (uint16_t)(1u | 2u | 4u | 8u);
  if (batch_count == 0 || batch_count > 8u) {
    return -1;
  }
  if (buf_append(payload, "AOFP\0", 5) != 0 || buf_u16(payload, 4u) != 0 ||
      buf_u16(payload, flags) != 0 || buf_u8(payload, 4u) != 0 ||
      buf_append(payload, "\0\0\0", 3) != 0 ||
      buf_u16(payload, (uint16_t)strlen(request_id)) != 0 ||
      buf_u16(payload, (uint16_t)strlen(output_prefix)) != 0 ||
      buf_u32(payload, AOEM_ZK_MERKLE_MEMBERSHIP_PROOF_V1_ID) != 0 ||
      buf_u32(payload, 0xA0E05051u) != 0 || buf_u32(payload, 0xA0E09EEDu) != 0 ||
      buf_u32(payload, 256u) != 0 || buf_u32(payload, 1u) != 0 ||
      buf_u32(payload, 2u) != 0 || buf_u32(payload, 16u) != 0 ||
      buf_u32(payload, batch_count) != 0 || buf_u32(payload, resident_asset_id) != 0 ||
      buf_append(payload, request_id, strlen(request_id)) != 0 ||
      buf_append(payload, output_prefix, strlen(output_prefix)) != 0) {
    return -1;
  }
  for (uint32_t batch_index = 0; batch_index < batch_count; ++batch_index) {
    byte_buf public_input = {0};
    byte_buf witness = {0};
    int rc =
        aoem_build_zk_merkle_membership_fixture(2u + batch_index, 4u, &public_input, &witness);
    if (rc == 0) {
      rc = buf_u32(payload, (uint32_t)public_input.len) != 0 ||
                   buf_u32(payload, (uint32_t)witness.len) != 0 ||
                   buf_append(payload, public_input.data, public_input.len) != 0 ||
                   buf_append(payload, witness.data, witness.len) != 0
               ? -1
               : 0;
    }
    buf_free(&public_input);
    buf_free(&witness);
    if (rc != 0) {
      return -1;
    }
  }
  return 0;
}

static int AOEM_MAYBE_UNUSED build_proof_batch_wire(
    byte_buf* wire,
    const char* request_id,
    const char* output_prefix,
    uint32_t batch_count) {
  byte_buf payload = {0};
  int rc = build_proof_batch_payload(&payload, request_id, output_prefix, batch_count);
  if (rc == 0) {
    rc = append_wire_op(wire, 98, output_prefix, &payload);
  }
  buf_free(&payload);
  return rc;
}

static int AOEM_MAYBE_UNUSED build_proof_resident_asset_batch_wire(
    byte_buf* wire,
    const char* request_id,
    const char* output_prefix,
    uint32_t batch_count,
    uint32_t resident_asset_id) {
  byte_buf payload = {0};
  int rc =
      build_proof_resident_asset_batch_payload(
          &payload,
          request_id,
          output_prefix,
          batch_count,
          resident_asset_id);
  if (rc == 0) {
    rc = append_wire_op(wire, 98, output_prefix, &payload);
  }
  buf_free(&payload);
  return rc;
}

static int AOEM_MAYBE_UNUSED build_proof_merkle_membership_batch_wire(
    byte_buf* wire,
    const char* request_id,
    const char* output_prefix,
    uint32_t batch_count,
    uint32_t resident_asset_id) {
  byte_buf payload = {0};
  int rc = build_proof_merkle_membership_batch_payload(
      &payload,
      request_id,
      output_prefix,
      batch_count,
      resident_asset_id);
  if (rc == 0) {
    rc = append_wire_op(wire, 98, output_prefix, &payload);
  }
  buf_free(&payload);
  return rc;
}

static int AOEM_MAYBE_UNUSED build_proof_zk_merkle_membership_batch_wire(
    byte_buf* wire,
    const char* request_id,
    const char* output_prefix,
    uint32_t batch_count,
    uint32_t resident_asset_id) {
  byte_buf payload = {0};
  int rc = build_proof_zk_merkle_membership_batch_payload(
      &payload,
      request_id,
      output_prefix,
      batch_count,
      resident_asset_id);
  if (rc == 0) {
    rc = append_wire_op(wire, 98, output_prefix, &payload);
  }
  buf_free(&payload);
  return rc;
}

static int build_malformed_proof_wire(
    byte_buf* wire,
    const char* request_id,
    const char* output_prefix) {
  byte_buf payload = {0};
  static const uint8_t public_input[] = {'b', 'a', 'd', ':', 'p', 'u', 'b'};
  static const uint8_t witness[] = {'b', 'a', 'd', ':', 'w', 'i', 't'};
  int rc = build_proof_payload(
      &payload,
      request_id,
      output_prefix,
      public_input,
      (uint32_t)sizeof(public_input),
      witness,
      (uint32_t)sizeof(witness));
  if (rc == 0 && payload.len > 4) {
    payload.len -= 4;
  }
  if (rc == 0) {
    rc = append_wire_op(wire, 98, output_prefix, &payload);
  }
  buf_free(&payload);
  return rc;
}

static int read_state_response(const aoem_host_api* api, const char* key, char** response_out) {
  char request[512];
  int written = snprintf(request, sizeof(request), "{\"key\":\"%s\"}", key);
  if (written <= 0 || (size_t)written >= sizeof(request)) {
    fprintf(stderr, "state read request overflow for key: %s\n", key);
    return -1;
  }
  uint8_t* out = NULL;
  size_t out_len = 0;
  int32_t rc = api->state_read_v1((const uint8_t*)request, (size_t)written, &out, &out_len);
  if (rc != 0 || !out || out_len == 0) {
    fprintf(stderr, "aoem_state_read_v1 failed for key %s rc=%d\n", key, rc);
    return -1;
  }
  char* response = (char*)malloc(out_len + 1);
  if (!response) {
    api->free_buf(out, out_len);
    return -1;
  }
  memcpy(response, out, out_len);
  response[out_len] = '\0';
  api->free_buf(out, out_len);
  *response_out = response;
  return 0;
}

static int read_state_found(const aoem_host_api* api, const char* key) {
  char* response = NULL;
  if (read_state_response(api, key, &response) != 0) {
    return -1;
  }
  int found = strstr(response, "\"found\":true") != NULL;
  free(response);
  return found ? 1 : 0;
}

static int read_state_contains_all(
    const aoem_host_api* api,
    const char* key,
    const char* needle_a,
    const char* needle_b,
    const char* needle_c) {
  char* response = NULL;
  if (read_state_response(api, key, &response) != 0) {
    return -1;
  }
  int ok = strstr(response, "\"found\":true") != NULL &&
           strstr(response, needle_a) != NULL &&
           strstr(response, needle_b) != NULL &&
           strstr(response, needle_c) != NULL;
  if (!ok) {
    fprintf(stderr, "unexpected state response for key %s: %s\n", key, response);
  }
  free(response);
  return ok ? 0 : -1;
}

static int run_success_smoke(const aoem_host_api* api, void* handle) {
  const char* request_id = "c-host-resident-proof";
  const char* output_prefix = "aoem.compute.output/c-host-resident-proof";
  const char* proof_key = "aoem.compute.output/c-host-resident-proof/zk/proof/bytes";
  const char* metadata_key = "aoem.compute.output/c-host-resident-proof/zk/proof/metadata";
  const char* status_key = "aoem.compute.output/c-host-resident-proof/zk/proof/status";
  const char* public_outputs_key =
      "aoem.compute.output/c-host-resident-proof/zk/proof/public_outputs";
  const char* verify_status_key =
      "aoem.compute.output/c-host-resident-proof/zk/proof/verify_status";

  byte_buf wire = {0};
  if (build_proof_wire(&wire, request_id, output_prefix) != 0) {
    fprintf(stderr, "failed to build resident proof wire payload\n");
    buf_free(&wire);
    return -1;
  }

  aoem_exec_v2_result result = {0};
  int32_t rc = api->execute_ops_wire_v1(handle, wire.data, wire.len, &result);
  buf_free(&wire);
  if (rc != 0 || result.processed != 1 || result.success != 1 || result.total_writes != 5) {
    fprintf(
        stderr,
        "resident proof execute failed rc=%d processed=%u success=%u writes=%llu\n",
        rc,
        result.processed,
        result.success,
        (unsigned long long)result.total_writes);
    return -1;
  }

  if (read_state_contains_all(
          api,
          proof_key,
          "compute.zk.resident_proof_v1",
          "\"fixed_profile_verifier_accepted\":true",
          "\"real_input_used\":true") != 0) {
    return -1;
  }
  if (read_state_contains_all(
          api,
          metadata_key,
          "\"public_capability\":true",
          "\"input_source\":\"payload_v2_real_input\"",
          "\"runtime_canon_unchanged\":true") != 0) {
    return -1;
  }
  if (read_state_contains_all(
          api,
          status_key,
          "compute.zk.resident_proof_v1.status",
          "\"proof_verified\":true",
          "\"state_read\":\"aoem_state_read_v1\"") != 0) {
    return -1;
  }
  if (read_state_contains_all(
          api,
          public_outputs_key,
          "compute.zk.resident_proof_v1.public_outputs",
          "\"real_input_used\":true",
          "\"witness_digest_blake3_hex\":") != 0) {
    return -1;
  }
  if (read_state_contains_all(
          api,
          verify_status_key,
          "compute.zk.resident_proof_v1.verify_status",
          "\"accepted\":true",
          "\"real_input_used\":true") != 0) {
    return -1;
  }
  return 0;
}

static int run_malformed_smoke(const aoem_host_api* api, void* handle) {
  const char* request_id = "c-host-resident-proof-malformed";
  const char* output_prefix = "aoem.compute.output/c-host-resident-proof-malformed";
  const char* proof_key = "aoem.compute.output/c-host-resident-proof-malformed/zk/proof/bytes";
  const char* metadata_key =
      "aoem.compute.output/c-host-resident-proof-malformed/zk/proof/metadata";
  const char* status_key = "aoem.compute.output/c-host-resident-proof-malformed/zk/proof/status";
  const char* public_outputs_key =
      "aoem.compute.output/c-host-resident-proof-malformed/zk/proof/public_outputs";
  const char* verify_status_key =
      "aoem.compute.output/c-host-resident-proof-malformed/zk/proof/verify_status";

  byte_buf wire = {0};
  if (build_malformed_proof_wire(&wire, request_id, output_prefix) != 0) {
    fprintf(stderr, "failed to build malformed resident proof wire payload\n");
    buf_free(&wire);
    return -1;
  }

  aoem_exec_v2_result result = {99, 99, 0, 99};
  int32_t rc = api->execute_ops_wire_v1(handle, wire.data, wire.len, &result);
  buf_free(&wire);
  if (rc == 0 || result.success != 0 || result.total_writes != 0) {
    fprintf(
        stderr,
        "malformed resident proof unexpectedly succeeded rc=%d success=%u writes=%llu\n",
        rc,
        result.success,
        (unsigned long long)result.total_writes);
    return -1;
  }

  if (read_state_found(api, proof_key) != 0 || read_state_found(api, metadata_key) != 0 ||
      read_state_found(api, status_key) != 0 || read_state_found(api, public_outputs_key) != 0 ||
      read_state_found(api, verify_status_key) != 0) {
    fprintf(stderr, "malformed resident proof wrote state unexpectedly\n");
    return -1;
  }
  return 0;
}

int main(int argc, char** argv) {
  const char* lib_path = NULL;
  if (argc > 1) {
    lib_path = argv[1];
  } else {
#ifdef _WIN32
    lib_path = "target\\release\\aoem_ffi.dll";
#elif defined(__APPLE__)
    lib_path = "target/release/libaoem_ffi.dylib";
#else
    lib_path = "target/release/libaoem_ffi.so";
#endif
  }

  aoem_host_api api;
  if (load_api(lib_path, &api) != 0) {
    return 2;
  }
  if (api.abi_version() != 1) {
    fprintf(stderr, "unexpected AOEM ABI version\n");
    return 2;
  }
  if (api.global_init() != 0) {
    fprintf(stderr, "aoem_global_init failed\n");
    return 2;
  }
  void* handle = api.create();
  if (!handle) {
    fprintf(stderr, "aoem_create failed\n");
    return 2;
  }

  int ok = run_success_smoke(&api, handle);
  if (ok == 0) {
    ok = run_malformed_smoke(&api, handle);
  }

  api.destroy(handle);
  if (ok != 0) {
    return 1;
  }
  printf(
      "C_HOST_RESIDENT_PROOF_SMOKE|real_input=ok|proof=ok|verify=ok|status=ok|metadata=ok|malformed=ok\n");
  return 0;
}
