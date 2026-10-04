/* Regression for retired private-membership envelopes. This reuses the public
 * digest-recomputation counterexample from SUPERVM's 2026-10-04 boundary audit,
 * and calls the authoritative C verifier/worker without an AOEM library.
 * Passing means unsupported/rejected, NOT that a replacement proof exists.
 */
#define AOEM_PROOF_WORKER_NO_MAIN
#include "aoem_proof_worker.c"

// Historical public digest construction retained only to reproduce the attack.
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

static void put_u32(uint8_t* out, uint32_t value) {
  for (unsigned i = 0; i < 4; ++i) {
    out[i] = (uint8_t)(value >> (8u * i));
  }
}

static int make_public_envelope(
    uint8_t proof[199],
    const uint8_t public_input[104],
    const uint8_t payload[52],
    char outputs[1024],
    const uint8_t* witness,
    size_t witness_len) {
  char root[65], commitment[65], nullifier[65], witness_commitment[65];
  uint8_t arbitrary_witness_commitment[32];
  memset(arbitrary_witness_commitment, 0x44, sizeof(arbitrary_witness_commitment));
  bytes_to_hex_lower(public_input, 32, root);
  bytes_to_hex_lower(public_input + 32, 32, commitment);
  bytes_to_hex_lower(public_input + 64, 32, nullifier);
  bytes_to_hex_lower(arbitrary_witness_commitment, 32, witness_commitment);
  int n = snprintf(outputs, 1024,
      "{\"root\":\"%s\",\"leaf_commitment\":\"%s\","
      "\"nullifier\":\"%s\",\"witness_commitment\":\"%s\","
      "\"private_witness_hidden\":true}",
      root, commitment, nullifier, witness_commitment);
  if (n < 0 || n >= 1024) return -1;

  memset(proof, 0, 199);
  memcpy(proof, "AORF\0", 5);
  proof[5] = 3;
  put_u32(proof + 7, 3);
  static const uint8_t label[] = "public_input";
  const uint8_t* parts[1] = {public_input};
  size_t lengths[1] = {104};
  contract_digest32(label, sizeof(label) - 1, parts, lengths, 1, proof + 11);
  /* An arbitrary public string, not a digest of any supplied secret witness. */
  memset(proof + 43, 0x55, 32);
  if (witness) {
    static const uint8_t witness_label[] = "witness_or_scalars";
    const uint8_t* witness_parts[1] = {witness};
    size_t witness_lengths[1] = {witness_len};
    contract_digest32(witness_label, sizeof(witness_label) - 1, witness_parts,
        witness_lengths, 1, proof + 43);
  }
  put_u32(proof + 139, 52);
  memcpy(proof + 143, payload, 52);
  compute_pipeline_digest_from_payload(payload, proof + 75);

  aoem_proof_contract_v3 parsed = {0};
  parsed.profile_id = 3;
  parsed.public_input_digest = proof + 11;
  parsed.witness_digest = proof + 43;
  parsed.pipeline_digest = proof + 75;
  parsed.public_outputs_digest = proof + 107;
  parsed.payload = proof + 143;
  parsed.payload_len = 52;
  if (compute_zk_merkle_public_outputs_digest_from_payload(
          &parsed, public_input, 104, outputs, proof + 107) != 0) return -1;

  char digest[65];
  bytes_to_hex_lower(proof + 107, 32, digest);
  outputs[n - 1] = '\0';
  int added = snprintf(outputs + n - 1, 1024 - (size_t)n + 1,
      ",\"proof_public_outputs_digest_hex\":\"%s\"}", digest);
  if (added < 0 || (size_t)added >= 1024 - (size_t)n + 1) return -1;
  put_u32(proof + 195, contract_checksum_u32(proof, 195));
  return 0;
}

