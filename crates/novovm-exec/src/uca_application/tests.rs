//! Actual AOEM signature evidence only, never current grant authority or finality.
use super::*;
use crate::pq_signature::PqVerificationError;
use novovm_protocol::{
    native_query::UcaIdentityIdV1,
    uca_application::{
        ApplicationEncodingError, ApplicationGrantActionV1, ApplicationGrantSelectorV1,
    },
};

fn explicit_facade() -> Result<AoemExecFacade> {
    let path = std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
        .ok_or_else(|| anyhow::anyhow!("explicit NOVOVM_AOEM_TEST_LIBRARY required"))?;
    AoemExecFacade::open(std::path::Path::new(&path), Default::default())
}

fn intent(public_key: &[u8]) -> Result<ApplicationGrantIntentV1> {
    Ok(ApplicationGrantIntentV1 {
        selector: ApplicationGrantSelectorV1 {
            chain_id: 717,
            genesis: [1; 32],
            identity: UcaIdentityIdV1::new("uca:alice").map_err(anyhow::Error::msg)?,
            application: [2; 32],
            service: [3; 32],
            device: [4; 32],
        },
        issuer_key_ref: derive_primary_key_ref_from_binding_v1(UcaKeyAlgo::Mldsa65, public_key)
            .try_into()
            .map_err(|_| anyhow::anyhow!("application issuer key reference must be 32 bytes"))?,
        expected_revision: 0,
        valid_from_height: 10,
        valid_until_height: 20,
        action: ApplicationGrantActionV1::Authorize {
            not_before_height: 10,
            expires_at_height: 100,
        },
    })
}

#[test]
#[ignore = "requires explicit NOVOVM_AOEM_TEST_LIBRARY; actual AOEM ML-DSA-65 batch admission"]
fn application_batch_preserves_order_filters_bad_rows_and_shares_one_owner() -> Result<()> {
    let facade = explicit_facade()?;
    let runtime = facade.mldsa_runtime_v1(AoemMldsaParameterSet::MlDsa65)?;
    let key = runtime.generate_keypair()?;
    let signer = facade.application_grant_signer_v1()?;
    let signed = signer.sign(
        intent(&key.public_key)?,
        &key.public_key,
        &key.expanded_secret_key,
    )?;
    let mut revoke_intent = signed.intent.clone();
    revoke_intent.expected_revision = 1;
    revoke_intent.action = ApplicationGrantActionV1::Revoke;
    let revoke = signer.sign(revoke_intent, &key.public_key, &key.expanded_secret_key)?;
    let mut bad_signature = signed.clone();
    bad_signature.signature[0] ^= 1;
    let mut bad_signature_length = signed.clone();
    bad_signature_length.signature.pop();
    let mut bad_key_length = signed.clone();
    bad_key_length.public_key.pop();
    let mut bad_intent = signed.clone();
    bad_intent.intent.valid_until_height = bad_intent.intent.valid_from_height;
    let mut wrong_issuer = signed.clone();
    wrong_issuer.intent.issuer_key_ref[0] ^= 1;
    // This signature is cryptographically valid; the application issuer binding
    // must still reject it before submitting any AOEM verification row.
    wrong_issuer.signature = runtime.sign(
        &key.public_key,
        &key.expanded_secret_key,
        &wrong_issuer.intent.signing_bytes()?,
        APPLICATION_CONTEXT_V1,
    )?;
    let verifier =
        facade.application_grant_verifier_with_backend_v1(MldsaVerificationBackendV1::Cpu)?;
    let before = verifier.last_execution_v1().expect("qualified owner");
    let cloned = verifier.clone();
    assert_eq!(cloned.last_execution_v1(), Some(before));
    assert!(verifier.verify(wrong_issuer.clone()).is_err());
    assert_eq!(verifier.last_execution_v1(), Some(before));
    let outcomes = cloned.verify_batch(vec![
        signed.clone(),
        bad_signature,
        bad_signature_length.clone(),
        wrong_issuer.clone(),
        revoke.clone(),
        bad_key_length,
        bad_intent,
    ])?;
    assert_eq!(outcomes.len(), 7);
    assert_eq!(
        outcomes[1]
            .as_ref()
            .err()
            .expect("bad signature")
            .downcast_ref::<PqVerificationError>(),
        Some(&PqVerificationError::InvalidSignature)
    );
    for index in [2, 5, 6] {
        assert!(outcomes[index]
            .as_ref()
            .err()
            .expect("malformed row")
            .is::<ApplicationEncodingError>());
    }
    assert!(outcomes[3]
        .as_ref()
        .err()
        .expect("wrong issuer")
        .to_string()
        .contains("issuer key reference mismatch"));
    let expected = [
        Some(&signed.intent),
        None,
        None,
        None,
        Some(&revoke.intent),
        None,
        None,
    ];
    for (result, expected) in outcomes.into_iter().zip(expected) {
        match expected {
            Some(intent) => assert_eq!(result?.intent(), intent),
            None => assert!(result.is_err()),
        }
    }
    let after = verifier.last_execution_v1().expect("completed batch");
    assert_eq!(after.sequence, before.sequence + 1);
    assert_eq!(after.rows, 3);
    assert_eq!(after.valid_count, 2);
    assert_eq!(after.backend_kind, 1);
    assert_eq!(cloned.last_execution_v1(), Some(after));

    assert!(verifier.verify_batch(Vec::new())?.is_empty());
    let rejected = verifier.verify_batch(vec![wrong_issuer, bad_signature_length.clone()])?;
    assert_eq!(rejected.len(), 2);
    assert!(rejected.iter().all(Result::is_err));
    let at_limit = verifier.verify_batch(vec![
        bad_signature_length.clone();
        MLDSA_VERIFY_BATCH_MAX_ITEMS
    ])?;
    assert_eq!(at_limit.len(), MLDSA_VERIFY_BATCH_MAX_ITEMS);
    assert!(at_limit.iter().all(Result::is_err));
    assert!(verifier
        .verify_batch(vec![signed; MLDSA_VERIFY_BATCH_MAX_ITEMS + 1])
        .is_err());
    assert_eq!(verifier.last_execution_v1(), Some(after));
    Ok(())
}

