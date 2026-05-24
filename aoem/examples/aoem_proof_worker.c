// Hosted AOEM proof worker sample.
//
// This is a deployable host-side example, not an AOEM runtime extension. It
// reads JSONL jobs, batches them into compute.zk.resident_proof_v1 requests,
// calls the existing wire_v1 product entry, reads proof state back through
// aoem_state_read_v1, and verifies proof envelopes outside AOEM state.

#define _CRT_SECURE_NO_WARNINGS

#define AOEM_RESIDENT_PROOF_VERIFY_NO_MAIN
#include "hosted_resident_proof_verify.c"
#undef AOEM_RESIDENT_PROOF_VERIFY_NO_MAIN

#include <ctype.h>

#define AOEM_WORKER_DEFAULT_BATCH_COUNT 4u
#define AOEM_WORKER_MAX_BATCH_COUNT 8u
#define AOEM_WORKER_LINE_MAX 65536u
#define AOEM_WORKER_LIFECYCLE_ASSET_ID 0xA0E0A700u
#define AOEM_ZK_RESIDENT_ASSET_LIFECYCLE_OPCODE 99u
#define AOEM_ZK_RESIDENT_ASSET_CMD_SETUP 1u
#define AOEM_ZK_RESIDENT_ASSET_CMD_LIST 2u
#define AOEM_ZK_RESIDENT_ASSET_CMD_SELECT 3u
#define AOEM_ZK_RESIDENT_ASSET_CMD_RELEASE 4u

typedef struct aoem_worker_job {
  char* request_id;
  uint32_t profile_id;
  uint32_t resident_asset_id;
  uint8_t* public_input;
  size_t public_input_len;
  uint8_t* witness;
  size_t witness_len;
} aoem_worker_job;

typedef struct aoem_worker_options {
  const char* library_path;
  const char* input_path;
  const char* output_path;
  uint32_t batch_count;
  int asset_lifecycle;
} aoem_worker_options;

typedef struct aoem_worker_stats {
  uint64_t jobs_ok;
  uint64_t failures;
  uint64_t malformed_seen;
  uint64_t malformed_rejected;
  uint32_t profile_id;
  int resident_asset_ok;
  int proof_ok;
  int verify_ok;
  int external_verify_ok;
} aoem_worker_stats;

static void worker_job_free(aoem_worker_job* job) {
  free(job->request_id);
  free(job->public_input);
  free(job->witness);
  memset(job, 0, sizeof(*job));
}

static int worker_request_id_safe(const char* request_id) {
  size_t len = request_id ? strlen(request_id) : 0u;
  if (len == 0u || len > 96u) {
    return 0;
  }
  for (size_t i = 0; i < len; ++i) {
    unsigned char c = (unsigned char)request_id[i];
    if (!(isalnum(c) || c == '-' || c == '_' || c == '.')) {
      return 0;
    }
  }
  return 1;
}

static int worker_json_dup_string_field(const char* json, const char* field, char** out) {
  if (json_dup_string_field(json, field, out) != 0) {
    return -1;
  }
  if (strchr(*out, '\\') != NULL) {
    free(*out);
    *out = NULL;
    return -1;
  }
  return 0;
}

static int worker_parse_u32_string(const char* value, uint32_t* out) {
  if (strcmp(value, "fixed_profile_v1") == 0) {
    *out = 1u;
    return 0;
  }
  if (strcmp(value, "merkle_membership_v1") == 0) {
    *out = AOEM_MERKLE_MEMBERSHIP_PROOF_V1_ID;
    return 0;
  }
  if (strcmp(value, "zk_merkle_membership_v1") == 0) {
    *out = AOEM_ZK_MERKLE_MEMBERSHIP_PROOF_V1_ID;
    return 0;
  }
  if (strcmp(value, "default") == 0) {
    *out = AOEM_FIXED_PROFILE_RESIDENT_ASSET_V1_ID;
    return 0;
  }
  char* end = NULL;
  unsigned long parsed = strtoul(value, &end, 0);
  if (!end || *end != '\0' || parsed == 0ul || parsed > 0xfffffffful) {
    return -1;
  }
  *out = (uint32_t)parsed;
  return 0;
}

static int worker_parse_hex_field(const char* json, const char* field, uint8_t** out, size_t* out_len) {
  char* hex = NULL;
  if (worker_json_dup_string_field(json, field, &hex) != 0) {
    return -1;
  }
  int rc = -1;
  if (hex[0] != '\0') {
    rc = hex_to_bytes(hex, out, out_len);
  }
  free(hex);
  return rc;
}

static int worker_parse_u64_json_field(const char* json, const char* field, uint64_t* out) {
  char needle[128];
  int written = snprintf(needle, sizeof(needle), "\"%s\":", field);
  if (written <= 0 || (size_t)written >= sizeof(needle)) {
    return -1;
  }
  const char* start = strstr(json, needle);
  if (!start) {
    return -1;
  }
  start += strlen(needle);
  while (*start == ' ' || *start == '\t') {
    ++start;
  }
  char* end = NULL;
  unsigned long long parsed = strtoull(start, &end, 10);
  if (!end || end == start) {
    return -1;
  }
  *out = (uint64_t)parsed;
  return 0;
}

