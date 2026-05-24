// Standalone verifier smoke for the fixed-profile resident proof workload.
//
// This external host example first obtains proof bytes through the existing
// wire_v1 product path, then verifies the proof envelope without consulting
// AOEM's internal verify_status.

#define main aoem_resident_proof_smoke_main
#include "hosted_resident_proof_smoke.c"
#undef main

#define AOEM_PROOF_CONTRACT_V3_PREFIX_LEN (5u + 2u + 4u + 32u * 4u + 4u)
#define AOEM_PROOF_CONTRACT_PAYLOAD_WORDS 13u
#define AOEM_PROOF_CONTRACT_PAYLOAD_LEN (AOEM_PROOF_CONTRACT_PAYLOAD_WORDS * 4u)

typedef struct aoem_proof_contract_v3 {
  uint32_t profile_id;
  const uint8_t* public_input_digest;
  const uint8_t* witness_digest;
  const uint8_t* pipeline_digest;
  const uint8_t* public_outputs_digest;
  const uint8_t* payload;
  uint32_t payload_len;
  uint32_t checksum;
} aoem_proof_contract_v3;

static uint32_t read_u32_le_at(const uint8_t* data) {
  return ((uint32_t)data[0]) | ((uint32_t)data[1] << 8) | ((uint32_t)data[2] << 16) |
         ((uint32_t)data[3] << 24);
}

static void write_u32_le_to(byte_buf* b, uint32_t v) {
  (void)buf_u32(b, v);
}

static void contract_mix_byte(uint32_t lanes[8], uint32_t* index, uint8_t byte) {
  uint32_t lane = *index & 7u;
  uint32_t rotate = 5u + ((lane + *index) & 15u);
  uint32_t value = lanes[lane] ^ (uint32_t)byte;
  value *= 0x01000193u;
  value = (value << rotate) | (value >> (32u - rotate));
  value += 0x9e3779b9u ^ ((*index) * 0x85ebca6bu);
  lanes[lane] = value;
  *index += 1u;
}

static void contract_mix_bytes(uint32_t lanes[8], uint32_t* index, const uint8_t* data, size_t len) {
  uint64_t n = (uint64_t)len;
  for (uint32_t i = 0; i < 8u; ++i) {
    contract_mix_byte(lanes, index, (uint8_t)((n >> (i * 8u)) & 0xffu));
  }
  for (size_t i = 0; i < len; ++i) {
    contract_mix_byte(lanes, index, data[i]);
  }
}

