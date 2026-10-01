//! A signature/nonce relation only. No authenticated parent state or execution proof.
use ed25519_dalek::{Signature, VerifyingKey};
use novovm_adapter_api::{
    native_signing::{native_signer_matches_v1, tx_signing_message_v1},
    TxIR,
};
use novovm_protocol::native_nonce::{
    advance_nonce_v1, nonce_identity_digest_v1, signer_nonce_identity_v2,
};
use serde::{Deserialize, Serialize};
pub mod parent;

pub const MAX_INPUT_BYTES: usize = 64 * 1024;

#[derive(Clone, Serialize, Deserialize)]
pub struct AuthInput {
    pub tx: TxIR,
    // These are public claims, NOT trusted state merely because supplied as input.
    pub expected_chain: u64,
    pub expected_nonce: u64,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthJournal {
    pub domain: String,
    pub message: [u8; 32],
    pub public_key: [u8; 32],
    pub nonce_identity: [u8; 32],
    pub chain: u64,
    pub nonce: u64,
    pub next_nonce: u64,
}

pub fn check(input: &AuthInput) -> Result<AuthJournal, &'static str> {
    let tx = &input.tx;
    if input.expected_chain == 0 || tx.chain_id != input.expected_chain {
        return Err("chain mismatch");
    }
    if tx.signature.len() != 96 {
        return Err("signature length");
    }
    let key_bytes: [u8; 32] = tx.signature[..32].try_into().map_err(|_| "key length")?;
    let key = VerifyingKey::from_bytes(&key_bytes).map_err(|_| "invalid public key")?;
    let signature = Signature::from_slice(&tx.signature[32..]).map_err(|_| "invalid signature")?;
    let message = tx_signing_message_v1(tx);
    // Intentionally stricter than the legacy CPU fallback: no weak-key proofs.
    key.verify_strict(&message, &signature)
        .map_err(|_| "invalid signature")?;
    if !native_signer_matches_v1(&tx.from, &key_bytes) {
        return Err("signer mismatch");
    }
    let next_nonce = advance_nonce_v1(input.expected_nonce, tx.nonce)
        .map_err(|_| "nonce mismatch or exhausted")?;
    Ok(AuthJournal {
        domain: "novovm-signature-nonce-relation/v1".into(),
        message,
        public_key: key_bytes,
        nonce_identity: nonce_identity_digest_v1(
            tx.chain_id,
            &signer_nonce_identity_v2(&key_bytes),
        ),
        chain: tx.chain_id,
        nonce: tx.nonce,
        next_nonce,
    })
}

pub fn decode_and_check(bytes: &[u8]) -> Result<Vec<u8>, &'static str> {
    if bytes.len() > MAX_INPUT_BYTES {
        return Err("input too large");
    }
    let (input, rest): (AuthInput, _) =
        postcard::take_from_bytes(bytes).map_err(|_| "invalid input")?;
    if !rest.is_empty() {
        return Err("trailing input");
    }
    postcard::to_allocvec(&check(&input)?).map_err(|_| "journal encoding")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use novovm_adapter_api::{native_signing::native_signer_address_v1, TxType};
    pub(super) fn fixture() -> AuthInput {
        let key = SigningKey::from_bytes(&[7; 32]);
        let mut tx = TxIR {
            hash: vec![0x41; 32],
            from: native_signer_address_v1(&key.verifying_key().to_bytes()).to_vec(),
            account_id: None,
            fee_owner_account_id: None,
            nonce_owner_account_id: None,
            to: Some(vec![3; 20]),
            value: 1,
            gas_limit: 21_000,
            gas_price: 1,
            nonce: 0,
            data: vec![],
            signature: vec![],
            chain_id: 1,
            tx_type: TxType::Transfer,
            execution_policy: Default::default(),
            evm_access_list: vec![],
            source_chain: None,
            target_chain: None,
        };
        tx.signature = key.verifying_key().to_bytes().to_vec();
        tx.signature
            .extend_from_slice(&key.sign(&tx_signing_message_v1(&tx)).to_bytes());
        AuthInput {
            tx,
            expected_chain: 1,
            expected_nonce: 0,
        }
    }
    #[test]
    fn valid_relation_and_public_claims_are_bound() {
        let input = fixture();
        let journal = check(&input).unwrap();
        assert_eq!(journal.chain, 1);
        assert_eq!(journal.nonce, 0);
        assert_eq!(journal.next_nonce, 1);
        let bytes = postcard::to_allocvec(&input).unwrap();
        assert_eq!(
            decode_and_check(&bytes).unwrap(),
            postcard::to_allocvec(&journal).unwrap()
        );
        let mut alias = input.clone();
        alias.tx.from = SigningKey::from_bytes(&[7; 32])
            .verifying_key()
            .to_bytes()
            .to_vec();
        resign(&mut alias);
        let alias_journal = check(&alias).unwrap();
        assert_eq!(journal.nonce_identity, alias_journal.nonce_identity);
        assert_ne!(journal.message, alias_journal.message);
    }
    pub(super) fn resign(input: &mut AuthInput) {
        let key = SigningKey::from_bytes(&[7; 32]);
        input.tx.signature = key.verifying_key().to_bytes().to_vec();
        input
            .tx
            .signature
            .extend_from_slice(&key.sign(&tx_signing_message_v1(&input.tx)).to_bytes());
    }
    #[test]
    fn invalid_signature_signer_chain_nonce_and_overflow_are_rejected() {
        for case in 0..9 {
            let mut input = fixture();
            match case {
                0 => input.tx.signature[40] ^= 1,
                1 => input.tx.value += 1,
                2 => input.expected_chain = 2,
                3 => {
                    input.expected_chain = 0;
                    input.tx.chain_id = 0;
                    resign(&mut input);
                }
                4 => input.expected_nonce = 1,
                5 => {
                    input.tx.nonce = u64::MAX;
                    input.expected_nonce = u64::MAX;
                    resign(&mut input);
                }
                6 => {
                    input.tx.from = vec![4; 20];
                    resign(&mut input);
                }
                7 => {
                    input.tx.signature.pop();
                }
                _ => {
                    input.tx.signature.push(0);
                }
            }
            assert!(check(&input).is_err(), "accepted case {case}");
        }
    }
    #[test]
    fn malformed_and_trailing_input_are_rejected() {
        assert!(decode_and_check(&[]).is_err());
        assert!(decode_and_check(&vec![0; MAX_INPUT_BYTES + 1]).is_err());
        let mut bytes = postcard::to_allocvec(&fixture()).unwrap();
        bytes.push(0);
        assert_eq!(decode_and_check(&bytes), Err("trailing input"));
    }
    #[test]
    fn weak_identity_key_cannot_authorize_proof() {
        let mut input = fixture();
        // Identity point A and R with S=0: strict verification must reject.
        let mut identity = [0u8; 32];
        identity[0] = 1;
        input.tx.from = identity.to_vec();
        input.tx.signature = identity.to_vec();
        input.tx.signature.extend_from_slice(&identity);
        input.tx.signature.extend_from_slice(&[0; 32]);
        assert!(check(&input).is_err());
    }
}