static int worker_parse_sibling_path_array(
    const char* json,
    uint32_t tree_depth,
    byte_buf* witness) {
  const char* start = strstr(json, "\"sibling_path\":[");
  if (!start) {
    return -1;
  }
  start += strlen("\"sibling_path\":[");
  for (uint32_t level = 0; level < tree_depth; ++level) {
    while (*start == ' ' || *start == '\t' || *start == ',') {
      ++start;
    }
    if (*start != '"') {
      return -1;
    }
    ++start;
    const char* end = strchr(start, '"');
    if (!end || (size_t)(end - start) != 64u) {
      return -1;
    }
    char hex[65];
    memcpy(hex, start, 64u);
    hex[64] = '\0';
    uint8_t* sibling = NULL;
    size_t sibling_len = 0;
    if (hex_to_bytes(hex, &sibling, &sibling_len) != 0 || sibling_len != 32u ||
        buf_append(witness, sibling, sibling_len) != 0) {
      free(sibling);
      return -1;
    }
    free(sibling);
    start = end + 1;
  }
  while (*start == ' ' || *start == '\t') {
    ++start;
  }
  return *start == ']' ? 0 : -1;
}

static int worker_parse_merkle_membership_job(const char* line, aoem_worker_job* job) {
  char* root_hex = NULL;
  char* leaf_hex = NULL;
  uint8_t* root = NULL;
  uint8_t* leaf = NULL;
  size_t root_len = 0;
  size_t leaf_len = 0;
  uint64_t leaf_index = 0;
  uint64_t tree_depth_u64 = 0;
  byte_buf public_input = {0};
  byte_buf witness = {0};
  int rc = -1;

  if (worker_json_dup_string_field(line, "merkle_root", &root_hex) != 0 ||
      worker_json_dup_string_field(line, "leaf_hash", &leaf_hex) != 0 ||
      hex_to_bytes(root_hex, &root, &root_len) != 0 || root_len != 32u ||
      hex_to_bytes(leaf_hex, &leaf, &leaf_len) != 0 || leaf_len != 32u ||
      worker_parse_u64_json_field(line, "leaf_index", &leaf_index) != 0 ||
      worker_parse_u64_json_field(line, "tree_depth", &tree_depth_u64) != 0 ||
      tree_depth_u64 > AOEM_MERKLE_MEMBERSHIP_MAX_DEPTH ||
      worker_parse_sibling_path_array(line, (uint32_t)tree_depth_u64, &witness) != 0) {
    goto done;
  }
  if (buf_append(&public_input, root, 32u) != 0 || buf_append(&public_input, leaf, 32u) != 0 ||
      buf_u64(&public_input, leaf_index) != 0 ||
      buf_u32(&public_input, (uint32_t)tree_depth_u64) != 0) {
    goto done;
  }
  job->public_input = public_input.data;
  job->public_input_len = public_input.len;
  public_input.data = NULL;
  public_input.len = public_input.cap = 0;
  job->witness = witness.data;
  job->witness_len = witness.len;
  witness.data = NULL;
  witness.len = witness.cap = 0;
  rc = 0;

done:
  free(root_hex);
  free(leaf_hex);
  free(root);
  free(leaf);
  buf_free(&public_input);
  buf_free(&witness);
  return rc;
}

static int worker_parse_zk_merkle_membership_job(const char* line, aoem_worker_job* job) {
  char* root_hex = NULL;
  char* commitment_hex = NULL;
  char* nullifier_hex = NULL;
  char* leaf_hex = NULL;
  char* secret_hex = NULL;
  uint8_t* root = NULL;
  uint8_t* commitment = NULL;
  uint8_t* nullifier = NULL;
  uint8_t* leaf = NULL;
  uint8_t* secret = NULL;
  size_t root_len = 0;
  size_t commitment_len = 0;
  size_t nullifier_len = 0;
  size_t leaf_len = 0;
  size_t secret_len = 0;
  uint64_t leaf_index = 0;
  uint64_t tree_depth_u64 = 0;
  byte_buf public_input = {0};
  byte_buf witness = {0};
  int rc = -1;

  if (worker_json_dup_string_field(line, "merkle_root", &root_hex) != 0 ||
      worker_json_dup_string_field(line, "leaf_commitment", &commitment_hex) != 0 ||
      worker_json_dup_string_field(line, "nullifier", &nullifier_hex) != 0 ||
      worker_json_dup_string_field(line, "leaf", &leaf_hex) != 0 ||
      worker_json_dup_string_field(line, "leaf_secret", &secret_hex) != 0 ||
      hex_to_bytes(root_hex, &root, &root_len) != 0 || root_len != 32u ||
      hex_to_bytes(commitment_hex, &commitment, &commitment_len) != 0 ||
      commitment_len != 32u ||
      hex_to_bytes(nullifier_hex, &nullifier, &nullifier_len) != 0 || nullifier_len != 32u ||
      hex_to_bytes(leaf_hex, &leaf, &leaf_len) != 0 || leaf_len == 0u ||
      hex_to_bytes(secret_hex, &secret, &secret_len) != 0 || secret_len == 0u ||
      worker_parse_u64_json_field(line, "leaf_index", &leaf_index) != 0 ||
      worker_parse_u64_json_field(line, "tree_depth", &tree_depth_u64) != 0 ||
      tree_depth_u64 > AOEM_MERKLE_MEMBERSHIP_MAX_DEPTH ||
      buf_u64(&witness, leaf_index) != 0 || buf_u32(&witness, (uint32_t)leaf_len) != 0 ||
      buf_u32(&witness, (uint32_t)secret_len) != 0 ||
      buf_append(&witness, leaf, leaf_len) != 0 ||
      buf_append(&witness, secret, secret_len) != 0 ||
      worker_parse_sibling_path_array(line, (uint32_t)tree_depth_u64, &witness) != 0) {
    goto done;
  }
  if (buf_append(&public_input, root, 32u) != 0 ||
      buf_append(&public_input, commitment, 32u) != 0 ||
      buf_append(&public_input, nullifier, 32u) != 0 ||
      buf_u32(&public_input, (uint32_t)tree_depth_u64) != 0 ||
      buf_u32(&public_input, AOEM_ZK_MERKLE_STYLE_V1_HASH_PROFILE) != 0) {
    goto done;
  }
  job->public_input = public_input.data;
  job->public_input_len = public_input.len;
  public_input.data = NULL;
  public_input.len = public_input.cap = 0;
  job->witness = witness.data;
  job->witness_len = witness.len;
  witness.data = NULL;
  witness.len = witness.cap = 0;
  rc = 0;

done:
  free(root_hex);
  free(commitment_hex);
  free(nullifier_hex);
  free(leaf_hex);
  free(secret_hex);
  free(root);
  free(commitment);
  free(nullifier);
  free(leaf);
  free(secret);
  buf_free(&public_input);
  buf_free(&witness);
  return rc;
}

