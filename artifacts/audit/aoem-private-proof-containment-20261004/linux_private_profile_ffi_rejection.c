/* Test-only real-library containment check. No proof generation, GPU fallback,
 * product wiring or new ABI. Build against the packaged header and reuse the
 * current SDK request builders; the library path must be supplied explicitly.
 */
#define AOEM_PROOF_WORKER_NO_MAIN
#include "../../../aoem/examples/aoem_proof_worker.c"

typedef const char* (*audit_last_error_fn)(void*);

static const char* const retired_error =
    "zk_merkle_membership_v1 retired: no independent cryptographic relation verifier";

static int require_absent(const aoem_host_api* api, const char* key) {
  char* response = NULL;
  char expected_key[512];
  int n = snprintf(expected_key, sizeof(expected_key), "\"key\":\"%s\"", key);
  if (n <= 0 || (size_t)n >= sizeof(expected_key) ||
      read_state_response(api, key, &response) != 0) return -1;
  int ok = strstr(response, "\"status\":\"ok\"") != NULL &&
      strstr(response, "\"status_code\":0") != NULL &&
      strstr(response, expected_key) != NULL &&
      strstr(response, "\"found\":false") != NULL &&
      strstr(response, "\"found\":true") == NULL;
  if (!ok) fprintf(stderr, "output absence not established: %s\n", response);
  free(response);
  return ok ? 0 : -1;
}

static int require_outputs_absent(
    const aoem_host_api* api, const char* prefix, uint32_t asset_id) {
  static const char* proof_suffixes[] = {
      "bytes", "status", "metadata", "public_outputs", "verify_status",
      "0/bytes", "0/status", "0/metadata", "0/public_outputs", "0/verify_status",
      "batch/count", "batch/status", "batch/metadata",
      "assets/list", "assets/status", "assets/selected"};
  static const char* asset_suffixes[] = {"status", "profile_id", "digest"};
  char key[384];
  for (size_t i = 0; i < sizeof(proof_suffixes) / sizeof(proof_suffixes[0]); ++i) {
    int n = snprintf(key, sizeof(key), "%s/zk/proof/%s", prefix, proof_suffixes[i]);
    if (n <= 0 || (size_t)n >= sizeof(key) || require_absent(api, key) != 0) return -1;
  }
  for (size_t i = 0; i < sizeof(asset_suffixes) / sizeof(asset_suffixes[0]); ++i) {
    int n = snprintf(key, sizeof(key), "%s/zk/proof/asset/%u/%s", prefix,
        asset_id, asset_suffixes[i]);
    if (n <= 0 || (size_t)n >= sizeof(key) || require_absent(api, key) != 0) return -1;
  }
  return 0;
}

static int check_rejection(
    const aoem_host_api* api, void* handle, audit_last_error_fn last_error,
    uint8_t opcode, const char* prefix, uint32_t asset_id, const byte_buf* wire) {
  if (require_outputs_absent(api, prefix, asset_id) != 0) return -1;
  aoem_exec_v2_result result = {99, 99, 99, 99};
  int32_t rc = api->execute_ops_wire_v1(handle, wire->data, wire->len, &result);
  const char* error = last_error(handle);
  int retired = error && strcmp(error, retired_error) == 0;
  printf("PRIVATE_PROFILE_REAL_FFI|opcode=%u|rc=%d|processed=%u|success=%u|"
         "failed_index=%u|writes=%llu|retired_error_exact=%s|error=%s\n",
      (unsigned)opcode, rc, result.processed, result.success, result.failed_index,
      (unsigned long long)result.total_writes, retired ? "true" : "false",
      error ? error : "missing");
  if (rc != -4 || result.processed != 0 || result.success != 0 ||
      result.failed_index != 0 || result.total_writes != 0 || !retired) return -1;
  return require_outputs_absent(api, prefix, asset_id);
}

int main(int argc, char** argv) {
  // Included for the existing request builders, not the worker's JSON entry.
  // Keep that uncalled static helper referenced without weakening -Werror.
  (void)worker_parse_job_line;
  if (argc != 2) {
    fprintf(stderr, "usage: %s EXPLICIT_CORRECTED_AOEM_LIBRARY\n", argv[0]);
    return 2;
  }
  aoem_host_api api;
  if (load_api(argv[1], &api) != 0 || api.abi_version() != 1) return 2;
  audit_last_error_fn last_error =
      (audit_last_error_fn)aoem_load_symbol(api.lib, "aoem_last_error");
  if (!last_error || api.global_init() != 0) return 2;
  void* handle = api.create();
  if (!handle) return 2;

  const uint32_t asset_id = 0xF0FFEF03u;
  const char* proof_prefix = "aoem.compute.output/linux-retired-profile3-proof";
  const char* asset_prefix = "aoem.compute.output/linux-retired-profile3-asset";
  byte_buf proof_wire = {0}, asset_wire = {0};
  int built = build_proof_zk_merkle_membership_batch_wire(
      &proof_wire, "linux-retired-profile3-proof", proof_prefix, 1u,
      AOEM_FIXED_PROFILE_RESIDENT_ASSET_V1_ID) == 0 &&
      worker_build_asset_lifecycle_wire(
          &asset_wire, "linux-retired-profile3-asset", asset_prefix,
          AOEM_ZK_RESIDENT_ASSET_CMD_SETUP, AOEM_ZK_MERKLE_MEMBERSHIP_PROOF_V1_ID,
          asset_id, "retired-private-fixture", NULL, 0u) == 0;
  int proof_ok = built && check_rejection(
      &api, handle, last_error, 98u, proof_prefix, asset_id, &proof_wire) == 0;
  int asset_ok = built && check_rejection(
      &api, handle, last_error, 99u, asset_prefix, asset_id, &asset_wire) == 0;
  buf_free(&proof_wire);
  buf_free(&asset_wire);
  api.destroy(handle);
  printf("PRIVATE_PROFILE_REAL_FFI_SUMMARY|request_build=%s|op98_wire_v4=%s|"
         "op99_setup=%s|zero_work_and_output_absence=%s|"
         "replacement_crypto_proof=not_implemented\n",
      built ? "ok" : "fail", proof_ok ? "rejected" : "fail",
      asset_ok ? "rejected" : "fail", proof_ok && asset_ok ? "ok" : "fail");
  return proof_ok && asset_ok ? 0 : 1;
}