static int check_diagnostic_profile(uint32_t profile) {
  uint8_t proof[199] = {0};
  uint8_t public_bytes[4] = {1, 2, 3, 4}, witness_bytes[4] = {5, 6, 7, 8};
  byte_buf merkle_public = {0}, merkle_witness = {0};
  const uint8_t* public_input = public_bytes;
  const uint8_t* witness = witness_bytes;
  size_t public_len = sizeof(public_bytes), witness_len = sizeof(witness_bytes);
  if (profile == AOEM_MERKLE_MEMBERSHIP_PROOF_V1_ID) {
    if (aoem_build_merkle_membership_fixture(2u, 4u, &merkle_public, &merkle_witness) != 0)
      return -1;
    public_input = merkle_public.data;
    witness = merkle_witness.data;
    public_len = merkle_public.len;
    witness_len = merkle_witness.len;
  }
  memcpy(proof, "AORF\0", 5);
  proof[5] = 3;
  put_u32(proof + 7, profile);
  put_u32(proof + 139, 52);
  put_u32(proof + 143, profile);
  const uint8_t* parts[1] = {public_input};
  size_t lengths[1] = {public_len};
  static const uint8_t public_label[] = "public_input", witness_label[] = "witness_or_scalars";
  contract_digest32(public_label, sizeof(public_label) - 1, parts, lengths, 1, proof + 11);
  parts[0] = witness;
  lengths[0] = witness_len;
  contract_digest32(witness_label, sizeof(witness_label) - 1, parts, lengths, 1, proof + 43);
  compute_pipeline_digest_from_payload(proof + 143, proof + 75);
  aoem_proof_contract_v3 parsed = {0};
  parsed.profile_id = profile;
  parsed.public_input_digest = proof + 11;
  parsed.witness_digest = proof + 43;
  parsed.pipeline_digest = proof + 75;
  parsed.public_outputs_digest = proof + 107;
  parsed.payload = proof + 143;
  parsed.payload_len = 52;
  int rc = profile == AOEM_MERKLE_MEMBERSHIP_PROOF_V1_ID
      ? compute_merkle_public_outputs_digest_from_payload(
            &parsed, public_input, public_len, witness, witness_len, proof + 107)
      : compute_public_outputs_digest_from_payload(&parsed, proof + 107);
  put_u32(proof + 195, contract_checksum_u32(proof, 195));
  if (rc == 0)
    rc = verify_contract_against_inputs(
        proof, sizeof(proof), public_input, public_len, witness, witness_len, profile, NULL);
  proof[143 + 24] ^= 1;
  int tamper = verify_contract_against_inputs(
      proof, sizeof(proof), public_input, public_len, witness, witness_len, profile, NULL);
  buf_free(&merkle_public);
  buf_free(&merkle_witness);
  return rc == 0 && tamper != 0 ? 0 : -1;
}