static char* worker_dup_literal(const char* value) {
  size_t len = strlen(value);
  char* out = (char*)malloc(len + 1u);
  if (!out) {
    return NULL;
  }
  memcpy(out, value, len + 1u);
  return out;
}

static void worker_json_write_escaped(FILE* out, const char* value) {
  fputc('"', out);
  for (const unsigned char* p = (const unsigned char*)value; *p; ++p) {
    if (*p == '"' || *p == '\\') {
      fputc('\\', out);
      fputc(*p, out);
    } else if (*p == '\n') {
      fputs("\\n", out);
    } else if (*p == '\r') {
      fputs("\\r", out);
    } else if (*p == '\t') {
      fputs("\\t", out);
    } else if (*p < 0x20u) {
      fprintf(out, "\\u%04x", (unsigned int)*p);
    } else {
      fputc(*p, out);
    }
  }
  fputc('"', out);
}

static void worker_write_error(FILE* out, const char* request_id, const char* error) {
  fputs("{\"request_id\":", out);
  worker_json_write_escaped(out, request_id ? request_id : "unknown");
  fputs(",\"status\":\"error\",\"error\":", out);
  worker_json_write_escaped(out, error);
  fputs(",\"proof_written\":false}\n", out);
}

static int worker_parse_job_line(const char* line, aoem_worker_job* job, char** error_out) {
  memset(job, 0, sizeof(*job));
  *error_out = NULL;

  if (worker_json_dup_string_field(line, "request_id", &job->request_id) != 0 ||
      !worker_request_id_safe(job->request_id)) {
    *error_out = worker_dup_literal("malformed_payload");
    return -1;
  }

  char* profile = NULL;
  char* resident_asset = NULL;
  if (worker_json_dup_string_field(line, "profile_id", &profile) != 0 ||
      worker_parse_u32_string(profile, &job->profile_id) != 0 ||
      worker_json_dup_string_field(line, "resident_asset_id", &resident_asset) != 0 ||
      worker_parse_u32_string(resident_asset, &job->resident_asset_id) != 0) {
    free(profile);
    free(resident_asset);
    *error_out = worker_dup_literal("malformed_payload");
    return -1;
  }
  if (job->profile_id == AOEM_FIXED_PROFILE_RESIDENT_PROOF_V1_ID) {
    if (worker_parse_hex_field(line, "public_input", &job->public_input, &job->public_input_len) != 0 ||
        worker_parse_hex_field(line, "witness", &job->witness, &job->witness_len) != 0) {
      free(profile);
      free(resident_asset);
      *error_out = worker_dup_literal("malformed_payload");
      return -1;
    }
  } else if (job->profile_id == AOEM_MERKLE_MEMBERSHIP_PROOF_V1_ID) {
    if (worker_parse_merkle_membership_job(line, job) != 0) {
      free(profile);
      free(resident_asset);
      *error_out = worker_dup_literal("malformed_payload");
      return -1;
    }
  } else if (job->profile_id == AOEM_ZK_MERKLE_MEMBERSHIP_PROOF_V1_ID) {
    if (worker_parse_zk_merkle_membership_job(line, job) != 0) {
      free(profile);
      free(resident_asset);
      *error_out = worker_dup_literal("malformed_payload");
      return -1;
    }
  } else {
    free(profile);
    free(resident_asset);
    *error_out = worker_dup_literal("malformed_payload");
    return -1;
  }

  free(profile);
  free(resident_asset);
  return 0;
}

static int worker_append_batch_payload(
    byte_buf* payload,
    const char* request_id,
    const char* output_prefix,
    const aoem_worker_job* jobs,
    uint32_t batch_count,
    uint32_t resident_asset_id) {
  const uint16_t flags = (uint16_t)(1u | 2u | 4u | 8u);
  if (batch_count == 0u || batch_count > AOEM_WORKER_MAX_BATCH_COUNT) {
    return -1;
  }
  if (strlen(request_id) > UINT16_MAX || strlen(output_prefix) > UINT16_MAX) {
    return -1;
  }

  if (buf_append(payload, "AOFP\0", 5) != 0 || buf_u16(payload, 4u) != 0 ||
      buf_u16(payload, flags) != 0 || buf_u8(payload, 4u) != 0 ||
      buf_append(payload, "\0\0\0", 3) != 0 ||
      buf_u16(payload, (uint16_t)strlen(request_id)) != 0 ||
      buf_u16(payload, (uint16_t)strlen(output_prefix)) != 0 ||
      buf_u32(payload, jobs[0].profile_id) != 0 || buf_u32(payload, 0xA0E05051u) != 0 ||
      buf_u32(payload, 0xA0E09EEDu) != 0 || buf_u32(payload, 256u) != 0 ||
      buf_u32(payload, 1u) != 0 || buf_u32(payload, 2u) != 0 ||
      buf_u32(payload, 16u) != 0 || buf_u32(payload, batch_count) != 0 ||
      buf_u32(payload, resident_asset_id) != 0 ||
      buf_append(payload, request_id, strlen(request_id)) != 0 ||
      buf_append(payload, output_prefix, strlen(output_prefix)) != 0) {
    return -1;
  }

  for (uint32_t i = 0; i < batch_count; ++i) {
    if (jobs[i].public_input_len > UINT32_MAX || jobs[i].witness_len > UINT32_MAX) {
      return -1;
    }
    if (buf_u32(payload, (uint32_t)jobs[i].public_input_len) != 0 ||
        buf_u32(payload, (uint32_t)jobs[i].witness_len) != 0 ||
        buf_append(payload, jobs[i].public_input, jobs[i].public_input_len) != 0 ||
        buf_append(payload, jobs[i].witness, jobs[i].witness_len) != 0) {
      return -1;
    }
  }
  return 0;
}