static void contract_digest32(
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
  contract_mix_bytes(lanes, &index, domain, sizeof(domain) - 1u);
  contract_mix_bytes(lanes, &index, label, label_len);
  uint8_t count_le[8];
  uint64_t count = (uint64_t)part_count;
  for (uint32_t i = 0; i < 8u; ++i) {
    count_le[i] = (uint8_t)((count >> (i * 8u)) & 0xffu);
  }
  contract_mix_bytes(lanes, &index, count_le, sizeof(count_le));
  for (size_t i = 0; i < part_count; ++i) {
    contract_mix_bytes(lanes, &index, parts[i], part_lens[i]);
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

static uint32_t contract_checksum_u32(const uint8_t* data, size_t len) {
  static const uint8_t label[] = "proof_checksum";
  const uint8_t* parts[1] = {data};
  size_t part_lens[1] = {len};
  uint8_t digest[32];
  contract_digest32(label, sizeof(label) - 1u, parts, part_lens, 1, digest);
  return read_u32_le_at(digest);
}

static int hex_nibble(char c) {
  if (c >= '0' && c <= '9') {
    return c - '0';
  }
  if (c >= 'a' && c <= 'f') {
    return c - 'a' + 10;
  }
  if (c >= 'A' && c <= 'F') {
    return c - 'A' + 10;
  }
  return -1;
}

static int hex_to_bytes(const char* hex, uint8_t** out, size_t* out_len) {
  size_t len = strlen(hex);
  if ((len & 1u) != 0) {
    return -1;
  }
  uint8_t* bytes = (uint8_t*)malloc(len / 2u);
  if (!bytes) {
    return -1;
  }
  for (size_t i = 0; i < len; i += 2u) {
    int hi = hex_nibble(hex[i]);
    int lo = hex_nibble(hex[i + 1u]);
    if (hi < 0 || lo < 0) {
      free(bytes);
      return -1;
    }
    bytes[i / 2u] = (uint8_t)((hi << 4) | lo);
  }
  *out = bytes;
  *out_len = len / 2u;
  return 0;
}

static int json_dup_string_field(const char* json, const char* field, char** out) {
  char needle[128];
  int written = snprintf(needle, sizeof(needle), "\"%s\":\"", field);
  if (written <= 0 || (size_t)written >= sizeof(needle)) {
    return -1;
  }
  const char* start = strstr(json, needle);
  if (!start) {
    return -1;
  }
  start += strlen(needle);
  const char* end = strchr(start, '"');
  if (!end || end < start) {
    return -1;
  }
  size_t len = (size_t)(end - start);
  char* value = (char*)malloc(len + 1u);
  if (!value) {
    return -1;
  }
  memcpy(value, start, len);
  value[len] = '\0';
  *out = value;
  return 0;
}

static int parse_proof_contract_v3(
    const uint8_t* proof,
    size_t proof_len,
    aoem_proof_contract_v3* parsed) {
  if (!proof || proof_len < AOEM_PROOF_CONTRACT_V3_PREFIX_LEN + 4u ||
      memcmp(proof, "AORF\0", 5u) != 0 || proof[5] != 3u || proof[6] != 0u) {
    return -1;
  }
  uint32_t payload_len = read_u32_le_at(proof + AOEM_PROOF_CONTRACT_V3_PREFIX_LEN - 4u);
  if (payload_len != AOEM_PROOF_CONTRACT_PAYLOAD_LEN ||
      proof_len != AOEM_PROOF_CONTRACT_V3_PREFIX_LEN + payload_len + 4u) {
    return -1;
  }
  uint32_t checksum_offset = AOEM_PROOF_CONTRACT_V3_PREFIX_LEN + payload_len;
  uint32_t expected = contract_checksum_u32(proof, checksum_offset);
  uint32_t actual = read_u32_le_at(proof + checksum_offset);
  if (expected != actual) {
    return -1;
  }
  parsed->profile_id = read_u32_le_at(proof + 7u);
  parsed->public_input_digest = proof + 11u;
  parsed->witness_digest = proof + 43u;
  parsed->pipeline_digest = proof + 75u;
  parsed->public_outputs_digest = proof + 107u;
  parsed->payload_len = payload_len;
  parsed->payload = proof + AOEM_PROOF_CONTRACT_V3_PREFIX_LEN;
  parsed->checksum = actual;
  return 0;
}

static int compute_pipeline_digest_from_payload(const uint8_t* payload, uint8_t out[32]) {
  byte_buf b = {0};
  write_u32_le_to(&b, read_u32_le_at(payload + 0u));
  write_u32_le_to(&b, read_u32_le_at(payload + 16u));
  write_u32_le_to(&b, read_u32_le_at(payload + 20u));
  write_u32_le_to(&b, read_u32_le_at(payload + 24u));
  write_u32_le_to(&b, read_u32_le_at(payload + 28u));
  write_u32_le_to(&b, read_u32_le_at(payload + 32u));
  static const uint8_t label[] = "pipeline";
  const uint8_t* parts[1] = {b.data};
  size_t part_lens[1] = {b.len};
  contract_digest32(label, sizeof(label) - 1u, parts, part_lens, 1, out);
  buf_free(&b);
  return 0;
}

static int compute_public_outputs_digest_from_payload(
    const aoem_proof_contract_v3* proof,
    uint8_t out[32]) {
  const uint8_t* payload = proof->payload;
  byte_buf b = {0};
  write_u32_le_to(&b, read_u32_le_at(payload + 0u));
  write_u32_le_to(&b, read_u32_le_at(payload + 4u));
  write_u32_le_to(&b, read_u32_le_at(payload + 36u));
  write_u32_le_to(&b, read_u32_le_at(payload + 40u));
  (void)buf_append(&b, proof->public_input_digest, 32u);
  (void)buf_append(&b, proof->witness_digest, 32u);
  (void)buf_append(&b, proof->pipeline_digest, 32u);
  write_u32_le_to(&b, read_u32_le_at(payload + 24u));
  write_u32_le_to(&b, read_u32_le_at(payload + 28u));
  write_u32_le_to(&b, read_u32_le_at(payload + 32u));
  static const uint8_t label[] = "public_outputs";
  const uint8_t* parts[1] = {b.data};
  size_t part_lens[1] = {b.len};
  contract_digest32(label, sizeof(label) - 1u, parts, part_lens, 1, out);
  buf_free(&b);
  return 0;
}

static int compute_merkle_membership_root_from_inputs(
    const uint8_t* public_input,
    size_t public_input_len,
    const uint8_t* witness,
    size_t witness_len,
    uint8_t path_digest[32],
    uint8_t computed_root[32]) {
  if (public_input_len != AOEM_MERKLE_MEMBERSHIP_PUBLIC_INPUT_LEN) {
    return -1;
  }
  uint64_t leaf_index = 0;
  for (uint32_t i = 0; i < 8u; ++i) {
    leaf_index |= ((uint64_t)public_input[64u + i]) << (i * 8u);
  }
  uint32_t tree_depth = read_u32_le_at(public_input + 72u);
  if (tree_depth > AOEM_MERKLE_MEMBERSHIP_MAX_DEPTH ||
      (tree_depth < 64u && leaf_index >= (1ull << tree_depth)) ||
      witness_len != (size_t)tree_depth * 32u) {
    return -1;
  }
  memcpy(computed_root, public_input + 32u, 32u);
  for (uint32_t level = 0; level < tree_depth; ++level) {
    const uint8_t* sibling = witness + (size_t)level * 32u;
    uint8_t next[32];
    if (((leaf_index >> level) & 1ull) == 0ull) {
      aoem_merkle_style_hash_pair_v1(computed_root, sibling, next);
    } else {
      aoem_merkle_style_hash_pair_v1(sibling, computed_root, next);
    }
    memcpy(computed_root, next, 32u);
  }
  if (memcmp(computed_root, public_input, 32u) != 0) {
    return -1;
  }
  static const uint8_t label[] = "merkle_membership_path";
  const uint8_t* parts[3] = {public_input + 64u, public_input + 72u, witness};
  size_t part_lens[3] = {8u, 4u, witness_len};
  contract_digest32(label, sizeof(label) - 1u, parts, part_lens, 3u, path_digest);
  return 0;
}

static int compute_merkle_public_outputs_digest_from_payload(
    const aoem_proof_contract_v3* proof,
    const uint8_t* public_input,
    size_t public_input_len,
    const uint8_t* witness,
    size_t witness_len,
    uint8_t out[32]) {
  uint8_t path_digest[32];
  uint8_t computed_root[32];
  if (compute_merkle_membership_root_from_inputs(
          public_input,
          public_input_len,
          witness,
          witness_len,
          path_digest,
          computed_root) != 0) {
    return -1;
  }

  const uint8_t* payload = proof->payload;
  byte_buf generic = {0};
  byte_buf merkle = {0};
  write_u32_le_to(&generic, read_u32_le_at(payload + 0u));
  write_u32_le_to(&generic, read_u32_le_at(payload + 4u));
  write_u32_le_to(&generic, read_u32_le_at(payload + 36u));
  write_u32_le_to(&generic, read_u32_le_at(payload + 40u));
  (void)buf_append(&generic, proof->public_input_digest, 32u);
  (void)buf_append(&generic, proof->witness_digest, 32u);
  (void)buf_append(&generic, proof->pipeline_digest, 32u);
  write_u32_le_to(&generic, read_u32_le_at(payload + 24u));
  write_u32_le_to(&generic, read_u32_le_at(payload + 28u));
  write_u32_le_to(&generic, read_u32_le_at(payload + 32u));

  (void)buf_append(&merkle, public_input, 32u);
  (void)buf_append(&merkle, public_input + 32u, 32u);
  (void)buf_append(&merkle, public_input + 64u, 8u);
  (void)buf_append(&merkle, public_input + 72u, 4u);
  (void)buf_append(&merkle, path_digest, 32u);
  (void)buf_append(&merkle, computed_root, 32u);

  static const uint8_t label[] = "public_outputs";
  const uint8_t* parts[2] = {generic.data, merkle.data};
  size_t part_lens[2] = {generic.len, merkle.len};
  contract_digest32(label, sizeof(label) - 1u, parts, part_lens, 2u, out);
  buf_free(&generic);
  buf_free(&merkle);
  return 0;
}

static int compute_zk_merkle_public_outputs_digest_from_payload(
    const aoem_proof_contract_v3* proof,
    const uint8_t* public_input,
    size_t public_input_len,
    const char* public_outputs_json,
    uint8_t out[32]) {
  if (public_input_len != 32u + 32u + 32u + 4u + 4u || !public_outputs_json ||
      strstr(public_outputs_json, "\"leaf_hash\"") != NULL ||
      strstr(public_outputs_json, "\"leaf_index\"") != NULL ||
      strstr(public_outputs_json, "\"sibling_path\"") != NULL ||
      strstr(public_outputs_json, "\"path_digest\"") != NULL ||
      strstr(public_outputs_json, "\"computed_root\"") != NULL ||
      strstr(public_outputs_json, "\"private_witness_hidden\":true") == NULL) {
    return -1;
  }

  char* root_hex = NULL;
  char* commitment_hex = NULL;
  char* nullifier_hex = NULL;
  char* witness_commitment_hex = NULL;
  uint8_t* root = NULL;
  uint8_t* commitment = NULL;
  uint8_t* nullifier = NULL;
  uint8_t* witness_commitment = NULL;
  size_t root_len = 0;
  size_t commitment_len = 0;
  size_t nullifier_len = 0;
  size_t witness_commitment_len = 0;
  int rc = -1;

  if (json_dup_string_field(public_outputs_json, "root", &root_hex) != 0 ||
      json_dup_string_field(public_outputs_json, "leaf_commitment", &commitment_hex) != 0 ||
      json_dup_string_field(public_outputs_json, "nullifier", &nullifier_hex) != 0 ||
      json_dup_string_field(public_outputs_json, "witness_commitment", &witness_commitment_hex) != 0 ||
      hex_to_bytes(root_hex, &root, &root_len) != 0 || root_len != 32u ||
      hex_to_bytes(commitment_hex, &commitment, &commitment_len) != 0 ||
      commitment_len != 32u ||
      hex_to_bytes(nullifier_hex, &nullifier, &nullifier_len) != 0 || nullifier_len != 32u ||
      hex_to_bytes(witness_commitment_hex, &witness_commitment, &witness_commitment_len) != 0 ||
      witness_commitment_len != 32u) {
    goto done;
  }
  if (memcmp(root, public_input, 32u) != 0 ||
      memcmp(commitment, public_input + 32u, 32u) != 0 ||
      memcmp(nullifier, public_input + 64u, 32u) != 0) {
    goto done;
  }

  const uint8_t* payload = proof->payload;
  byte_buf generic = {0};
  byte_buf zk = {0};
  write_u32_le_to(&generic, read_u32_le_at(payload + 0u));
  write_u32_le_to(&generic, read_u32_le_at(payload + 4u));
  write_u32_le_to(&generic, read_u32_le_at(payload + 36u));
  write_u32_le_to(&generic, read_u32_le_at(payload + 40u));
  (void)buf_append(&generic, proof->public_input_digest, 32u);
  (void)buf_append(&generic, proof->witness_digest, 32u);
  (void)buf_append(&generic, proof->pipeline_digest, 32u);
  write_u32_le_to(&generic, read_u32_le_at(payload + 24u));
  write_u32_le_to(&generic, read_u32_le_at(payload + 28u));
  write_u32_le_to(&generic, read_u32_le_at(payload + 32u));

  (void)buf_append(&zk, public_input, 32u);
  (void)buf_append(&zk, public_input + 32u, 32u);
  (void)buf_append(&zk, public_input + 64u, 32u);
  (void)buf_append(&zk, public_input + 96u, 4u);
  (void)buf_append(&zk, public_input + 100u, 4u);
  (void)buf_append(&zk, witness_commitment, 32u);

  static const uint8_t label[] = "public_outputs";
  const uint8_t* parts[2] = {generic.data, zk.data};
  size_t part_lens[2] = {generic.len, zk.len};
  contract_digest32(label, sizeof(label) - 1u, parts, part_lens, 2u, out);
  buf_free(&generic);
  buf_free(&zk);
  rc = 0;

done:
  free(root_hex);
  free(commitment_hex);
  free(nullifier_hex);
  free(witness_commitment_hex);
  free(root);
  free(commitment);
  free(nullifier);
  free(witness_commitment);
  return rc;
}

static void bytes_to_hex_lower(const uint8_t* bytes, size_t len, char* out) {
  static const char hex[] = "0123456789abcdef";
  for (size_t i = 0; i < len; ++i) {
    out[i * 2u] = hex[(bytes[i] >> 4u) & 0x0fu];
    out[i * 2u + 1u] = hex[bytes[i] & 0x0fu];
  }
  out[len * 2u] = '\0';
}

static int verify_contract_against_inputs(
    const uint8_t* proof,
    size_t proof_len,
    const uint8_t* public_input,
    size_t public_input_len,
    const uint8_t* witness,
    size_t witness_len,
    uint32_t expected_profile_id,
    const char* public_outputs_json) {
  aoem_proof_contract_v3 parsed;
  if (parse_proof_contract_v3(proof, proof_len, &parsed) != 0 ||
      parsed.profile_id != expected_profile_id) {
    return -1;
  }

  uint8_t expected_public_input_digest[32];
  uint8_t expected_witness_digest[32];
  uint8_t expected_pipeline_digest[32];
  uint8_t expected_public_outputs_digest[32];
  static const uint8_t public_label[] = "public_input";
  static const uint8_t witness_label[] = "witness_or_scalars";
  const uint8_t* public_parts[1] = {public_input};
  size_t public_part_lens[1] = {public_input_len};
  const uint8_t* witness_parts[1] = {witness};
  size_t witness_part_lens[1] = {witness_len};
  const int zk_without_private_witness =
      expected_profile_id == AOEM_ZK_MERKLE_MEMBERSHIP_PROOF_V1_ID && witness == NULL &&
      witness_len == 0u;
  contract_digest32(
      public_label,
      sizeof(public_label) - 1u,
      public_parts,
      public_part_lens,
      1,
      expected_public_input_digest);
  if (!zk_without_private_witness) {
    contract_digest32(
        witness_label,
        sizeof(witness_label) - 1u,
        witness_parts,
        witness_part_lens,
        1,
        expected_witness_digest);
  }
  compute_pipeline_digest_from_payload(parsed.payload, expected_pipeline_digest);
  if (expected_profile_id == AOEM_MERKLE_MEMBERSHIP_PROOF_V1_ID) {
    if (compute_merkle_public_outputs_digest_from_payload(
            &parsed,
            public_input,
            public_input_len,
            witness,
            witness_len,
            expected_public_outputs_digest) != 0) {
      return -1;
    }
  } else if (expected_profile_id == AOEM_ZK_MERKLE_MEMBERSHIP_PROOF_V1_ID) {
    if (compute_zk_merkle_public_outputs_digest_from_payload(
            &parsed,
            public_input,
            public_input_len,
            public_outputs_json,
            expected_public_outputs_digest) != 0) {
      return -1;
    }
  } else {
    compute_public_outputs_digest_from_payload(&parsed, expected_public_outputs_digest);
  }

  if (memcmp(parsed.public_input_digest, expected_public_input_digest, 32u) != 0 ||
      (!zk_without_private_witness &&
       memcmp(parsed.witness_digest, expected_witness_digest, 32u) != 0) ||
      memcmp(parsed.pipeline_digest, expected_pipeline_digest, 32u) != 0 ||
      memcmp(parsed.public_outputs_digest, expected_public_outputs_digest, 32u) != 0) {
    return -1;
  }
  if (public_outputs_json) {
    char* digest_hex = NULL;
    char expected_hex[65];
    bytes_to_hex_lower(expected_public_outputs_digest, 32u, expected_hex);
    if (json_dup_string_field(public_outputs_json, "proof_public_outputs_digest_hex", &digest_hex) != 0) {
      return -1;
    }
    int ok = strcmp(digest_hex, expected_hex) == 0;
    free(digest_hex);
    if (!ok) {
      return -1;
    }
  }
  return 0;
}

static int AOEM_MAYBE_UNUSED run_external_verifier_smoke(const aoem_host_api* api, void* handle) {
  const char* request_id = "c-host-resident-proof-verify";
  const char* output_prefix = "aoem.compute.output/c-host-resident-proof-verify";
  const char* proof_key = "aoem.compute.output/c-host-resident-proof-verify/zk/proof/bytes";
  const char* public_outputs_key =
      "aoem.compute.output/c-host-resident-proof-verify/zk/proof/public_outputs";
  static const uint8_t public_input[] = {
      'a', 'o', 'e', 'm', ':', 'p', 'u', 'b', 'l', 'i', 'c', ':', 'v', '0', '3'};
  static const uint8_t witness[] = {
      'a', 'o', 'e', 'm', ':', 'w', 'i', 't', 'n', 'e', 's', 's', ':', 'v', '0', '3',
      0x31, 0x32, 0x33, 0x34, 0x41, 0x42, 0x43, 0x44};

  byte_buf wire = {0};
  if (build_proof_wire_with_input(
          &wire,
          request_id,
          output_prefix,
          public_input,
          (uint32_t)sizeof(public_input),
          witness,
          (uint32_t)sizeof(witness)) != 0) {
    fprintf(stderr, "failed to build verifier resident proof wire payload\n");
    buf_free(&wire);
    return -1;
  }
  aoem_exec_v2_result result = {0};
  int32_t rc = api->execute_ops_wire_v1(handle, wire.data, wire.len, &result);
  buf_free(&wire);
  if (rc != 0 || result.processed != 1 || result.success != 1 || result.total_writes != 5) {
    fprintf(stderr, "resident proof verifier setup execute failed rc=%d\n", rc);
    return -1;
  }

  char* proof_response = NULL;
  char* public_outputs_response = NULL;
  char* proof_hex = NULL;
  uint8_t* proof_bytes = NULL;
  size_t proof_len = 0;
  if (read_state_response(api, proof_key, &proof_response) != 0 ||
      read_state_response(api, public_outputs_key, &public_outputs_response) != 0 ||
      json_dup_string_field(proof_response, "proof_bytes_hex", &proof_hex) != 0 ||
      hex_to_bytes(proof_hex, &proof_bytes, &proof_len) != 0) {
    free(proof_response);
    free(public_outputs_response);
    free(proof_hex);
    free(proof_bytes);
    return -1;
  }

  int ok = verify_contract_against_inputs(
      proof_bytes,
      proof_len,
      public_input,
      sizeof(public_input),
      witness,
      sizeof(witness),
      1u,
      public_outputs_response);
  uint8_t tampered_public_input[sizeof(public_input)];
  memcpy(tampered_public_input, public_input, sizeof(public_input));
  tampered_public_input[0] ^= 0x55u;
  int public_tamper_rejected =
      verify_contract_against_inputs(
          proof_bytes,
          proof_len,
          tampered_public_input,
          sizeof(tampered_public_input),
          witness,
          sizeof(witness),
          1u,
          public_outputs_response) != 0;
  uint8_t* tampered_proof = (uint8_t*)malloc(proof_len);
  int proof_tamper_rejected = 0;
  if (tampered_proof) {
    memcpy(tampered_proof, proof_bytes, proof_len);
    if (proof_len > AOEM_PROOF_CONTRACT_V3_PREFIX_LEN + 4u) {
      tampered_proof[AOEM_PROOF_CONTRACT_V3_PREFIX_LEN + 3u] ^= 0x01u;
    }
    proof_tamper_rejected =
        verify_contract_against_inputs(
            tampered_proof,
            proof_len,
            public_input,
            sizeof(public_input),
            witness,
            sizeof(witness),
            1u,
            public_outputs_response) != 0;
    free(tampered_proof);
  }
  int profile_tamper_rejected =
      verify_contract_against_inputs(
          proof_bytes,
          proof_len,
          public_input,
          sizeof(public_input),
          witness,
          sizeof(witness),
          2u,
          public_outputs_response) != 0;

  free(proof_response);
  free(public_outputs_response);
  free(proof_hex);
  free(proof_bytes);

  return ok == 0 && public_tamper_rejected && proof_tamper_rejected && profile_tamper_rejected
             ? 0
             : -1;
}

static int AOEM_MAYBE_UNUSED run_external_batch_verifier_smoke(
    const aoem_host_api* api,
    void* handle,
    uint32_t batch_count) {
  const char* request_id = "c-host-resident-proof-verify-batch";
  const char* output_prefix = "aoem.compute.output/c-host-resident-proof-verify-batch";

  byte_buf wire = {0};
  if (build_proof_resident_asset_batch_wire(
          &wire,
          request_id,
          output_prefix,
          batch_count,
          AOEM_FIXED_PROFILE_RESIDENT_ASSET_V1_ID) != 0) {
    fprintf(stderr, "failed to build verifier resident asset proof batch wire payload\n");
    buf_free(&wire);
    return -1;
  }
  aoem_exec_v2_result result = {0};
  int32_t rc = api->execute_ops_wire_v1(handle, wire.data, wire.len, &result);
  buf_free(&wire);
  const uint64_t expected_writes = 3u + 5u * (uint64_t)batch_count;
  if (rc != 0 || result.processed != 1 || result.success != 1 ||
      result.total_writes != expected_writes) {
    fprintf(stderr, "resident proof batch verifier setup execute failed rc=%d\n", rc);
    return -1;
  }

  for (uint32_t batch_index = 0; batch_index < batch_count; ++batch_index) {
    char proof_key[256];
    char public_outputs_key[256];
    if (snprintf(proof_key, sizeof(proof_key), "%s/zk/proof/%u/bytes", output_prefix, batch_index) <=
            0 ||
        snprintf(
            public_outputs_key,
            sizeof(public_outputs_key),
            "%s/zk/proof/%u/public_outputs",
            output_prefix,
            batch_index) <= 0) {
      return -1;
    }

    char* proof_response = NULL;
    char* public_outputs_response = NULL;
    char* proof_hex = NULL;
    uint8_t* proof_bytes = NULL;
    size_t proof_len = 0;
    if (read_state_response(api, proof_key, &proof_response) != 0 ||
        read_state_response(api, public_outputs_key, &public_outputs_response) != 0 ||
        json_dup_string_field(proof_response, "proof_bytes_hex", &proof_hex) != 0 ||
        hex_to_bytes(proof_hex, &proof_bytes, &proof_len) != 0) {
      free(proof_response);
      free(public_outputs_response);
      free(proof_hex);
      free(proof_bytes);
      return -1;
    }

    uint8_t public_input[16];
    uint8_t witness[32];
    fill_batch_public_input(batch_index, public_input);
    fill_batch_witness(batch_index, witness);
    int ok = verify_contract_against_inputs(
        proof_bytes,
        proof_len,
        public_input,
        sizeof(public_input),
        witness,
        sizeof(witness),
        1u,
        public_outputs_response);

    free(proof_response);
    free(public_outputs_response);
    free(proof_hex);
    free(proof_bytes);
    if (ok != 0) {
      return -1;
    }
  }
  return 0;
}

static int AOEM_MAYBE_UNUSED run_merkle_membership_verifier_smoke(
    const aoem_host_api* api,
    void* handle) {
  const char* request_id = "c-host-resident-proof-verify-merkle";
  const char* output_prefix = "aoem.compute.output/c-host-resident-proof-verify-merkle";
  const char* proof_key =
      "aoem.compute.output/c-host-resident-proof-verify-merkle/zk/proof/0/bytes";
  const char* public_outputs_key =
      "aoem.compute.output/c-host-resident-proof-verify-merkle/zk/proof/0/public_outputs";

  byte_buf wire = {0};
  if (build_proof_merkle_membership_batch_wire(
          &wire,
          request_id,
          output_prefix,
          1u,
          AOEM_FIXED_PROFILE_RESIDENT_ASSET_V1_ID) != 0) {
    fprintf(stderr, "failed to build merkle membership verifier wire payload\n");
    buf_free(&wire);
    return -1;
  }
  aoem_exec_v2_result result = {0};
  int32_t rc = api->execute_ops_wire_v1(handle, wire.data, wire.len, &result);
  buf_free(&wire);
  if (rc != 0 || result.processed != 1 || result.success != 1 || result.total_writes != 8) {
    fprintf(stderr, "merkle membership verifier setup execute failed rc=%d\n", rc);
    return -1;
  }

  char* proof_response = NULL;
  char* public_outputs_response = NULL;
  char* proof_hex = NULL;
  uint8_t* proof_bytes = NULL;
  size_t proof_len = 0;
  byte_buf public_input = {0};
  byte_buf witness = {0};
  if (read_state_response(api, proof_key, &proof_response) != 0 ||
      read_state_response(api, public_outputs_key, &public_outputs_response) != 0 ||
      json_dup_string_field(proof_response, "proof_bytes_hex", &proof_hex) != 0 ||
      hex_to_bytes(proof_hex, &proof_bytes, &proof_len) != 0 ||
      aoem_build_merkle_membership_fixture(3u, 4u, &public_input, &witness) != 0) {
    free(proof_response);
    free(public_outputs_response);
    free(proof_hex);
    free(proof_bytes);
    buf_free(&public_input);
    buf_free(&witness);
    return -1;
  }

  int ok = verify_contract_against_inputs(
      proof_bytes,
      proof_len,
      public_input.data,
      public_input.len,
      witness.data,
      witness.len,
      AOEM_MERKLE_MEMBERSHIP_PROOF_V1_ID,
      public_outputs_response);

  byte_buf tampered_public = {0};
  byte_buf tampered_witness = {0};
  (void)buf_append(&tampered_public, public_input.data, public_input.len);
  (void)buf_append(&tampered_witness, witness.data, witness.len);

  tampered_public.data[0] ^= 0x11u;
  int root_rejected =
      verify_contract_against_inputs(
          proof_bytes,
          proof_len,
          tampered_public.data,
          tampered_public.len,
          witness.data,
          witness.len,
          AOEM_MERKLE_MEMBERSHIP_PROOF_V1_ID,
          public_outputs_response) != 0;
  tampered_public.data[0] ^= 0x11u;
  tampered_public.data[32] ^= 0x22u;
  int leaf_rejected =
      verify_contract_against_inputs(
          proof_bytes,
          proof_len,
          tampered_public.data,
          tampered_public.len,
          witness.data,
          witness.len,
          AOEM_MERKLE_MEMBERSHIP_PROOF_V1_ID,
          public_outputs_response) != 0;
  tampered_public.data[32] ^= 0x22u;
  tampered_witness.data[0] ^= 0x33u;
  int path_rejected =
      verify_contract_against_inputs(
          proof_bytes,
          proof_len,
          public_input.data,
          public_input.len,
          tampered_witness.data,
          tampered_witness.len,
          AOEM_MERKLE_MEMBERSHIP_PROOF_V1_ID,
          public_outputs_response) != 0;
  tampered_witness.data[0] ^= 0x33u;
  tampered_public.data[64] ^= 0x01u;
  int index_rejected =
      verify_contract_against_inputs(
          proof_bytes,
          proof_len,
          tampered_public.data,
          tampered_public.len,
          witness.data,
          witness.len,
          AOEM_MERKLE_MEMBERSHIP_PROOF_V1_ID,
          public_outputs_response) != 0;
  tampered_public.data[64] ^= 0x01u;
  tampered_public.data[72] ^= 0x01u;
  int depth_rejected =
      verify_contract_against_inputs(
          proof_bytes,
          proof_len,
          tampered_public.data,
          tampered_public.len,
          witness.data,
          witness.len,
          AOEM_MERKLE_MEMBERSHIP_PROOF_V1_ID,
          public_outputs_response) != 0;
  int profile_rejected =
      verify_contract_against_inputs(
          proof_bytes,
          proof_len,
          public_input.data,
          public_input.len,
          witness.data,
          witness.len,
          AOEM_FIXED_PROFILE_RESIDENT_PROOF_V1_ID,
          public_outputs_response) != 0;
  uint8_t* tampered_proof = (uint8_t*)malloc(proof_len);
  int proof_rejected = 0;
  if (tampered_proof) {
    memcpy(tampered_proof, proof_bytes, proof_len);
    if (proof_len > AOEM_PROOF_CONTRACT_V3_PREFIX_LEN + 4u) {
      tampered_proof[AOEM_PROOF_CONTRACT_V3_PREFIX_LEN + 5u] ^= 0x44u;
    }
    proof_rejected =
        verify_contract_against_inputs(
            tampered_proof,
            proof_len,
            public_input.data,
            public_input.len,
            witness.data,
            witness.len,
            AOEM_MERKLE_MEMBERSHIP_PROOF_V1_ID,
            public_outputs_response) != 0;
    free(tampered_proof);
  }

  free(proof_response);
  free(public_outputs_response);
  free(proof_hex);
  free(proof_bytes);
  buf_free(&public_input);
  buf_free(&witness);
  buf_free(&tampered_public);
  buf_free(&tampered_witness);

  return ok == 0 && root_rejected && leaf_rejected && path_rejected && index_rejected &&
                 depth_rejected && profile_rejected && proof_rejected
             ? 0
             : -1;
}

static int AOEM_MAYBE_UNUSED run_zk_merkle_membership_verifier_smoke(
    const aoem_host_api* api,
    void* handle) {
  const char* request_id = "c-host-resident-proof-verify-zk-merkle";
  const char* output_prefix = "aoem.compute.output/c-host-resident-proof-verify-zk-merkle";
  const char* proof_key =
      "aoem.compute.output/c-host-resident-proof-verify-zk-merkle/zk/proof/0/bytes";
  const char* public_outputs_key =
      "aoem.compute.output/c-host-resident-proof-verify-zk-merkle/zk/proof/0/public_outputs";

  byte_buf wire = {0};
  if (build_proof_zk_merkle_membership_batch_wire(
          &wire,
          request_id,
          output_prefix,
          1u,
          AOEM_FIXED_PROFILE_RESIDENT_ASSET_V1_ID) != 0) {
    fprintf(stderr, "failed to build zk merkle membership verifier wire payload\n");
    buf_free(&wire);
    return -1;
  }
  aoem_exec_v2_result result = {0};
  int32_t rc = api->execute_ops_wire_v1(handle, wire.data, wire.len, &result);
  buf_free(&wire);
  if (rc != 0 || result.processed != 1 || result.success != 1 || result.total_writes != 8) {
    fprintf(stderr, "zk merkle membership verifier setup execute failed rc=%d\n", rc);
    return -1;
  }

  char* proof_response = NULL;
  char* public_outputs_response = NULL;
  char* proof_hex = NULL;
  uint8_t* proof_bytes = NULL;
  size_t proof_len = 0;
  byte_buf public_input = {0};
  byte_buf witness = {0};
  if (read_state_response(api, proof_key, &proof_response) != 0 ||
      read_state_response(api, public_outputs_key, &public_outputs_response) != 0 ||
      json_dup_string_field(proof_response, "proof_bytes_hex", &proof_hex) != 0 ||
      hex_to_bytes(proof_hex, &proof_bytes, &proof_len) != 0 ||
      aoem_build_zk_merkle_membership_fixture(2u, 4u, &public_input, &witness) != 0) {
    free(proof_response);
    free(public_outputs_response);
    free(proof_hex);
    free(proof_bytes);
    buf_free(&public_input);
    buf_free(&witness);
    return -1;
  }

  int privacy_ok =
      strstr(public_outputs_response, "\"private_witness_hidden\":true") != NULL &&
      strstr(public_outputs_response, "\"leaf_hidden\":true") != NULL &&
      strstr(public_outputs_response, "\"sibling_path_hidden\":true") != NULL &&
      strstr(public_outputs_response, "\"leaf_index_hidden\":true") != NULL &&
      strstr(public_outputs_response, "\"leaf_hash\"") == NULL &&
      strstr(public_outputs_response, "\"leaf_index\"") == NULL &&
      strstr(public_outputs_response, "\"sibling_path\"") == NULL &&
      strstr(public_outputs_response, "\"path_digest\"") == NULL &&
      strstr(public_outputs_response, "\"computed_root\"") == NULL;

  int ok = verify_contract_against_inputs(
      proof_bytes,
      proof_len,
      public_input.data,
      public_input.len,
      NULL,
      0u,
      AOEM_ZK_MERKLE_MEMBERSHIP_PROOF_V1_ID,
      public_outputs_response);

  byte_buf tampered_public = {0};
  (void)buf_append(&tampered_public, public_input.data, public_input.len);
  tampered_public.data[0] ^= 0x11u;
  int root_rejected =
      verify_contract_against_inputs(
          proof_bytes,
          proof_len,
          tampered_public.data,
          tampered_public.len,
          NULL,
          0u,
          AOEM_ZK_MERKLE_MEMBERSHIP_PROOF_V1_ID,
          public_outputs_response) != 0;
  tampered_public.data[0] ^= 0x11u;
  tampered_public.data[32] ^= 0x22u;
  int commitment_rejected =
      verify_contract_against_inputs(
          proof_bytes,
          proof_len,
          tampered_public.data,
          tampered_public.len,
          NULL,
          0u,
          AOEM_ZK_MERKLE_MEMBERSHIP_PROOF_V1_ID,
          public_outputs_response) != 0;
  tampered_public.data[32] ^= 0x22u;
  tampered_public.data[64] ^= 0x33u;
  int nullifier_rejected =
      verify_contract_against_inputs(
          proof_bytes,
          proof_len,
          tampered_public.data,
          tampered_public.len,
          NULL,
          0u,
          AOEM_ZK_MERKLE_MEMBERSHIP_PROOF_V1_ID,
          public_outputs_response) != 0;
  tampered_public.data[64] ^= 0x33u;
  tampered_public.data[96] ^= 0x01u;
  int depth_rejected =
      verify_contract_against_inputs(
          proof_bytes,
          proof_len,
          tampered_public.data,
          tampered_public.len,
          NULL,
          0u,
          AOEM_ZK_MERKLE_MEMBERSHIP_PROOF_V1_ID,
          public_outputs_response) != 0;
  int profile_rejected =
      verify_contract_against_inputs(
          proof_bytes,
          proof_len,
          public_input.data,
          public_input.len,
          NULL,
          0u,
          AOEM_MERKLE_MEMBERSHIP_PROOF_V1_ID,
          public_outputs_response) != 0;
  uint8_t* tampered_proof = (uint8_t*)malloc(proof_len);
  int proof_rejected = 0;
  if (tampered_proof) {
    memcpy(tampered_proof, proof_bytes, proof_len);
    if (proof_len > AOEM_PROOF_CONTRACT_V3_PREFIX_LEN + 4u) {
      tampered_proof[AOEM_PROOF_CONTRACT_V3_PREFIX_LEN + 9u] ^= 0x44u;
    }
    proof_rejected =
        verify_contract_against_inputs(
            tampered_proof,
            proof_len,
            public_input.data,
            public_input.len,
            NULL,
            0u,
            AOEM_ZK_MERKLE_MEMBERSHIP_PROOF_V1_ID,
            public_outputs_response) != 0;
    free(tampered_proof);
  }

  free(proof_response);
  free(public_outputs_response);
  free(proof_hex);
  free(proof_bytes);
  buf_free(&public_input);
  buf_free(&witness);
  buf_free(&tampered_public);

  return ok == 0 && privacy_ok && root_rejected && commitment_rejected && nullifier_rejected &&
                 depth_rejected && profile_rejected && proof_rejected
             ? 0
             : -1;
}

#ifndef AOEM_RESIDENT_PROOF_VERIFY_NO_MAIN
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
  if (api.abi_version() != 1 || api.global_init() != 0) {
    fprintf(stderr, "AOEM verifier host initialization failed\n");
    return 2;
  }
  void* handle = api.create();
  if (!handle) {
    fprintf(stderr, "aoem_create failed\n");
    return 2;
  }
  int ok = run_external_verifier_smoke(&api, handle);
  if (ok == 0) {
    ok = run_external_batch_verifier_smoke(&api, handle, 4u);
  }
  if (ok == 0) {
    ok = run_merkle_membership_verifier_smoke(&api, handle);
  }
  if (ok == 0) {
    ok = run_zk_merkle_membership_verifier_smoke(&api, handle);
  }
  api.destroy(handle);
  if (ok != 0) {
    return 1;
  }
  printf(
      "C_HOST_RESIDENT_PROOF_VERIFY|profile=zk_merkle_membership_v1|proof_parse=ok|private_membership=ok|privacy=ok|public_input_binding=ok|public_outputs_binding=ok|verify=ok|resident_asset=ok|batch_verify=ok|tamper_root_rejected=ok|tamper_commitment_rejected=ok|tamper_nullifier_rejected=ok|tamper_depth_rejected=ok|tamper_rejected=ok\n");
  return 0;
}
#endif