static int check_worker_rejection(void) {
  const char* lines[] = {
      "{\"request_id\":\"private-no-witness\",\"profile_id\":\"zk_merkle_membership_v1\",\"resident_asset_id\":\"default\"}",
      "{\"request_id\":\"private-with-witness\",\"profile_id\":\"3\",\"resident_asset_id\":\"default\",\"leaf\":\"0102\",\"leaf_secret\":\"0304\",\"witness\":\"01020304\"}",
      "{\"request_id\":\"private-numeric-alias\",\"profile_id\":\"0x3\",\"resident_asset_id\":\"default\"}"};
  FILE* output = tmpfile();
  if (!output) return -1;
  int ok = 1;
  for (size_t i = 0; i < sizeof(lines) / sizeof(lines[0]); ++i) {
    aoem_worker_job job;
    char* error = NULL;
    int rc = worker_parse_job_line(lines[i], &job, &error);
    ok &= rc != 0 && error && strcmp(error, AOEM_WORKER_UNSUPPORTED_PRIVATE_PROFILE) == 0 &&
          job.public_input == NULL && job.witness == NULL;
    worker_write_error(output, job.request_id, error ? error : "missing_error");
    free(error);
    worker_job_free(&job);
  }
  // Even internal callers cannot bypass JSON admission or emit archived profile3
  // success. NULL API is deliberate: these guards must precede any ABI call.
  aoem_worker_job job = {0};
  job.request_id = "private-direct";
  job.profile_id = AOEM_ZK_MERKLE_MEMBERSHIP_PROOF_V1_ID;
  aoem_worker_stats stats = {0};
  ok &= worker_read_and_emit_job(NULL, output, "unused", 0, &job) != 0;
  ok &= worker_process_batch(NULL, NULL, output, &job, 1, 0, &stats) != 0;
  ok &= stats.jobs_ok == 0 && stats.failures == 1 && stats.unsupported_profiles == 1;
  rewind(output);
  char line[512];
  unsigned count = 0;
  while (fgets(line, sizeof(line), output)) {
    ++count;
    ok &= strstr(line, "\"status\":\"error\"") != NULL &&
          strstr(line, "\"proof_written\":false") != NULL &&
          strstr(line, AOEM_WORKER_UNSUPPORTED_PRIVATE_PROFILE) != NULL &&
          strstr(line, "\"proof\":") == NULL;
  }
  fclose(output);
  return ok && count == 5 ? 0 : -1;
}