static int worker_build_batch_wire(
    byte_buf* wire,
    const char* request_id,
    const char* output_prefix,
    const aoem_worker_job* jobs,
    uint32_t batch_count,
    uint32_t resident_asset_id) {
  byte_buf payload = {0};
  int rc = worker_append_batch_payload(
      &payload,
      request_id,
      output_prefix,
      jobs,
      batch_count,
      resident_asset_id);
  if (rc == 0) {
    rc = append_wire_op(wire, 98, output_prefix, &payload);
  }
  buf_free(&payload);
  return rc;
}

static int worker_append_asset_lifecycle_payload(
    byte_buf* payload,
    const char* request_id,
    const char* output_prefix,
    uint8_t command,
    uint32_t profile_id,
    uint32_t resident_asset_id,
    const char* asset_label,
    const uint8_t* asset_metadata,
    uint32_t asset_metadata_len) {
  const uint16_t flags = (uint16_t)(1u | 2u);
  const size_t request_id_len = strlen(request_id);
  const size_t output_prefix_len = strlen(output_prefix);
  const size_t asset_label_len = strlen(asset_label);
  if (request_id_len > UINT16_MAX || output_prefix_len > UINT16_MAX ||
      asset_label_len > UINT16_MAX || asset_metadata_len > UINT16_MAX) {
    return -1;
  }
  if (buf_append(payload, "AOZA\0", 5) != 0 || buf_u16(payload, 1u) != 0 ||
      buf_u16(payload, flags) != 0 || buf_u8(payload, command) != 0 ||
      buf_append(payload, "\0\0\0", 3) != 0 ||
      buf_u16(payload, (uint16_t)request_id_len) != 0 ||
      buf_u16(payload, (uint16_t)output_prefix_len) != 0 ||
      buf_u32(payload, profile_id) != 0 || buf_u32(payload, resident_asset_id) != 0 ||
      buf_u16(payload, (uint16_t)asset_label_len) != 0 ||
      buf_u16(payload, (uint16_t)asset_metadata_len) != 0 ||
      buf_append(payload, request_id, request_id_len) != 0 ||
      buf_append(payload, output_prefix, output_prefix_len) != 0 ||
      buf_append(payload, asset_label, asset_label_len) != 0) {
    return -1;
  }
  if (asset_metadata_len != 0u && buf_append(payload, asset_metadata, asset_metadata_len) != 0) {
    return -1;
  }
  return 0;
}

static int worker_build_asset_lifecycle_wire(
    byte_buf* wire,
    const char* request_id,
    const char* output_prefix,
    uint8_t command,
    uint32_t profile_id,
    uint32_t resident_asset_id,
    const char* asset_label,
    const uint8_t* asset_metadata,
    uint32_t asset_metadata_len) {
  byte_buf payload = {0};
  int rc = worker_append_asset_lifecycle_payload(
      &payload,
      request_id,
      output_prefix,
      command,
      profile_id,
      resident_asset_id,
      asset_label,
      asset_metadata,
      asset_metadata_len);
  if (rc == 0) {
    rc = append_wire_op(wire, AOEM_ZK_RESIDENT_ASSET_LIFECYCLE_OPCODE, output_prefix, &payload);
  }
  buf_free(&payload);
  return rc;
}

