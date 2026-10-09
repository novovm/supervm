//! Existing UCA signature_v1 wire contract shared by node and client adapters.
//! Encoding only: does not validate a key, authorize a caller or verify a proof.
//! These bytes reveal the account/root binding and must not be sent to chat peers.
use crate::unified_account::UcaKeyAlgo;
use sha2::{Digest, Sha256};

pub fn derive_primary_key_ref_from_binding_v1(key_algo: UcaKeyAlgo, public_key: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(b"uca-primary-key-ref-v2");
    hasher.update(key_algo.as_str().as_bytes());
    hasher.update([0u8]);
    hasher.update(public_key);
    hasher.finalize().to_vec()
}

/// Preserves the existing node's NUL-delimited, lowercase-hex transcript.
/// Caller must validate input bounds and action before asking a signer to sign.
pub fn primary_key_proof_message_v1(
    account_id: &str,
    action: &str,
    key_algo: UcaKeyAlgo,
    public_key: &[u8],
    primary_key_ref: &[u8],
) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"novovm-uca-primary-key-proof-v1");
    out.push(0);
    out.extend_from_slice(action.as_bytes());
    out.push(0);
    out.extend_from_slice(account_id.as_bytes());
    out.push(0);
    out.extend_from_slice(key_algo.as_str().as_bytes());
    out.push(0);
    append_hex(&mut out, public_key);
    out.push(0);
    append_hex(&mut out, primary_key_ref);
    out
}

fn append_hex(out: &mut Vec<u8>, bytes: &[u8]) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    out.extend_from_slice(b"0x");
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize]);
        out.push(HEX[(byte & 15) as usize]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn existing_transcript_has_exact_domain_delimiters_and_hex() {
        assert_eq!(
            primary_key_proof_message_v1(
                "uca-test",
                "create",
                UcaKeyAlgo::Ed25519,
                &[0xab, 0x01],
                &[0x00, 0xff]
            ),
            b"novovm-uca-primary-key-proof-v1\0create\0uca-test\0ed25519\x000xab01\x000x00ff"
        );
    }

    #[test]
    fn algorithm_account_action_and_key_bindings_change_the_message() {
        let key = [7; 32];
        let reference = derive_primary_key_ref_from_binding_v1(UcaKeyAlgo::Ed25519, &key);
        let base =
            primary_key_proof_message_v1("alice", "create", UcaKeyAlgo::Ed25519, &key, &reference);
        assert_ne!(
            reference,
            derive_primary_key_ref_from_binding_v1(UcaKeyAlgo::Secp256k1, &key)
        );
        for variant in [
            primary_key_proof_message_v1("bob", "create", UcaKeyAlgo::Ed25519, &key, &reference),
            primary_key_proof_message_v1("alice", "rotate", UcaKeyAlgo::Ed25519, &key, &reference),
            primary_key_proof_message_v1("alice", "create", UcaKeyAlgo::Mldsa87, &key, &reference),
            primary_key_proof_message_v1(
                "alice",
                "create",
                UcaKeyAlgo::Ed25519,
                &[8; 32],
                &reference,
            ),
        ] {
            assert_ne!(base, variant);
        }
    }
}
