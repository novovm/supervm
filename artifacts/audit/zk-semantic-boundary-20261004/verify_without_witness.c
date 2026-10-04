/* Local, fictitious-input counterexample for the shipped standalone verifier.
 * No AOEM library is loaded and no GPU, network, wallet or chain is accessed.
 * The verifier implementation is included unchanged, not copied or weakened.
 */
#define AOEM_RESIDENT_PROOF_VERIFY_NO_MAIN
#include "../../../aoem/examples/hosted_resident_proof_verify.c"

static void put_u32(uint8_t* out, uint32_t value) {
  for (unsigned i = 0; i < 4; ++i) {
    out[i] = (uint8_t)(value >> (8u * i));
  }
}

static int make_public_envelope(
    uint8_t proof[199],
    const uint8_t public_input[104],
    const uint8_t payload[52],
    char outputs[1024]) {
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

int main(void) {
  uint8_t public_input[104] = {0};
  memset(public_input, 0x11, 32);
  memset(public_input + 32, 0x22, 32);
  memset(public_input + 64, 0x33, 32);
  /* At depth zero a valid membership requires root == leaf_commitment.
   * These fictitious public values deliberately violate that relation.
   */
  put_u32(public_input + 96, 0);
  put_u32(public_input + 100, 1);
  uint8_t payload[52] = {0};
  put_u32(payload, 3);
  put_u32(payload + 4, 0x12345678);
  put_u32(payload + 16, 1);
  put_u32(payload + 20, 2);
  put_u32(payload + 24, 0x10203040);
  put_u32(payload + 28, 0x50607080);
  put_u32(payload + 32, 0x90a0b0c0);
  put_u32(payload + 36, 104);
  put_u32(payload + 40, 16);

  uint8_t proof[199];
  char outputs[1024];
  if (make_public_envelope(proof, public_input, payload, outputs) != 0) return 2;
  int original = verify_contract_against_inputs(
      proof, sizeof(proof), public_input, sizeof(public_input), NULL, 0, 3, outputs);

  proof[143 + 24] ^= 1;
  int raw_tamper = verify_contract_against_inputs(
      proof, sizeof(proof), public_input, sizeof(public_input), NULL, 0, 3, outputs);
  put_u32(proof + 195, contract_checksum_u32(proof, 195));
  int checksum_only = verify_contract_against_inputs(
      proof, sizeof(proof), public_input, sizeof(public_input), NULL, 0, 3, outputs);

  payload[24] ^= 1;
  if (make_public_envelope(proof, public_input, payload, outputs) != 0) return 2;
  int recomputed = verify_contract_against_inputs(
      proof, sizeof(proof), public_input, sizeof(public_input), NULL, 0, 3, outputs);

  uint8_t changed_public[104];
  memcpy(changed_public, public_input, 104);
  changed_public[0] ^= 1;
  int changed_statement = verify_contract_against_inputs(
      proof, sizeof(proof), changed_public, sizeof(changed_public), NULL, 0, 3, outputs);
  int impossible_membership = memcmp(public_input, public_input + 32, 32) != 0;
  printf("{\"profile_id\":3,\"witness_supplied\":false,\"aoem_called\":false,"
      "\"gpu_called\":false,\"tree_depth\":0,\"root_differs_from_leaf_commitment\":%s,"
      "\"public_constructed_verifier_rc\":%d,\"raw_tamper_verifier_rc\":%d,"
      "\"checksum_only_repaired_verifier_rc\":%d,\"all_public_digests_recomputed_verifier_rc\":%d,"
      "\"changed_statement_without_rebind_verifier_rc\":%d}\n",
      impossible_membership ? "true" : "false", original, raw_tamper,
      checksum_only, recomputed, changed_statement);
  return impossible_membership && original == 0 && raw_tamper != 0 &&
      checksum_only != 0 && recomputed == 0 && changed_statement != 0 ? 0 : 1;
}