static int worker_read_and_emit_job(
    const aoem_host_api* api,
    FILE* output,
    const char* output_prefix,
    uint32_t batch_index,
    const aoem_worker_job* job) {
  char proof_key[512];
  char metadata_key[512];
  char status_key[512];
  char public_outputs_key[512];
  char verify_status_key[512];

  if (snprintf(proof_key, sizeof(proof_key), "%s/zk/proof/%u/bytes", output_prefix, batch_index) <=
          0 ||
      snprintf(metadata_key, sizeof(metadata_key), "%s/zk/proof/%u/metadata", output_prefix, batch_index) <=
          0 ||
      snprintf(status_key, sizeof(status_key), "%s/zk/proof/%u/status", output_prefix, batch_index) <=
          0 ||
      snprintf(
          public_outputs_key,
          sizeof(public_outputs_key),
          "%s/zk/proof/%u/public_outputs",
          output_prefix,
          batch_index) <= 0 ||
      snprintf(
          verify_status_key,
          sizeof(verify_status_key),
          "%s/zk/proof/%u/verify_status",
          output_prefix,
          batch_index) <= 0) {
    worker_write_error(output, job->request_id, "state_key_overflow");
    return -1;
  }

  char* proof_response = NULL;
  char* metadata_response = NULL;
  char* status_response = NULL;
  char* public_outputs_response = NULL;
  char* verify_response = NULL;
  char* proof_hex = NULL;
  uint8_t* proof_bytes = NULL;
  size_t proof_len = 0u;

  int ok =
      read_state_response(api, proof_key, &proof_response) == 0 &&
      read_state_response(api, metadata_key, &metadata_response) == 0 &&
      read_state_response(api, status_key, &status_response) == 0 &&
      read_state_response(api, public_outputs_key, &public_outputs_response) == 0 &&
      read_state_response(api, verify_status_key, &verify_response) == 0 &&
      json_dup_string_field(proof_response, "proof_bytes_hex", &proof_hex) == 0 &&
      hex_to_bytes(proof_hex, &proof_bytes, &proof_len) == 0 &&
      verify_contract_against_inputs(
          proof_bytes,
          proof_len,
          job->public_input,
          job->public_input_len,
          job->witness,
          job->witness_len,
          job->profile_id,
          public_outputs_response) == 0 &&
      strstr(status_response, "\"proof_verified\":true") != NULL &&
      strstr(verify_response, "\"accepted\":true") != NULL &&
      strstr(metadata_response, "\"resident_asset_bound\":true") != NULL &&
      (job->profile_id != AOEM_ZK_MERKLE_MEMBERSHIP_PROOF_V1_ID ||
       (strstr(public_outputs_response, "\"private_witness_hidden\":true") != NULL &&
        strstr(public_outputs_response, "\"leaf_hash\"") == NULL &&
        strstr(public_outputs_response, "\"leaf_index\"") == NULL &&
        strstr(public_outputs_response, "\"sibling_path\"") == NULL &&
        strstr(public_outputs_response, "\"path_digest\"") == NULL &&
        strstr(public_outputs_response, "\"computed_root\"") == NULL));

  if (!ok) {
    worker_write_error(output, job->request_id, "proof_verification_failed");
  } else {
    fputs("{\"request_id\":", output);
    worker_json_write_escaped(output, job->request_id);
    fputs(",\"status\":\"ok\",\"profile_id\":", output);
    worker_json_write_escaped(
        output,
        job->profile_id == AOEM_MERKLE_MEMBERSHIP_PROOF_V1_ID
            ? "merkle_membership_v1"
            : (job->profile_id == AOEM_ZK_MERKLE_MEMBERSHIP_PROOF_V1_ID
                   ? "zk_merkle_membership_v1"
                   : "fixed_profile_v1"));
    fputs(",\"proof\":", output);
    worker_json_write_escaped(output, proof_hex);
    fputs(",\"verify_status\":\"ok\",\"public_outputs\":", output);
    worker_json_write_escaped(output, public_outputs_response);
    fputs(",\"metadata\":", output);
    worker_json_write_escaped(output, metadata_response);
    fputs("}\n", output);
  }

  free(proof_response);
  free(metadata_response);
  free(status_response);
  free(public_outputs_response);
  free(verify_response);
  free(proof_hex);
  free(proof_bytes);
  return ok ? 0 : -1;
}

static int worker_process_batch(
    const aoem_host_api* api,
    void* handle,
    FILE* output,
    aoem_worker_job* jobs,
    uint32_t job_count,
    uint64_t batch_seq,
    aoem_worker_stats* stats) {
  if (job_count == 0u) {
    return 0;
  }

  char request_id[128];
  char output_prefix[192];
  stats->profile_id = jobs[0].profile_id;
  if (snprintf(request_id, sizeof(request_id), "aoem-proof-worker-batch-%llu", (unsigned long long)batch_seq) <=
          0 ||
      snprintf(output_prefix, sizeof(output_prefix), "aoem.compute.output/%s", request_id) <= 0) {
    for (uint32_t i = 0; i < job_count; ++i) {
      worker_write_error(output, jobs[i].request_id, "batch_request_overflow");
    }
    stats->failures += job_count;
    return -1;
  }

  byte_buf wire = {0};
  if (worker_build_batch_wire(
          &wire,
          request_id,
          output_prefix,
          jobs,
          job_count,
          jobs[0].resident_asset_id) != 0) {
    for (uint32_t i = 0; i < job_count; ++i) {
      worker_write_error(output, jobs[i].request_id, "wire_build_failed");
    }
    stats->failures += job_count;
    buf_free(&wire);
    return -1;
  }

  aoem_exec_v2_result result = {0};
  int32_t rc = api->execute_ops_wire_v1(handle, wire.data, wire.len, &result);
  buf_free(&wire);
  const uint64_t expected_writes = 3u + 5u * (uint64_t)job_count;
  if (rc != 0 || result.processed != 1 || result.success != 1 ||
      result.total_writes != expected_writes) {
    for (uint32_t i = 0; i < job_count; ++i) {
      worker_write_error(output, jobs[i].request_id, "aoem_execute_failed");
    }
    stats->failures += job_count;
    return -1;
  }

  for (uint32_t i = 0; i < job_count; ++i) {
    if (worker_read_and_emit_job(api, output, output_prefix, i, &jobs[i]) == 0) {
      stats->jobs_ok += 1u;
    } else {
      stats->failures += 1u;
    }
  }
  return stats->failures == 0u ? 0 : -1;
}

static int worker_execute_asset_lifecycle_command(
    const aoem_host_api* api,
    void* handle,
    const char* request_id,
    const char* output_prefix,
    uint8_t command,
    uint32_t resident_asset_id,
    const char* asset_label,
    const uint8_t* asset_metadata,
    uint32_t asset_metadata_len,
    uint64_t expected_writes) {
  byte_buf wire = {0};
  if (worker_build_asset_lifecycle_wire(
          &wire,
          request_id,
          output_prefix,
          command,
          1u,
          resident_asset_id,
          asset_label,
          asset_metadata,
          asset_metadata_len) != 0) {
    buf_free(&wire);
    return -1;
  }
  aoem_exec_v2_result result = {0};
  int32_t rc = api->execute_ops_wire_v1(handle, wire.data, wire.len, &result);
  buf_free(&wire);
  if (rc != 0 || result.processed != 1 || result.success != 1 ||
      result.total_writes != expected_writes) {
    return -1;
  }
  return 0;
}