int main(void) {
  uint8_t public_input[104] = {0}, payload[52] = {0}, proof[199], witness[16] = {0};
  char outputs[1024];
  memset(public_input, 0x11, 32);
  memset(public_input + 32, 0x22, 32);
  memset(public_input + 64, 0x33, 32);
  // Depth zero demands root == leaf_commitment; this statement is impossible.
  put_u32(public_input + 96, 0);
  put_u32(public_input + 100, 1);
  put_u32(payload, 3);
  put_u32(payload + 4, 0x12345678);
  put_u32(payload + 16, 1);
  put_u32(payload + 20, 2);
  put_u32(payload + 24, 0x10203040);
  put_u32(payload + 28, 0x50607080);
  put_u32(payload + 32, 0x90a0b0c0);
  put_u32(payload + 36, 104);
  put_u32(payload + 40, 16);
  int ok = memcmp(public_input, public_input + 32, 32) != 0;
  unsigned rejected = 0;
  for (unsigned with_witness = 0; with_witness < 2; ++with_witness) {
    const uint8_t* supplied = with_witness ? witness : NULL;
    size_t supplied_len = with_witness ? sizeof(witness) : 0;
    for (unsigned attempt = 0; attempt < 4; ++attempt) {
      if (attempt == 3) payload[24] ^= 1;
      if (make_public_envelope(proof, public_input, payload, outputs, supplied, supplied_len) != 0)
        return 2;
      aoem_proof_contract_v3 parsed;
      ok &= parse_proof_contract_v3(proof, sizeof(proof), &parsed) ==
            AOEM_PROOF_VERIFY_UNSUPPORTED_PRIVATE_PROFILE;
      if (attempt == 1 || attempt == 2) proof[143 + 24] ^= 1;
      if (attempt == 2) put_u32(proof + 195, contract_checksum_u32(proof, 195));
      int rc = verify_contract_against_inputs(proof, sizeof(proof), public_input,
          sizeof(public_input), supplied, supplied_len, 3, outputs);
      ok &= rc == AOEM_PROOF_VERIFY_UNSUPPORTED_PRIVATE_PROFILE;
      rejected += rc == AOEM_PROOF_VERIFY_UNSUPPORTED_PRIVATE_PROFILE;
    }
    // Older AORF version labels must not provide a fallback either.
    for (uint8_t version = 1; version <= 2; ++version) {
      proof[5] = version;
      int rc = verify_contract_against_inputs(proof, sizeof(proof), public_input,
          sizeof(public_input), supplied, supplied_len, 3, outputs);
      ok &= rc == AOEM_PROOF_VERIFY_UNSUPPORTED_PRIVATE_PROFILE;
      rejected += rc == AOEM_PROOF_VERIFY_UNSUPPORTED_PRIVATE_PROFILE;
    }
  }
  // A public envelope cannot be relabeled as profile1 while carrying profile3
  // payload semantics, even after the attacker repairs its public checksum.
  if (make_public_envelope(proof, public_input, payload, outputs, NULL, 0) != 0) return 2;
  put_u32(proof + 7, 1);
  put_u32(proof + 195, contract_checksum_u32(proof, 195));
  aoem_proof_contract_v3 parsed;
  int relabel_rejected = parse_proof_contract_v3(proof, sizeof(proof), &parsed) != 0 &&
      verify_contract_against_inputs(proof, sizeof(proof), public_input,
          sizeof(public_input), NULL, 0, 1, outputs) != 0 &&
      verify_contract_against_inputs(proof, sizeof(proof), public_input,
          sizeof(public_input), NULL, 0, 3, outputs) == AOEM_PROOF_VERIFY_UNSUPPORTED_PRIVATE_PROFILE;
  int diagnostics = check_diagnostic_profile(1) == 0 && check_diagnostic_profile(2) == 0;
  const char* diagnostic_status = "{\"kind\":\"compute.zk.resident_proof_v1.status\","
      "\"proof_verified\":false,\"envelope_integrity_verified\":true,"
      "\"cryptographic_proof_verified\":false,\"verification_scope\":\"envelope_integrity_only_not_zk\"}";
  const char* legacy_status = "{\"kind\":\"compute.zk.resident_proof_v1.status\",\"proof_verified\":true}";
  ok &= resident_diagnostic_status_is_valid(diagnostic_status) &&
      !resident_diagnostic_status_is_valid(legacy_status);
  const char* diagnostic_verify = "{\"kind\":\"compute.zk.resident_proof_v1.verify_status\","
      "\"accepted\":false,\"envelope_integrity_verified\":true,"
      "\"cryptographic_proof_verified\":false,\"verification_scope\":\"envelope_integrity_only_not_zk\"}";
  const char* misleading_verify = "{\"kind\":\"compute.zk.resident_proof_v1.verify_status\","
      "\"accepted\":true,\"envelope_integrity_verified\":true,"
      "\"cryptographic_proof_verified\":false,\"verification_scope\":\"envelope_integrity_only_not_zk\"}";
  const char* diagnostic_bytes = "{\"fixed_profile_verifier_accepted\":false,"
      "\"envelope_integrity_verified\":true,\"cryptographic_proof_verified\":false,"
      "\"verification_scope\":\"envelope_integrity_only_not_zk\"}";
  const char* misleading_bytes = "{\"fixed_profile_verifier_accepted\":true,"
      "\"envelope_integrity_verified\":true,\"cryptographic_proof_verified\":false,"
      "\"verification_scope\":\"envelope_integrity_only_not_zk\"}";
  ok &= resident_diagnostic_verify_status_is_valid(diagnostic_verify) &&
      !resident_diagnostic_verify_status_is_valid(misleading_verify) &&
      resident_diagnostic_bytes_is_valid(diagnostic_bytes) &&
      !resident_diagnostic_bytes_is_valid(misleading_bytes);
  int worker = check_worker_rejection() == 0;
  printf("PRIVATE_PROFILE_FAIL_CLOSED|adversarial_rejected=%u/12|with_and_without_witness=%s|"
         "diagnostic_profiles_1_2=%s|worker_no_proof=%s|profile_relabel_rejected=%s|replacement_crypto_proof=not_implemented\n",
         rejected, ok ? "ok" : "fail", diagnostics ? "ok" : "fail", worker ? "ok" : "fail",
         relabel_rejected ? "ok" : "fail");
  return ok && diagnostics && worker && relabel_rejected ? 0 : 1;
}