#[test]
#[ignore = "requires explicit NOVOVM_AOEM_TEST_LIBRARY; actual AOEM ML-DSA-65 fact binding"]
fn application_fact_rejects_tampering_purpose_transplants_and_resigned_bytes() -> Result<()> {
    let facade = explicit_facade()?;
    let runtime = facade.mldsa_runtime_v1(AoemMldsaParameterSet::MlDsa65)?;
    let mut key = runtime.generate_keypair()?;
    let signer = facade.application_grant_signer_v1()?;
    let signed = signer.sign(
        intent(&key.public_key)?,
        &key.public_key,
        &key.expanded_secret_key,
    )?;
    let verifier = facade.application_grant_verifier_v1()?;
    let checked = verifier.verify(signed.clone())?;
    let fact = checked.into_signature_fact()?;
    let cloned = fact.clone();
    let before = verifier.last_execution_v1();
    assert_eq!(fact.intent(), &signed.intent);
    assert_eq!(
        cloned.reattach(signed.clone())?.signed.encode()?,
        signed.encode()?
    );
    let owned_bytes = std::mem::size_of::<CheckedApplicationGrantV1>()
        + signed.public_key.capacity()
        + signed.signature.capacity();
    assert!(owned_bytes - std::mem::size_of::<ApplicationGrantSignatureFactV1>() > 5_000);
    for mutation in 0..17 {
        let mut changed = signed.clone();
        match mutation {
            0 => changed.intent.selector.chain_id += 1,
            1 => changed.intent.selector.genesis[0] ^= 1,
            2 => {
                changed.intent.selector.identity =
                    UcaIdentityIdV1::new("uca:bob").map_err(anyhow::Error::msg)?
            }
            3 => changed.intent.selector.application[0] ^= 1,
            4 => changed.intent.selector.service[0] ^= 1,
            5 => changed.intent.selector.device[0] ^= 1,
            6 => changed.intent.issuer_key_ref[0] ^= 1,
            7 => changed.intent.expected_revision += 1,
            8 => changed.intent.valid_from_height -= 1,
            9 => changed.intent.valid_until_height += 1,
            10 => changed.intent.action = ApplicationGrantActionV1::Revoke,
            11 => {
                changed.intent.action = ApplicationGrantActionV1::Authorize {
                    not_before_height: 11,
                    expires_at_height: 100,
                }
            }
            12 => {
                changed.intent.action = ApplicationGrantActionV1::Authorize {
                    not_before_height: 10,
                    expires_at_height: 101,
                }
            }
            13 => changed.public_key[0] ^= 1,
            14 => changed.signature[0] ^= 1,
            15 => {
                changed.public_key.pop();
            }
            _ => {
                changed.signature.pop();
            }
        }
        assert!(fact.reattach(changed).is_err(), "mutation {mutation}");
    }
    let mut wrong_purpose = signed.clone();
    wrong_purpose.signature = runtime.sign(
        &key.public_key,
        &key.expanded_secret_key,
        &signed.intent.signing_bytes()?,
        novovm_protocol::uca_spend::SPEND_CONTEXT_V3,
    )?;
    assert!(fact.reattach(wrong_purpose.clone()).is_err());
    assert_eq!(verifier.last_execution_v1(), before);
    assert!(verifier.verify(wrong_purpose).is_err());

    // FIPS 204 skEncode (Algorithm 24) places the private signing seed K at
    // bytes 32..64. Change only K in this ephemeral, zeroizing test key: the
    // deterministic AOEM signer then produces another valid signature for the
    // SAME public key, intent and purpose, without changing the algorithm.
    // https://nvlpubs.nist.gov/nistpubs/fips/nist.fips.204.pdf
    key.expanded_secret_key[32] ^= 1;
    let resigned = signer.sign(
        signed.intent.clone(),
        &key.public_key,
        &key.expanded_secret_key,
    )?;
    assert_eq!(resigned.intent, signed.intent);
    assert_eq!(resigned.public_key, signed.public_key);
    assert_ne!(resigned.signature, signed.signature);
    let resigned_fact = verifier.verify(resigned.clone())?.into_signature_fact()?;
    let before_reattach = verifier.last_execution_v1();
    assert!(fact.reattach(resigned.clone()).is_err());
    assert!(resigned_fact.reattach(signed.clone()).is_err());
    assert!(resigned_fact.reattach(resigned).is_ok());
    assert_eq!(cloned.reattach(signed.clone())?.signed, signed);
    assert_eq!(verifier.last_execution_v1(), before_reattach);
    Ok(())
}