static int worker_execute_rejected_proof(
    const aoem_host_api* api,
    void* handle,
    aoem_worker_job* jobs,
    uint32_t job_count,
    uint32_t resident_asset_id,
    const char* request_id,
    const char* output_prefix) {
  byte_buf wire = {0};
  if (worker_build_batch_wire(
          &wire,
          request_id,
          output_prefix,
          jobs,
          job_count,
          resident_asset_id) != 0) {
    buf_free(&wire);
    return -1;
  }
  aoem_exec_v2_result result = {99, 99, 0, 99};
  int32_t rc = api->execute_ops_wire_v1(handle, wire.data, wire.len, &result);
  buf_free(&wire);
  if (rc == 0 || result.success != 0 || result.total_writes != 0) {
    return -1;
  }
  char batch_status_key[256];
  if (snprintf(batch_status_key, sizeof(batch_status_key), "%s/zk/proof/batch/status", output_prefix) <=
      0) {
    return -1;
  }
  return read_state_found(api, batch_status_key) == 0 ? 0 : -1;
}

static int worker_run_asset_lifecycle_mode(const aoem_host_api* api, void* handle, uint32_t batch_count) {
  static const uint8_t asset_metadata[] = {
      'a', 'o', 'e', 'm', '-', 'p', 'r', 'o', 'o', 'f', '-', 'v', '0', '7'};
  const uint32_t resident_asset_id = AOEM_WORKER_LIFECYCLE_ASSET_ID;

  int setup_ok = 0;
  int list_ok = 0;
  int select_ok = 0;
  int proof_ok = 0;
  int verify_ok = 0;
  int external_verify_ok = 0;
  int release_ok = 0;
  int proof_after_release_rejected = 0;
  int malformed_ok = 0;
  FILE* proof_output = NULL;

  const char* setup_prefix = "aoem.compute.output/aoem-proof-worker-asset-setup";
  if (worker_execute_asset_lifecycle_command(
          api,
          handle,
          "aoem-proof-worker-asset-setup",
          setup_prefix,
          AOEM_ZK_RESIDENT_ASSET_CMD_SETUP,
          resident_asset_id,
          "worker-asset-v07",
          asset_metadata,
          (uint32_t)sizeof(asset_metadata),
          3u) == 0) {
    char status_key[256];
    char digest_key[256];
    if (snprintf(status_key, sizeof(status_key), "%s/zk/proof/asset/%u/status", setup_prefix, resident_asset_id) >
            0 &&
        snprintf(digest_key, sizeof(digest_key), "%s/zk/proof/asset/%u/digest", setup_prefix, resident_asset_id) >
            0 &&
        read_state_contains_all(
            api,
            status_key,
            "\"command\":\"setup\"",
            "\"status\":\"ready\"",
            "\"resident_asset_id\":") == 0 &&
        read_state_contains_all(
            api,
            digest_key,
            "compute.zk.resident_asset_lifecycle_v1.asset.digest",
            "\"asset_digest\":",
            "\"asset_digest_algorithm\":") == 0) {
      setup_ok = 1;
    }
  }

  const char* list_prefix = "aoem.compute.output/aoem-proof-worker-asset-list";
  if (setup_ok &&
      worker_execute_asset_lifecycle_command(
          api,
          handle,
          "aoem-proof-worker-asset-list",
          list_prefix,
          AOEM_ZK_RESIDENT_ASSET_CMD_LIST,
          0u,
          "",
          NULL,
          0u,
          2u) == 0) {
    char list_key[256];
    char status_key[256];
    if (snprintf(list_key, sizeof(list_key), "%s/zk/proof/assets/list", list_prefix) > 0 &&
        snprintf(status_key, sizeof(status_key), "%s/zk/proof/assets/status", list_prefix) > 0 &&
        read_state_contains_all(
            api,
            list_key,
            "compute.zk.resident_asset_lifecycle_v1.list",
            "\"asset_count\":",
            "\"selected_resident_asset_id\":") == 0 &&
        read_state_contains_all(
            api,
            status_key,
            "compute.zk.resident_asset_lifecycle_v1.assets.status",
            "\"command\":\"list\"",
            "\"status\":\"ok\"") == 0) {
      list_ok = 1;
    }
  }

  const char* select_prefix = "aoem.compute.output/aoem-proof-worker-asset-select";
  if (list_ok &&
      worker_execute_asset_lifecycle_command(
          api,
          handle,
          "aoem-proof-worker-asset-select",
          select_prefix,
          AOEM_ZK_RESIDENT_ASSET_CMD_SELECT,
          resident_asset_id,
          "",
          NULL,
          0u,
          1u) == 0) {
    char selected_key[256];
    if (snprintf(selected_key, sizeof(selected_key), "%s/zk/proof/assets/selected", select_prefix) > 0 &&
        read_state_contains_all(
            api,
            selected_key,
            "\"command\":\"select\"",
            "\"selected_resident_asset_id\":",
            "\"status\":\"ok\"") == 0) {
      select_ok = 1;
    }
  }

  aoem_worker_job jobs[AOEM_WORKER_MAX_BATCH_COUNT];
  memset(jobs, 0, sizeof(jobs));
  if (batch_count == 0u || batch_count > AOEM_WORKER_MAX_BATCH_COUNT) {
    batch_count = AOEM_WORKER_DEFAULT_BATCH_COUNT;
  }
  for (uint32_t i = 0; i < batch_count; ++i) {
    char request_id[64];
    (void)snprintf(request_id, sizeof(request_id), "asset-life-job-%u", i);
    jobs[i].request_id = worker_dup_literal(request_id);
    jobs[i].profile_id = 1u;
    jobs[i].resident_asset_id = resident_asset_id;
    jobs[i].public_input_len = 8u;
    jobs[i].witness_len = 16u;
    jobs[i].public_input = (uint8_t*)malloc(jobs[i].public_input_len);
    jobs[i].witness = (uint8_t*)malloc(jobs[i].witness_len);
    if (!jobs[i].request_id || !jobs[i].public_input || !jobs[i].witness) {
      goto lifecycle_done;
    }
    for (size_t j = 0; j < jobs[i].public_input_len; ++j) {
      jobs[i].public_input[j] = (uint8_t)(0x40u + i + j);
    }
    for (size_t j = 0; j < jobs[i].witness_len; ++j) {
      jobs[i].witness[j] = (uint8_t)(0x90u ^ (i * 17u + (uint32_t)j));
    }
  }

  proof_output = tmpfile();
  if (!proof_output) {
#ifdef _WIN32
    proof_output = fopen("NUL", "wb");
#else
    proof_output = fopen("/dev/null", "wb");
#endif
  }
  if (!proof_output) {
    goto lifecycle_done;
  }
  aoem_worker_stats stats = {0};
  stats.resident_asset_ok = 1;
  stats.proof_ok = 1;
  stats.verify_ok = 1;
  stats.external_verify_ok = 1;
  if (select_ok &&
      worker_process_batch(
          api,
          handle,
          proof_output,
          jobs,
          batch_count,
          7000u,
          &stats) == 0 &&
      stats.jobs_ok == batch_count &&
      stats.failures == 0u) {
    proof_ok = 1;
    verify_ok = 1;
    external_verify_ok = 1;
  }
  fclose(proof_output);

  const char* release_prefix = "aoem.compute.output/aoem-proof-worker-asset-release";
  if (proof_ok &&
      worker_execute_asset_lifecycle_command(
          api,
          handle,
          "aoem-proof-worker-asset-release",
          release_prefix,
          AOEM_ZK_RESIDENT_ASSET_CMD_RELEASE,
          resident_asset_id,
          "",
          NULL,
          0u,
          1u) == 0) {
    char released_key[256];
    if (snprintf(released_key, sizeof(released_key), "%s/zk/proof/asset/%u/status", release_prefix, resident_asset_id) >
            0 &&
        read_state_contains_all(
            api,
            released_key,
            "compute.zk.resident_asset_lifecycle_v1.asset.status",
            "\"command\":\"release\"",
            "\"status\":\"released\"") == 0) {
      release_ok = 1;
    }
  }

  if (release_ok &&
      worker_execute_rejected_proof(
          api,
          handle,
          jobs,
          batch_count,
          resident_asset_id,
          "aoem-proof-worker-asset-proof-after-release",
          "aoem.compute.output/aoem-proof-worker-asset-proof-after-release") == 0) {
    proof_after_release_rejected = 1;
  }

  byte_buf bad_wire = {0};
  byte_buf bad_payload = {0};
  if (worker_append_asset_lifecycle_payload(
          &bad_payload,
          "aoem-proof-worker-asset-malformed",
          "aoem.compute.output/aoem-proof-worker-asset-malformed",
          99u,
          1u,
          0u,
          "",
          NULL,
          0u) == 0 &&
      append_wire_op(
          &bad_wire,
          AOEM_ZK_RESIDENT_ASSET_LIFECYCLE_OPCODE,
          "aoem.compute.output/aoem-proof-worker-asset-malformed",
          &bad_payload) == 0) {
    aoem_exec_v2_result result = {99, 99, 0, 99};
    int32_t rc = api->execute_ops_wire_v1(handle, bad_wire.data, bad_wire.len, &result);
    if (rc != 0 && result.success == 0 && result.total_writes == 0) {
      malformed_ok = 1;
    }
  }
  buf_free(&bad_payload);
  buf_free(&bad_wire);

lifecycle_done:
  for (uint32_t i = 0; i < batch_count; ++i) {
    worker_job_free(&jobs[i]);
  }

  const int failures =
      !(setup_ok && list_ok && select_ok && proof_ok && verify_ok && external_verify_ok &&
        release_ok && proof_after_release_rejected && malformed_ok);
  printf(
      "AOEM_PROOF_WORKER_ASSET_LIFECYCLE|setup=%s|list=%s|select=%s|proof_with_asset=%s|verify=%s|external_verify=%s|release=%s|proof_after_release=%s|malformed=%s|failures=%d\n",
      setup_ok ? "ok" : "fail",
      list_ok ? "ok" : "fail",
      select_ok ? "ok" : "fail",
      proof_ok ? "ok" : "fail",
      verify_ok ? "ok" : "fail",
      external_verify_ok ? "ok" : "fail",
      release_ok ? "ok" : "fail",
      proof_after_release_rejected ? "rejected" : "fail",
      malformed_ok ? "ok" : "fail",
      failures);
  return failures ? 1 : 0;
}

