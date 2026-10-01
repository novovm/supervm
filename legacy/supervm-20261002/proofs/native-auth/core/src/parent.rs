//! V2 binds the nonce lookup to a caller-selected parent root. The verifier must
//! obtain that root from its trusted parent selection, never from the prover.
use crate::{check, AuthInput, AuthJournal};
use novovm_adapter_api::TxIR;
use novovm_protocol::native_parent_nonce::parent_nonce_v3;
use serde::{Deserialize, Serialize};

pub const MAX_PARENT_INPUT_BYTES: usize =
    novovm_protocol::native_parent_nonce::MAX_PARENT_WIRE_BYTES + crate::MAX_INPUT_BYTES;

#[derive(Clone, Serialize, Deserialize)]
pub struct ParentAuthInput {
    pub tx: TxIR,
    pub expected_chain: u64,
    pub parent_root: [u8; 32],
    pub parent_state_wire: Vec<u8>,
}
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParentAuthJournal {
    pub domain: String,
    pub parent_root: [u8; 32],
    pub auth: AuthJournal,
}
pub fn check_parent(input: &ParentAuthInput) -> Result<ParentAuthJournal, &'static str> {
    if input.tx.signature.len() != 96 {
        return Err("signature length");
    }
    let key: [u8; 32] = input.tx.signature[..32]
        .try_into()
        .map_err(|_| "key length")?;
    let expected_nonce = parent_nonce_v3(
        &input.parent_state_wire,
        &input.parent_root,
        input.expected_chain,
        &key,
    )?;
    let auth = check(&AuthInput {
        tx: input.tx.clone(),
        expected_chain: input.expected_chain,
        expected_nonce,
    })?;
    Ok(ParentAuthJournal {
        domain: "novovm-parent-state-signature-nonce-relation/v2".into(),
        parent_root: input.parent_root,
        auth,
    })
}
pub fn decode_and_check_parent(bytes: &[u8]) -> Result<Vec<u8>, &'static str> {
    if bytes.len() > MAX_PARENT_INPUT_BYTES {
        return Err("input too large");
    }
    let (input, rest): (ParentAuthInput, _) =
        postcard::take_from_bytes(bytes).map_err(|_| "invalid input")?;
    if !rest.is_empty() {
        return Err("trailing input");
    }
    postcard::to_allocvec(&check_parent(&input)?).map_err(|_| "journal encoding")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{fixture, resign};
    use novovm_protocol::native_nonce::{
        nonce_identity_digest_v1, signer_nonce_identity_v2, NATIVE_AUTH_NONCE_IDENTITY_SCHEME_V2,
    };
    use novovm_protocol::native_parent_nonce::native_state_wire_root_v3;
    fn text(tag: u8, value: &str) -> Vec<u8> {
        [
            vec![tag],
            (value.len() as u64).to_be_bytes().to_vec(),
            value.as_bytes().to_vec(),
        ]
        .concat()
    }
    fn object(mut fields: Vec<(&str, Vec<u8>)>) -> Vec<u8> {
        fields.sort_by_key(|(key, _)| *key);
        let mut bytes = vec![6];
        bytes.extend_from_slice(&(fields.len() as u64).to_be_bytes());
        for (key, value) in fields {
            bytes.extend_from_slice(&(key.len() as u64).to_be_bytes());
            bytes.extend_from_slice(key.as_bytes());
            bytes.extend_from_slice(&value);
        }
        bytes
    }
    fn parent_fixture() -> ParentAuthInput {
        let mut signed = fixture();
        signed.tx.nonce = 19;
        resign(&mut signed);
        let key: [u8; 32] = signed.tx.signature[..32].try_into().unwrap();
        let digest = nonce_identity_digest_v1(1, &signer_nonce_identity_v2(&key));
        let index: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        // Small synthetic projection tests the relation; real complete Host
        // projections and frozen roots are exercised by node integration tests.
        let wire = object(vec![
            (
                "schema",
                text(4, "novovm-consensus-native-state-projection/v3"),
            ),
            ("account_asset_balances", object(vec![])),
            (
                "module_state_shards",
                object(vec![(
                    "native_execution",
                    object(vec![
                        (
                            "native_auth_nonce_identity_scheme",
                            text(4, NATIVE_AUTH_NONCE_IDENTITY_SCHEME_V2),
                        ),
                        (
                            "native_auth_next_nonces",
                            object(vec![(&index, text(3, "19"))]),
                        ),
                    ]),
                )]),
            ),
        ]);
        ParentAuthInput {
            tx: signed.tx,
            expected_chain: 1,
            parent_root: native_state_wire_root_v3(&wire),
            parent_state_wire: wire,
        }
    }
    #[test]
    fn parent_nonce_is_read_from_committed_state_not_a_free_claim() {
        let input = parent_fixture();
        let output = check_parent(&input).unwrap();
        assert_eq!(output.parent_root, input.parent_root);
        assert_eq!(output.auth.nonce, 19);
        assert_eq!(output.auth.next_nonce, 20);
        let bytes = postcard::to_allocvec(&input).unwrap();
        assert_eq!(
            decode_and_check_parent(&bytes).unwrap(),
            postcard::to_allocvec(&output).unwrap()
        );
        let mut trailing = bytes;
        trailing.push(0);
        assert!(decode_and_check_parent(&trailing).is_err());
    }
    #[test]
    fn altered_parent_or_replayed_signed_nonce_is_rejected() {
        let input = parent_fixture();
        for case in 0..5 {
            let mut bad = input.clone();
            match case {
                0 => bad.parent_root[0] ^= 1,
                1 => {
                    *bad.parent_state_wire.last_mut().unwrap() ^= 1;
                }
                2 => {
                    let mut signed = fixture();
                    signed.tx.nonce = 18;
                    resign(&mut signed);
                    bad.tx = signed.tx;
                }
                3 => bad.expected_chain = 2,
                _ => bad.tx.signature[40] ^= 1,
            }
            assert!(check_parent(&bad).is_err(), "case {case}");
        }
    }
}
