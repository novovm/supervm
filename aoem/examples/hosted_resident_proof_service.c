// Minimal hosted service loop for the fixed-profile resident proof workload.
//
// This is an external host example, not an AOEM runtime extension. It repeatedly
// calls the existing wire_v1 product entry and reads proof state back through
// aoem_state_read_v1.

#define main aoem_resident_proof_smoke_main
#include "hosted_resident_proof_smoke.c"
#undef main

static uint32_t parse_iterations_arg(int argc, char** argv) {
  if (argc <= 2) {
    return 10;
  }
  for (int i = 2; i < argc; ++i) {
    if (strcmp(argv[i], "--iterations") == 0 && i + 1 < argc) {
      long parsed = strtol(argv[i + 1], NULL, 10);
      if (parsed > 0 && parsed <= 1000000L) {
        return (uint32_t)parsed;
      }
      return 10;
    }
  }
  long parsed = strtol(argv[2], NULL, 10);
  if (parsed <= 0 || parsed > 1000000L) {
    return 10;
  }
  return (uint32_t)parsed;
}

static uint32_t parse_batch_count_arg(int argc, char** argv) {
  for (int i = 2; i < argc; ++i) {
    if (strcmp(argv[i], "--batch-count") == 0 && i + 1 < argc) {
      long parsed = strtol(argv[i + 1], NULL, 10);
      if (parsed > 0 && parsed <= 8L) {
        return (uint32_t)parsed;
      }
      return 1;
    }
  }
  return 1;
}

static uint32_t parse_resident_asset_id_arg(int argc, char** argv) {
  for (int i = 2; i < argc; ++i) {
    if (strcmp(argv[i], "--resident-asset-id") == 0 && i + 1 < argc) {
      unsigned long parsed = strtoul(argv[i + 1], NULL, 0);
      if (parsed > 0u && parsed <= 0xfffffffful) {
        return (uint32_t)parsed;
      }
      return AOEM_FIXED_PROFILE_RESIDENT_ASSET_V1_ID;
    }
  }
  return AOEM_FIXED_PROFILE_RESIDENT_ASSET_V1_ID;
}