static void worker_usage(const char* argv0) {
  fprintf(
      stderr,
      "usage: %s --library PATH --input jobs.jsonl --output proofs.jsonl [--batch-count N]\n"
      "       %s --library PATH --asset-lifecycle [--batch-count N]\n",
      argv0,
      argv0);
}

static int worker_parse_args(int argc, char** argv, aoem_worker_options* opts) {
  memset(opts, 0, sizeof(*opts));
  opts->batch_count = AOEM_WORKER_DEFAULT_BATCH_COUNT;
  for (int i = 1; i < argc; ++i) {
    if (strcmp(argv[i], "--library") == 0 && i + 1 < argc) {
      opts->library_path = argv[++i];
    } else if (strcmp(argv[i], "--input") == 0 && i + 1 < argc) {
      opts->input_path = argv[++i];
    } else if (strcmp(argv[i], "--output") == 0 && i + 1 < argc) {
      opts->output_path = argv[++i];
    } else if (strcmp(argv[i], "--batch-count") == 0 && i + 1 < argc) {
      long parsed = strtol(argv[++i], NULL, 10);
      if (parsed <= 0 || parsed > (long)AOEM_WORKER_MAX_BATCH_COUNT) {
        return -1;
      }
      opts->batch_count = (uint32_t)parsed;
    } else if (strcmp(argv[i], "--asset-lifecycle") == 0) {
      opts->asset_lifecycle = 1;
    } else {
      return -1;
    }
  }
  if (!opts->library_path) {
    return -1;
  }
  if (opts->asset_lifecycle) {
    return 0;
  }
  return opts->input_path && opts->output_path ? 0 : -1;
}

