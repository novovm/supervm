//! Deterministic nonce identity and sequence rules, with no state or environment access.
//! Identity derivation is NOT signature verification. Only authenticated public keys
//! may authorize state changes; chain/parent/configuration validation remains required.
use sha2::{Digest, Sha256};

pub const NATIVE_AUTH_NONCE_IDENTITY_SCHEME_V2: &str = "novovm-native-auth/ed25519-public-key/v2";

fn hash(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    hasher.finalize().into()
}

/// Encodes the already authenticated signer. Address aliases cannot change its bucket.
pub fn signer_nonce_identity_v2(public_key: &[u8; 32]) -> Vec<u8> {
    let mut bytes = NATIVE_AUTH_NONCE_IDENTITY_SCHEME_V2.as_bytes().to_vec();
    bytes.push(0);
    bytes.extend_from_slice(public_key);
    bytes
}

pub fn nonce_identity_digest_v1(chain_id: u64, identity: &[u8]) -> [u8; 32] {
    hash(&[
        b"novovm-native-auth-nonce-identity-v1",
        &chain_id.to_be_bytes(),
        identity,
    ])
}

pub fn nonce_reservation_digest_v1(tx_hash: &[u8; 32], signature: &[u8]) -> [u8; 32] {
    hash(&[
        b"novovm-native-auth-nonce-reservation-v1",
        tx_hash,
        signature,
    ])
}

pub fn nonce_ledger_digest_v1(chain_id: u64, identity: &[u8], nonce: u64) -> [u8; 32] {
    hash(&[
        b"novovm-native-auth-durable-nonce-key-v1",
        &chain_id.to_be_bytes(),
        identity,
        &nonce.to_be_bytes(),
    ])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NonceSequenceErrorV1 {
    Mismatch,
    Exhausted,
}

pub fn nonce_successor_v1(nonce: u64) -> Option<u64> {
    nonce.checked_add(1)
}

/// No mutation on failure; mismatch takes precedence over overflow, as in batches.
pub fn advance_nonce_v1(expected: u64, supplied: u64) -> Result<u64, NonceSequenceErrorV1> {
    if supplied != expected {
        return Err(NonceSequenceErrorV1::Mismatch);
    }
    nonce_successor_v1(expected).ok_or(NonceSequenceErrorV1::Exhausted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_nonce_preserves_legacy_hash_bytes() {
        // Test-only frozen legacy encoding, including NUL separator and big endian numbers.
        let mut identity = b"novovm-native-auth/ed25519-public-key/v2\0".to_vec();
        identity.extend_from_slice(&[0x51; 32]);
        assert_eq!(signer_nonce_identity_v2(&[0x51; 32]), identity);
        for chain in [0u64, 1, 8022, u64::MAX] {
            let old_identity = [
                b"novovm-native-auth-nonce-identity-v1".as_slice(),
                &chain.to_be_bytes(),
                &identity,
            ]
            .concat();
            assert_eq!(
                nonce_identity_digest_v1(chain, &identity),
                <[u8; 32]>::from(Sha256::digest(old_identity))
            );
            for nonce in [0u64, 1, u64::MAX - 1, u64::MAX] {
                let old_ledger = [
                    b"novovm-native-auth-durable-nonce-key-v1".as_slice(),
                    &chain.to_be_bytes(),
                    &identity,
                    &nonce.to_be_bytes(),
                ]
                .concat();
                assert_eq!(
                    nonce_ledger_digest_v1(chain, &identity, nonce),
                    <[u8; 32]>::from(Sha256::digest(old_ledger))
                );
            }
        }
        let old_reservation = [
            b"novovm-native-auth-nonce-reservation-v1".as_slice(),
            &[0x21; 32],
            &[0x43; 96],
        ]
        .concat();
        assert_eq!(
            nonce_reservation_digest_v1(&[0x21; 32], &[0x43; 96]),
            <[u8; 32]>::from(Sha256::digest(old_reservation))
        );
    }

    #[test]
    fn native_nonce_domains_do_not_alias() {
        let a = signer_nonce_identity_v2(&[1; 32]);
        let b = signer_nonce_identity_v2(&[2; 32]);
        assert_ne!(
            nonce_identity_digest_v1(1, &a),
            nonce_identity_digest_v1(2, &a)
        );
        assert_ne!(
            nonce_identity_digest_v1(1, &a),
            nonce_identity_digest_v1(1, &b)
        );
        assert_ne!(
            nonce_ledger_digest_v1(1, &a, 0),
            nonce_ledger_digest_v1(1, &a, 1)
        );
        assert_ne!(
            nonce_reservation_digest_v1(&[1; 32], &[2; 96]),
            nonce_reservation_digest_v1(&[3; 32], &[2; 96])
        );
        assert_ne!(
            nonce_reservation_digest_v1(&[1; 32], &[2; 96]),
            nonce_reservation_digest_v1(&[1; 32], &[3; 96])
        );
    }

    #[test]
    fn native_nonce_sequence_matches_legacy_and_rejects_replay_gap_overflow() {
        for expected in [0, 1, 9, u64::MAX - 1, u64::MAX] {
            for supplied in [0, 1, 9, u64::MAX - 1, u64::MAX] {
                let legacy = if expected != supplied {
                    Err(NonceSequenceErrorV1::Mismatch)
                } else {
                    expected
                        .checked_add(1)
                        .ok_or(NonceSequenceErrorV1::Exhausted)
                };
                assert_eq!(advance_nonce_v1(expected, supplied), legacy);
            }
        }
        assert_eq!(advance_nonce_v1(0, 0), Ok(1));
        assert_eq!(advance_nonce_v1(1, 0), Err(NonceSequenceErrorV1::Mismatch));
        assert_eq!(advance_nonce_v1(1, 2), Err(NonceSequenceErrorV1::Mismatch));
        assert_eq!(
            advance_nonce_v1(u64::MAX, u64::MAX),
            Err(NonceSequenceErrorV1::Exhausted)
        );
    }
}