static int AOEM_MAYBE_UNUSED run_success_iteration(
    const aoem_host_api* api,
    void* handle,
    uint32_t iteration) {
  char request_id[128];
  char output_prefix[192];
  char proof_key[256];
  char metadata_key[256];
  char status_key[256];
  char public_outputs_key[256];
  char verify_status_key[256];

  if (snprintf(request_id, sizeof(request_id), "c-host-resident-proof-service-%u", iteration) <= 0 ||
      snprintf(output_prefix, sizeof(output_prefix), "aoem.compute.output/%s", request_id) <= 0 ||
      snprintf(proof_key, sizeof(proof_key), "%s/zk/proof/bytes", output_prefix) <= 0 ||
      snprintf(metadata_key, sizeof(metadata_key), "%s/zk/proof/metadata", output_prefix) <= 0 ||
      snprintf(status_key, sizeof(status_key), "%s/zk/proof/status", output_prefix) <= 0 ||
      snprintf(
          public_outputs_key,
          sizeof(public_outputs_key),
          "%s/zk/proof/public_outputs",
          output_prefix) <= 0 ||
      snprintf(
          verify_status_key,
          sizeof(verify_status_key),
          "%s/zk/proof/verify_status",
          output_prefix) <= 0) {
    fprintf(stderr, "resident proof service key formatting failed\n");
    return -1;
  }

  uint8_t public_input[16];
  uint8_t witness[32];
  for (uint32_t i = 0; i < (uint32_t)sizeof(public_input); ++i) {
    public_input[i] = (uint8_t)(0x30u + ((iteration + i) & 0x0fu));
  }
  for (uint32_t i = 0; i < (uint32_t)sizeof(witness); ++i) {
    witness[i] = (uint8_t)(0x80u ^ ((iteration * 17u + i * 3u) & 0xffu));
  }

  byte_buf wire = {0};
  if (build_proof_wire_with_input(
          &wire,
          request_id,
          output_prefix,
          public_input,
          (uint32_t)sizeof(public_input),
          witness,
          (uint32_t)sizeof(witness)) != 0) {
    fprintf(stderr, "failed to build resident proof service wire payload\n");
    buf_free(&wire);
    return -1;
  }

  aoem_exec_v2_result result = {0};
  int32_t rc = api->execute_ops_wire_v1(handle, wire.data, wire.len, &result);
  buf_free(&wire);
  if (rc != 0 || result.processed != 1 || result.success != 1 || result.total_writes != 5) {
    fprintf(
        stderr,
        "resident proof service execute failed rc=%d processed=%u success=%u writes=%llu\n",
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

static int run_batch_iteration(
    const aoem_host_api* api,
    void* handle,
    uint32_t iteration,
    uint32_t batch_count,
    uint32_t resident_asset_id) {
  char request_id[128];
  char output_prefix[192];
  char batch_count_key[256];
  char batch_metadata_key[256];
  char batch_status_key[256];

  if (snprintf(request_id, sizeof(request_id), "c-host-resident-proof-service-batch-%u", iteration) <=
          0 ||
      snprintf(output_prefix, sizeof(output_prefix), "aoem.compute.output/%s", request_id) <= 0 ||
      snprintf(batch_count_key, sizeof(batch_count_key), "%s/zk/proof/batch_count", output_prefix) <=
          0 ||
      snprintf(
          batch_metadata_key,
          sizeof(batch_metadata_key),
          "%s/zk/proof/batch/metadata",
          output_prefix) <= 0 ||
      snprintf(
          batch_status_key,
          sizeof(batch_status_key),
          "%s/zk/proof/batch/status",
          output_prefix) <= 0) {
    fprintf(stderr, "resident proof batch service key formatting failed\n");
    return -1;
  }

  byte_buf wire = {0};
  if (build_proof_resident_asset_batch_wire(
          &wire,
          request_id,
          output_prefix,
          batch_count,
          resident_asset_id) != 0) {
    fprintf(stderr, "failed to build resident asset proof batch service wire payload\n");
    buf_free(&wire);
    return -1;
  }

  aoem_exec_v2_result result = {0};
  int32_t rc = api->execute_ops_wire_v1(handle, wire.data, wire.len, &result);
  buf_free(&wire);
  const uint64_t expected_writes = 3u + 5u * (uint64_t)batch_count;
  if (rc != 0 || result.processed != 1 || result.success != 1 ||
      result.total_writes != expected_writes) {
    fprintf(
        stderr,
        "resident proof batch service execute failed rc=%d processed=%u success=%u writes=%llu expected=%llu\n",
        rc,
        result.processed,
        result.success,
        (unsigned long long)result.total_writes,
        (unsigned long long)expected_writes);
    return -1;
  }

  if (read_state_contains_all(
          api,
          batch_count_key,
          "compute.zk.resident_proof_v1.batch_count",
          "\"batch_count\":",
          "\"resident_asset_bound\":true") != 0) {
    return -1;
  }
  if (read_state_contains_all(
          api,
          batch_metadata_key,
          "compute.zk.resident_proof_v1.batch.metadata",
          "\"input_source\":\"payload_v4_resident_asset_batch_real_input\"",
          "\"resident_asset_bound\":true") != 0 ||
      read_state_contains_all(
          api,
          batch_metadata_key,
          "compute.zk.resident_proof_v1.batch.metadata",
          "\"setup_once_runtime_inputs_only\":true",
          "\"runtime_canon_unchanged\":true") != 0) {
    return -1;
  }
  if (read_state_contains_all(
          api,
          batch_status_key,
          "compute.zk.resident_proof_v1.batch.status",
          "\"batch_status\":\"ok\"",
          "\"resident_asset_status\":\"ok\"") != 0 ||
      read_state_contains_all(
          api,
          batch_status_key,
          "compute.zk.resident_proof_v1.batch.status",
          "\"resident_asset_bound\":true",
          "\"all_verify_status\":\"ok\"") != 0) {
    return -1;
  }

  for (uint32_t batch_index = 0; batch_index < batch_count; ++batch_index) {
    char proof_key[256];
    char metadata_key[256];
    char status_key[256];
    char public_outputs_key[256];
    char verify_status_key[256];
    if (snprintf(proof_key, sizeof(proof_key), "%s/zk/proof/%u/bytes", output_prefix, batch_index) <=
            0 ||
        snprintf(
            metadata_key,
            sizeof(metadata_key),
            "%s/zk/proof/%u/metadata",
            output_prefix,
            batch_index) <= 0 ||
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
      return -1;
    }
    if (read_state_contains_all(
            api,
            proof_key,
            "compute.zk.resident_proof_v1",
            "\"fixed_profile_verifier_accepted\":true",
            "\"resident_asset_bound\":true") != 0) {
      return -1;
    }
    if (read_state_contains_all(
            api,
            metadata_key,
            "\"input_source\":\"payload_v4_resident_asset_batch_real_input\"",
            "\"resident_asset_bound\":true",
            "\"proof_batch_count\":") != 0 ||
        read_state_contains_all(
            api,
            metadata_key,
            "\"setup_once_runtime_inputs_only\":true",
            "\"resident_asset_id\":",
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
            "\"proof_public_outputs_digest_hex\":") != 0) {
      return -1;
    }
    if (read_state_contains_all(
            api,
            verify_status_key,
            "compute.zk.resident_proof_v1.verify_status",
            "\"accepted\":true",
            "\"external_verifier_compatible\":true") != 0) {
      return -1;
    }
  }
  return 0;
}

static int run_merkle_membership_iteration(
    const aoem_host_api* api,
    void* handle,
    uint32_t iteration,
    uint32_t batch_count,
    uint32_t resident_asset_id) {
  char request_id[128];
  char output_prefix[192];
  char batch_status_key[256];
  if (snprintf(request_id, sizeof(request_id), "c-host-resident-proof-service-merkle-%u", iteration) <=
          0 ||
      snprintf(output_prefix, sizeof(output_prefix), "aoem.compute.output/%s", request_id) <= 0 ||
      snprintf(
          batch_status_key,
          sizeof(batch_status_key),
          "%s/zk/proof/batch/status",
          output_prefix) <= 0) {
    return -1;
  }

  byte_buf wire = {0};
  if (build_proof_merkle_membership_batch_wire(
          &wire,
          request_id,
          output_prefix,
          batch_count,
          resident_asset_id) != 0) {
    buf_free(&wire);
    return -1;
  }
  aoem_exec_v2_result result = {0};
  int32_t rc = api->execute_ops_wire_v1(handle, wire.data, wire.len, &result);
  buf_free(&wire);
  const uint64_t expected_writes = 3u + 5u * (uint64_t)batch_count;
  if (rc != 0 || result.processed != 1 || result.success != 1 ||
      result.total_writes != expected_writes) {
    return -1;
  }
  if (read_state_contains_all(
          api,
          batch_status_key,
          "compute.zk.resident_proof_v1.batch.status",
          "\"batch_status\":\"ok\"",
          "\"all_verify_status\":\"ok\"") != 0) {
    return -1;
  }
  for (uint32_t batch_index = 0; batch_index < batch_count; ++batch_index) {
    char proof_key[256];
    char public_outputs_key[256];
    char verify_status_key[256];
    if (snprintf(proof_key, sizeof(proof_key), "%s/zk/proof/%u/bytes", output_prefix, batch_index) <=
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
      return -1;
    }
    if (read_state_contains_all(
            api,
            proof_key,
            "\"proof_profile\":\"merkle_membership_v1\"",
            "\"membership_verifier_accepted\":true",
            "\"zero_knowledge_privacy_claim\":false") != 0) {
      return -1;
    }
    if (read_state_contains_all(
            api,
            public_outputs_key,
            "\"membership_profile\":true",
            "\"hash_profile\":\"merkle_style_v1\"",
            "\"membership_root_match\":true") != 0) {
      return -1;
    }
    if (read_state_contains_all(
            api,
            verify_status_key,
            "\"verifier\":\"merkle_membership_v1\"",
            "\"membership_root\":\"ok\"",
            "\"accepted\":true") != 0) {
      return -1;
    }
  }
  return 0;
}

static int run_merkle_membership_tamper_rejection(
    const aoem_host_api* api,
    void* handle,
    uint32_t resident_asset_id) {
  const char* request_id = "c-host-resident-proof-service-merkle-tamper";
  const char* output_prefix =
      "aoem.compute.output/c-host-resident-proof-service-merkle-tamper";
  const char* batch_status_key =
      "aoem.compute.output/c-host-resident-proof-service-merkle-tamper/zk/proof/batch/status";

  byte_buf wire = {0};
  byte_buf public_input = {0};
  byte_buf witness = {0};
  if (build_proof_merkle_membership_batch_wire(
          &wire,
          request_id,
          output_prefix,
          1u,
          resident_asset_id) != 0 ||
      aoem_build_merkle_membership_fixture(3u, 4u, &public_input, &witness) != 0) {
    buf_free(&wire);
    buf_free(&public_input);
    buf_free(&witness);
    return -1;
  }

  int tampered = 0;
  for (size_t i = 0; i + 32u <= wire.len; ++i) {
    if (memcmp(wire.data + i, public_input.data, 32u) == 0) {
      wire.data[i] ^= 0x5au;
      tampered = 1;
      break;
    }
  }
  buf_free(&public_input);
  buf_free(&witness);
  if (!tampered) {
    buf_free(&wire);
    return -1;
  }

  aoem_exec_v2_result result = {99, 99, 0, 99};
  int32_t rc = api->execute_ops_wire_v1(handle, wire.data, wire.len, &result);
  buf_free(&wire);
  if (rc == 0 || result.success != 0 || result.total_writes != 0) {
    return -1;
  }
  return read_state_found(api, batch_status_key) == 0 ? 0 : -1;
}

static int run_zk_merkle_membership_iteration(
    const aoem_host_api* api,
    void* handle,
    uint32_t iteration,
    uint32_t batch_count,
    uint32_t resident_asset_id) {
  char request_id[128];
  char output_prefix[192];
  char batch_status_key[256];
  if (snprintf(
          request_id,
          sizeof(request_id),
          "c-host-resident-proof-service-zk-merkle-%u",
          iteration) <= 0 ||
      snprintf(output_prefix, sizeof(output_prefix), "aoem.compute.output/%s", request_id) <= 0 ||
      snprintf(
          batch_status_key,
          sizeof(batch_status_key),
          "%s/zk/proof/batch/status",
          output_prefix) <= 0) {
    return -1;
  }

  byte_buf wire = {0};
  if (build_proof_zk_merkle_membership_batch_wire(
          &wire,
          request_id,
          output_prefix,
          batch_count,
          resident_asset_id) != 0) {
    buf_free(&wire);
    return -1;
  }
  aoem_exec_v2_result result = {0};
  int32_t rc = api->execute_ops_wire_v1(handle, wire.data, wire.len, &result);
  buf_free(&wire);
  const uint64_t expected_writes = 3u + 5u * (uint64_t)batch_count;
  if (rc != 0 || result.processed != 1 || result.success != 1 ||
      result.total_writes != expected_writes) {
    return -1;
  }
  if (read_state_contains_all(
          api,
          batch_status_key,
          "compute.zk.resident_proof_v1.batch.status",
          "\"batch_status\":\"ok\"",
          "\"all_verify_status\":\"ok\"") != 0) {
    return -1;
  }
  for (uint32_t batch_index = 0; batch_index < batch_count; ++batch_index) {
    char proof_key[256];
    char public_outputs_key[256];
    char verify_status_key[256];
    if (snprintf(proof_key, sizeof(proof_key), "%s/zk/proof/%u/bytes", output_prefix, batch_index) <=
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
      return -1;
    }
    if (read_state_contains_all(
            api,
            proof_key,
            "\"proof_profile\":\"zk_merkle_membership_v1\"",
            "\"private_membership_verifier_accepted\":true",
            "\"private_witness_hidden\":true") != 0) {
      return -1;
    }
    if (read_state_contains_all(
            api,
            public_outputs_key,
            "\"zk_membership_profile\":true",
            "\"hash_profile\":\"zk_merkle_style_v1\"",
            "\"private_witness_hidden\":true") != 0) {
      return -1;
    }
    char* public_outputs_response = NULL;
    int privacy_ok =
        read_state_response(api, public_outputs_key, &public_outputs_response) == 0 &&
        strstr(public_outputs_response, "\"leaf_hash\"") == NULL &&
        strstr(public_outputs_response, "\"leaf_index\"") == NULL &&
        strstr(public_outputs_response, "\"sibling_path\"") == NULL &&
        strstr(public_outputs_response, "\"path_digest\"") == NULL &&
        strstr(public_outputs_response, "\"computed_root\"") == NULL;
    free(public_outputs_response);
    if (!privacy_ok) {
      return -1;
    }
    if (read_state_contains_all(
            api,
            verify_status_key,
            "\"verifier\":\"zk_merkle_membership_v1\"",
            "\"private_witness_hidden\":true",
            "\"accepted\":true") != 0) {
      return -1;
    }
  }
  return 0;
}

static int run_zk_merkle_membership_tamper_rejection(
    const aoem_host_api* api,
    void* handle,
    uint32_t resident_asset_id) {
  const char* request_id = "c-host-resident-proof-service-zk-merkle-tamper";
  const char* output_prefix =
      "aoem.compute.output/c-host-resident-proof-service-zk-merkle-tamper";
  const char* batch_status_key =
      "aoem.compute.output/c-host-resident-proof-service-zk-merkle-tamper/zk/proof/batch/status";

  byte_buf wire = {0};
  byte_buf public_input = {0};
  byte_buf witness = {0};
  if (build_proof_zk_merkle_membership_batch_wire(
          &wire,
          request_id,
          output_prefix,
          1u,
          resident_asset_id) != 0 ||
      aoem_build_zk_merkle_membership_fixture(2u, 4u, &public_input, &witness) != 0) {
    buf_free(&wire);
    buf_free(&public_input);
    buf_free(&witness);
    return -1;
  }

  int tampered = 0;
  for (size_t i = 0; i + 32u <= wire.len; ++i) {
    if (memcmp(wire.data + i, public_input.data, 32u) == 0) {
      wire.data[i] ^= 0x5au;
      tampered = 1;
      break;
    }
  }
  buf_free(&public_input);
  buf_free(&witness);
  if (!tampered) {
    buf_free(&wire);
    return -1;
  }

  aoem_exec_v2_result result = {99, 99, 0, 99};
  int32_t rc = api->execute_ops_wire_v1(handle, wire.data, wire.len, &result);
  buf_free(&wire);
  if (rc == 0 || result.success != 0 || result.total_writes != 0) {
    return -1;
  }
  return read_state_found(api, batch_status_key) == 0 ? 0 : -1;
}

static int run_malformed_iteration(const aoem_host_api* api, void* handle) {
  const char* request_id = "c-host-resident-proof-service-malformed";
  const char* output_prefix = "aoem.compute.output/c-host-resident-proof-service-malformed";
  const char* proof_key =
      "aoem.compute.output/c-host-resident-proof-service-malformed/zk/proof/bytes";
  const char* metadata_key =
      "aoem.compute.output/c-host-resident-proof-service-malformed/zk/proof/metadata";
  const char* status_key =
      "aoem.compute.output/c-host-resident-proof-service-malformed/zk/proof/status";
  const char* public_outputs_key =
      "aoem.compute.output/c-host-resident-proof-service-malformed/zk/proof/public_outputs";
  const char* verify_status_key =
      "aoem.compute.output/c-host-resident-proof-service-malformed/zk/proof/verify_status";

  byte_buf wire = {0};
  if (build_malformed_proof_wire(&wire, request_id, output_prefix) != 0) {
    fprintf(stderr, "failed to build malformed resident proof service wire payload\n");
    buf_free(&wire);
    return -1;
  }

  aoem_exec_v2_result result = {99, 99, 0, 99};
  int32_t rc = api->execute_ops_wire_v1(handle, wire.data, wire.len, &result);
  buf_free(&wire);
  if (rc == 0 || result.success != 0 || result.total_writes != 0) {
    fprintf(
        stderr,
        "malformed resident proof service unexpectedly succeeded rc=%d success=%u writes=%llu\n",
        rc,
        result.success,
        (unsigned long long)result.total_writes);
    return -1;
  }

  if (read_state_found(api, proof_key) != 0 || read_state_found(api, metadata_key) != 0 ||
      read_state_found(api, status_key) != 0 || read_state_found(api, public_outputs_key) != 0 ||
      read_state_found(api, verify_status_key) != 0) {
    fprintf(stderr, "malformed resident proof service wrote state unexpectedly\n");
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
  const uint32_t iterations = parse_iterations_arg(argc, argv);
  const uint32_t batch_count = parse_batch_count_arg(argc, argv);
  const uint32_t resident_asset_id = parse_resident_asset_id_arg(argc, argv);

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

  uint32_t failures = 0;
  for (uint32_t i = 0; i < iterations; ++i) {
    int ok = run_batch_iteration(&api, handle, i, batch_count, resident_asset_id);
    if (ok != 0) {
      ++failures;
      break;
    }
  }

  int membership_ok = 0;
  if (failures == 0) {
    membership_ok = 1;
    for (uint32_t i = 0; i < iterations; ++i) {
      if (run_merkle_membership_iteration(&api, handle, i, batch_count, resident_asset_id) != 0) {
        membership_ok = 0;
        ++failures;
        break;
      }
    }
  }

  int tamper_rejected_ok = 0;
  if (failures == 0) {
    tamper_rejected_ok =
        run_merkle_membership_tamper_rejection(&api, handle, resident_asset_id) == 0;
    if (!tamper_rejected_ok) {
      ++failures;
    }
  }

  int zk_membership_ok = 0;
  if (failures == 0) {
    zk_membership_ok = 1;
    for (uint32_t i = 0; i < iterations; ++i) {
      if (run_zk_merkle_membership_iteration(&api, handle, i, batch_count, resident_asset_id) !=
          0) {
        zk_membership_ok = 0;
        ++failures;
        break;
      }
    }
  }

  int zk_tamper_rejected_ok = 0;
  if (failures == 0) {
    zk_tamper_rejected_ok =
        run_zk_merkle_membership_tamper_rejection(&api, handle, resident_asset_id) == 0;
    if (!zk_tamper_rejected_ok) {
      ++failures;
    }
  }

  int malformed_ok = 0;
  if (failures == 0) {
    malformed_ok = run_malformed_iteration(&api, handle) == 0;
    if (!malformed_ok) {
      ++failures;
    }
  }

  api.destroy(handle);

  const char* ok = failures == 0 ? "ok" : "fail";
  printf(
      "C_HOST_RESIDENT_PROOF_SERVICE|profile=zk_merkle_membership_v1|iterations=%u|batch_count=%u|resident_asset=%s|real_input=%s|batch=%s|membership=%s|zk_membership=%s|privacy=%s|proof=%s|verify=%s|external_verify=%s|tamper_rejected=%s|status=%s|metadata=%s|malformed=%s|failures=%u\n",
      iterations,
      batch_count,
      ok,
      ok,
      ok,
      membership_ok ? "ok" : "fail",
      zk_membership_ok ? "ok" : "fail",
      zk_membership_ok ? "ok" : "fail",
      ok,
      ok,
      ok,
      (tamper_rejected_ok && zk_tamper_rejected_ok) ? "ok" : "fail",
      ok,
      ok,
      malformed_ok ? "ok" : "fail",
      failures);
  return failures == 0 ? 0 : 1;
}