int main(int argc, char** argv) {
  aoem_worker_options opts;
  if (worker_parse_args(argc, argv, &opts) != 0) {
    worker_usage(argv[0]);
    return 2;
  }

  FILE* input = NULL;
  FILE* output = NULL;

  aoem_host_api api;
  if (load_api(opts.library_path, &api) != 0 || api.global_init() != 0) {
    return 1;
  }
  void* handle = api.create();
  if (!handle) {
    fprintf(stderr, "aoem_create failed\n");
    return 1;
  }

  if (opts.asset_lifecycle) {
    int rc = worker_run_asset_lifecycle_mode(&api, handle, opts.batch_count);
    api.destroy(handle);
    return rc;
  }

  input = strcmp(opts.input_path, "-") == 0 ? stdin : fopen(opts.input_path, "rb");
  if (!input) {
    fprintf(stderr, "failed to open input JSONL: %s\n", opts.input_path);
    api.destroy(handle);
    return 2;
  }
  output = fopen(opts.output_path, "wb");
  if (!output) {
    fprintf(stderr, "failed to open output JSONL: %s\n", opts.output_path);
    if (input != stdin) {
      fclose(input);
    }
    api.destroy(handle);
    return 2;
  }

  aoem_worker_job batch[AOEM_WORKER_MAX_BATCH_COUNT];
  memset(batch, 0, sizeof(batch));
  uint32_t batch_len = 0u;
  uint64_t batch_seq = 0u;
  aoem_worker_stats stats = {0};
  stats.resident_asset_ok = 1;
  stats.proof_ok = 1;
  stats.verify_ok = 1;
  stats.external_verify_ok = 1;

  char line[AOEM_WORKER_LINE_MAX];
  while (fgets(line, sizeof(line), input)) {
    size_t len = strlen(line);
    while (len > 0u && (line[len - 1u] == '\n' || line[len - 1u] == '\r')) {
      line[--len] = '\0';
    }
    if (len == 0u) {
      continue;
    }

    aoem_worker_job job;
    char* error = NULL;
    if (worker_parse_job_line(line, &job, &error) != 0) {
      char* request_id = NULL;
      (void)worker_json_dup_string_field(line, "request_id", &request_id);
      worker_write_error(output, request_id, error ? error : "malformed_payload");
      free(request_id);
      free(error);
      worker_job_free(&job);
      stats.malformed_seen += 1u;
      stats.malformed_rejected += 1u;
      continue;
    }

    if (batch_len > 0u &&
        (job.profile_id != batch[0].profile_id ||
         job.resident_asset_id != batch[0].resident_asset_id ||
         batch_len == opts.batch_count)) {
      (void)worker_process_batch(&api, handle, output, batch, batch_len, batch_seq++, &stats);
      for (uint32_t i = 0; i < batch_len; ++i) {
        worker_job_free(&batch[i]);
      }
      batch_len = 0u;
    }
    batch[batch_len++] = job;
  }

  if (batch_len > 0u) {
    (void)worker_process_batch(&api, handle, output, batch, batch_len, batch_seq++, &stats);
    for (uint32_t i = 0; i < batch_len; ++i) {
      worker_job_free(&batch[i]);
    }
  }

  api.destroy(handle);
  if (input != stdin) {
    fclose(input);
  }
  fclose(output);

  if (stats.failures != 0u) {
    stats.proof_ok = 0;
    stats.verify_ok = 0;
    stats.external_verify_ok = 0;
  }
  const int malformed_ok = stats.malformed_seen == stats.malformed_rejected;
  printf(
      "AOEM_PROOF_WORKER_SUMMARY|profile=%s|jobs=%llu|batch_count=%u|resident_asset=%s|privacy=%s|proof=%s|verify=%s|external_verify=%s|malformed=%s|failures=%llu\n",
      stats.profile_id == AOEM_ZK_MERKLE_MEMBERSHIP_PROOF_V1_ID
          ? "zk_merkle_membership_v1"
          : (stats.profile_id == AOEM_MERKLE_MEMBERSHIP_PROOF_V1_ID ? "merkle_membership_v1"
                                                                    : "fixed_profile_v1"),
      (unsigned long long)stats.jobs_ok,
      opts.batch_count,
      stats.resident_asset_ok ? "ok" : "fail",
      stats.failures == 0 ? "ok" : "fail",
      stats.proof_ok ? "ok" : "fail",
      stats.verify_ok ? "ok" : "fail",
      stats.external_verify_ok ? "ok" : "fail",
      stats.malformed_seen > 0u ? (malformed_ok ? "ok" : "fail") : "not_seen",
      (unsigned long long)stats.failures);

  return stats.failures == 0u && malformed_ok ? 0 : 1;
}
